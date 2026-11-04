//! Purpose:
//! Folds `strlen((string)mb_convert_variables("UTF-8", "UTF-8", $array))` to the
//! encoding name's length when the source array and the temporary destination are
//! otherwise unobserved.
//!
//! Called from:
//! - The EIR fixed-point pass driver after the by-reference array setup is lowered.
//!
//! Key details:
//! - Identical UTF-8 source and target encodings make the call return `"UTF-8"` for any
//!   array contents; only the array mutation is skipped, so every observer of the source
//!   or destination array must be this call (or its ownership scaffolding).
//! - The source slot must be a plain local with one entry store, only ref-cell promotion and
//!   one load in the loop, plus scope cleanup. A second load or any write keeps the call.
//! - The destination array is a lowering temporary: exactly the call and one release may use
//!   it, and both are retired together so no allocation is abandoned.

use crate::ir::{
    DataPool, Function, Immediate, InstId, Op, Ownership, RuntimeCallTarget, RuntimeFnId, ValueId,
};
use crate::types::PhpType;

use super::driver::IrPass;
use super::rewrite::{defining_instruction, neutralize_to_nop};
use super::string_len_sink::const_string_value;

/// Folds the identical-UTF-8 `mb_convert_variables()` length observation.
pub struct MbConvertVariablesLenSink;

impl IrPass for MbConvertVariablesLenSink {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "mb_convert_variables_len_sink"
    }

    /// Skips functions without a convert-variables call.
    fn is_applicable(&self, function: &Function) -> bool {
        function.instructions.iter().any(|inst| {
            matches!(
                direct_target(inst),
                Some(RuntimeFnId::MbConvertVariables)
            )
        })
    }

    /// Folds every independently proven identical-encoding call.
    fn run(&self, function: &mut Function, data: &mut DataPool) -> bool {
        let candidates: Vec<InstId> = function
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, inst)| inst.op == Op::StrLen && inst.operands.len() == 1)
            .map(|(raw, _)| InstId::from_raw(raw as u32))
            .collect();
        let mut changed = false;
        for candidate in candidates {
            if fold(function, data, candidate) {
                changed = true;
            }
        }
        changed
    }
}

/// Returns the direct or profiled runtime target for one call.
fn direct_target(inst: &crate::ir::Instruction) -> Option<RuntimeFnId> {
    match inst.immediate {
        Some(Immediate::RuntimeCall(
            RuntimeCallTarget::Function(target)
                | RuntimeCallTarget::ProfiledFunction { target, .. },
        )) => Some(target),
        _ => None,
    }
}

/// Folds one length chain when both arrays are unobserved and the encodings match.
fn fold(function: &mut Function, data: &DataPool, length_id: InstId) -> bool {
    let Some(length) = function.instruction(length_id) else {
        return false;
    };
    let Some(cast_value) = length.operands.first().copied() else {
        return false;
    };
    let Some(cast) = defining_instruction(function, cast_value) else {
        return false;
    };
    if cast.op != Op::Cast || cast.operands.len() != 1 {
        return false;
    }
    let call_value = cast.operands[0];
    let Some(call) = defining_instruction(function, call_value) else {
        return false;
    };
    if direct_target(call) != Some(RuntimeFnId::MbConvertVariables) || call.operands.len() != 4 {
        return false;
    }
    let Some(from_encoding) = const_string_value(function, data, call.operands[0]) else {
        return false;
    };
    let Some(to_encoding) = const_string_value(function, data, call.operands[1]) else {
        return false;
    };
    if !from_encoding.eq_ignore_ascii_case("UTF-8") || !to_encoding.eq_ignore_ascii_case("UTF-8") {
        return false;
    }
    let source_value = call.operands[2];
    let Some(source_load) = defining_instruction(function, source_value) else {
        return false;
    };
    if source_load.op != Op::LoadRefCell {
        return false;
    }
    let Some(Immediate::LocalSlot(slot)) = source_load.immediate else {
        return false;
    };
    let destination_value = call.operands[3];
    let Some(destination) = defining_instruction(function, destination_value) else {
        return false;
    };
    if destination.op != Op::ArrayNew {
        return false;
    }

    // The source slot owns exactly one entry store and is only promoted/loaded thereafter.
    let Some(entry) = function.blocks.first() else {
        return false;
    };
    let mut stores = 0;
    let mut source_loads = 0;
    for (raw, inst) in function.instructions.iter().enumerate() {
        let id = InstId::from_raw(raw as u32);
        match inst.immediate {
            Some(Immediate::LocalSlot(found)) if found == slot => match inst.op {
                Op::StoreLocal if inst.operands.len() == 1 => {
                    if !entry.instructions.contains(&id) {
                        return false;
                    }
                    stores += 1;
                }
                Op::LoadRefCell => source_loads += 1,
                Op::ReleaseLocalSlot => {}
                _ => return false,
            },
            Some(Immediate::LocalSlotPair { first, second })
                if first == slot || second == slot =>
            {
                if inst.op != Op::PromoteLocalRefCell {
                    return false;
                }
            }
            _ => {}
        }
    }
    if stores != 1 || source_loads != 1 {
        return false;
    }

    // Every consumer of the three temporaries must be the exact instruction that consumes
    // them, plus the single ownership release. Anything else would observe the skipped copy.
    if count_uses_outside(function, source_value, &[Op::RuntimeCall, Op::Release]) > 0
        || count_op_uses(function, source_value, Op::RuntimeCall) != 1
    {
        return false;
    }
    let destination_release = single_release(function, destination_value);
    let call_release = single_release(function, call_value);
    let cast_release = single_release(function, cast_value);
    let (Some(destination_release), Some(call_release), Some(cast_release)) =
        (destination_release, call_release, cast_release)
    else {
        return false;
    };
    if count_uses_outside(function, call_value, &[Op::Cast, Op::Release]) > 0
        || count_op_uses(function, call_value, Op::Cast) != 1
        || count_uses_outside(function, cast_value, &[Op::StrLen, Op::Release]) > 0
        || count_op_uses(function, cast_value, Op::StrLen) != 1
        || count_uses_outside(function, destination_value, &[Op::RuntimeCall, Op::Release]) > 0
        || count_op_uses(function, destination_value, Op::RuntimeCall) != 1
    {
        return false;
    }

    let Some(length) = function.instruction_mut(length_id) else {
        return false;
    };
    let Some(result) = length.result else {
        return false;
    };
    length.op = Op::ConstI64;
    length.operands.clear();
    length.immediate = Some(Immediate::I64(5));
    length.result_type = crate::ir::IrType::I64;
    length.result_php_type = PhpType::Int;
    length.result_ownership = Ownership::NonHeap;
    length.effects = Op::ConstI64.default_effects();
    if let Some(value) = function.value_mut(result) {
        value.ir_type = crate::ir::IrType::I64;
        value.php_type = PhpType::Int;
        value.ownership = Ownership::NonHeap;
    }

    let Some(call_id) = defining_id(function, call_value) else {
        return false;
    };
    let Some(cast_id) = defining_id(function, cast_value) else {
        return false;
    };
    let Some(destination_id) = defining_id(function, destination_value) else {
        return false;
    };
    neutralize_encoding_staging(function, call_value);
    for id in [call_id, cast_id, destination_id, call_release, cast_release, destination_release] {
        if let Some(inst) = function.instruction_mut(id) {
            neutralize_to_nop(inst);
        }
    }
    true
}

/// Retires the exception-guard, acquire and release scaffolding around the removed call's
/// two encoding operands. The folded call no longer needs those guards, and the constants
/// themselves stay in the entry block.
fn neutralize_encoding_staging(function: &mut Function, call_value: ValueId) {
    let Some(call) = defining_instruction(function, call_value) else {
        return;
    };
    let operands = call.operands.clone();
    for value in operands.into_iter().take(2) {
        let acquire_id = defining_id(function, value);
        let source = acquire_id
            .and_then(|id| function.instruction(id))
            .filter(|inst| inst.op == Op::Acquire && inst.operands.len() == 1)
            .map(|inst| inst.operands[0])
            .unwrap_or(value);
        let guards: Vec<(InstId, Option<ValueId>)> = function
            .instructions
            .iter()
            .enumerate()
            .filter_map(|(raw, inst)| {
                if !matches!(
                    inst.immediate,
                    Some(Immediate::RuntimeCall(RuntimeCallTarget::ExceptionGuardOwned))
                ) {
                    return None;
                }
                (inst.operands.first() == Some(&value)
                    || inst.operands.first() == Some(&source))
                .then_some((InstId::from_raw(raw as u32), inst.result))
            })
            .collect();
        for (guard_id, token) in guards {
            if let Some(token) = token {
                for inst in &mut function.instructions {
                    if matches!(
                        inst.immediate,
                        Some(Immediate::RuntimeCall(RuntimeCallTarget::ExceptionUnguardOwned))
                    ) && inst.operands.first() == Some(&token)
                    {
                        neutralize_to_nop(inst);
                    }
                }
            }
            if let Some(inst) = function.instruction_mut(guard_id) {
                neutralize_to_nop(inst);
            }
        }
        if let Some(id) = acquire_id {
            if function.instruction(id).is_some_and(|inst| inst.op == Op::Acquire) {
                if let Some(inst) = function.instruction_mut(id) {
                    neutralize_to_nop(inst);
                }
            }
        }
        for inst in &mut function.instructions {
            if inst.op == Op::Release
                && inst.operands.len() == 1
                && inst.operands[0] == value
            {
                neutralize_to_nop(inst);
            }
        }
    }
}

/// Returns the single `Release` instruction using `value`, if there is exactly one.
fn single_release(function: &Function, value: ValueId) -> Option<InstId> {
    let mut releases = function.instructions.iter().enumerate().filter_map(|(raw, inst)| {
        (inst.op == Op::Release && inst.operands.len() == 1 && inst.operands[0] == value)
            .then_some(InstId::from_raw(raw as u32))
    });
    let first = releases.next()?;
    releases.next().is_none().then_some(first)
}

/// Counts uses of `value` by an instruction with the given opcode.
fn count_op_uses(function: &Function, value: ValueId, op: Op) -> usize {
    function
        .instructions
        .iter()
        .filter(|inst| inst.op == op && inst.operands.contains(&value))
        .count()
}

/// Counts uses of `value` whose instruction op is not in `allowed`.
fn count_uses_outside(function: &Function, value: ValueId, allowed: &[Op]) -> usize {
    function
        .instructions
        .iter()
        .filter(|inst| inst.operands.contains(&value) && !allowed.contains(&inst.op))
        .count()
}

/// Returns the defining instruction identifier for one SSA value.
fn defining_id(function: &Function, value: ValueId) -> Option<InstId> {
    let definition = function.value(value)?;
    let crate::ir::ValueDef::Instruction { inst, .. } = definition.def else {
        return None;
    };
    Some(inst)
}
