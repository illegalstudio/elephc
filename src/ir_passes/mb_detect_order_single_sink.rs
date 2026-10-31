//! Purpose:
//! Specializes `mb_detect_order()` when its argument is a one-element UTF-8 array literal,
//! replacing the array construction and bridge call with one allocation-free catalog setter.
//!
//! Called from:
//! - The EIR fixed-point pass driver after the mbstring length sinks.
//!
//! Key details:
//! - Only the exact `["UTF-8"]` literal shape is rewritten; every other list, dynamic value, or
//!   invalid encoding keeps the shared bridge and its PHP diagnostics.
//! - The pass proves the temporary array and its exception guard have no other observer before
//!   removing them, so no ownership or unwind edge is dropped.
//! - The replacement keeps the original `array|bool` result representation and boxes the catalog
//!   setter's boolean exactly where the bridge did.

use crate::ir::{
    DataPool, Function, Immediate, InstId, Op, RuntimeCallTarget, RuntimeFnId, ValueId,
};
use super::driver::IrPass;
use super::rewrite::{neutralize_to_nop, replace_all_uses};

/// Rewrites one-element UTF-8 `mb_detect_order()` calls.
pub struct MbDetectOrderSingleSink;

impl IrPass for MbDetectOrderSingleSink {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "mb_detect_order_single_sink"
    }

    /// Skips functions with no `mb_detect_order()` call.
    fn is_applicable(&self, function: &Function) -> bool {
        function
            .instructions
            .iter()
            .any(|inst| direct_target(inst) == Some(RuntimeFnId::MbDetectOrder))
    }

    /// Specializes every independently proven literal call.
    fn run(&self, function: &mut Function, data: &mut DataPool) -> bool {
        let candidates: Vec<InstId> = function
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, inst)| direct_target(inst) == Some(RuntimeFnId::MbDetectOrder))
            .map(|(raw, _)| InstId::from_raw(raw as u32))
            .collect();
        let mut changed = false;
        for candidate in candidates {
            if specialize(function, candidate, data) {
                changed = true;
            }
        }
        changed
    }
}

/// Returns the runtime target of a direct or profiled call instruction.
fn direct_target(inst: &crate::ir::Instruction) -> Option<RuntimeFnId> {
    match inst.immediate {
        Some(Immediate::RuntimeCall(
            RuntimeCallTarget::Function(target)
                | RuntimeCallTarget::ProfiledFunction { target, .. },
        )) => Some(target),
        _ => None,
    }
}

/// Returns whether `inst` is the shared-owned-value exception guard.
fn is_owned_guard(inst: &crate::ir::Instruction) -> bool {
    matches!(
        inst.immediate,
        Some(Immediate::RuntimeCall(RuntimeCallTarget::ExceptionGuardOwned))
    )
}

/// Returns whether `inst` is the matching owned-value exception unguard.
fn is_owned_unguard(inst: &crate::ir::Instruction) -> bool {
    matches!(
        inst.immediate,
        Some(Immediate::RuntimeCall(RuntimeCallTarget::ExceptionUnguardOwned))
    )
}

/// Returns the defining instruction and identifier for one SSA value.
fn defining_instruction_with_id(
    function: &Function,
    value: ValueId,
) -> Option<(InstId, &crate::ir::Instruction)> {
    let definition = function.value(value)?;
    let crate::ir::ValueDef::Instruction { inst, .. } = definition.def else {
        return None;
    };
    Some((inst, function.instruction(inst)?))
}

/// Returns the block containing one instruction.
fn block_of(function: &Function, id: InstId) -> Option<crate::ir::BlockId> {
    function.blocks.iter().enumerate().find_map(|(index, block)| {
        block
            .instructions
            .contains(&id)
            .then(|| crate::ir::BlockId::from_raw(index as u32))
    })
}

/// Specializes one detected call after proving the array and guard have no other observers.
fn specialize(function: &mut Function, call_id: InstId, data: &DataPool) -> bool {
    let Some(call) = function.instruction(call_id) else {
        return false;
    };
    if call.operands.len() != 1 {
        return false;
    }
    let array_value = call.operands[0];
    let Some((array_source_id, array_source)) = defining_instruction_with_id(function, array_value)
    else {
        return false;
    };
    // The call operand is normally the acquired temporary array; accept a direct array too.
    let (array_new_value, acquire_id) = if array_source.op == Op::Acquire
        && array_source.operands.len() == 1
    {
        (array_source.operands[0], Some(array_source_id))
    } else if array_source.op == Op::ArrayNew {
        (array_value, None)
    } else {
        return false;
    };
    let Some((array_new_id, array_new)) = defining_instruction_with_id(function, array_new_value)
    else {
        return false;
    };
    if array_new.op != Op::ArrayNew {
        return false;
    }
    let Some(call_block) = block_of(function, call_id) else {
        return false;
    };

    let mut push = None;
    let mut release_array = None;
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !inst.operands.contains(&array_new_value) {
            continue;
        }
        let id = InstId::from_raw(raw as u32);
        if inst.op == Op::ArrayPush && inst.operands.len() == 2 && inst.operands[0] == array_new_value
        {
            if push.replace((id, inst.operands[1])).is_some() {
                return false;
            }
            continue;
        }
        if inst.op == Op::Acquire && inst.operands.as_slice() == [array_new_value] {
            if acquire_id != Some(id) {
                return false;
            }
            continue;
        }
        if inst.op == Op::Release && inst.operands.as_slice() == [array_new_value] {
            release_array = Some(id);
            continue;
        }
        return false;
    }
    let Some((push_id, element)) = push else {
        return false;
    };
    let Some((_, element_inst)) = defining_instruction_with_id(function, element) else {
        return false;
    };
    let (Op::ConstStr, Some(Immediate::Data(data_id))) = (element_inst.op, &element_inst.immediate)
    else {
        return false;
    };
    // The rewrite only proves the common UTF-8 literal; other encodings keep the bridge.
    let Some(name) = data.strings.get(data_id.as_raw() as usize) else {
        return false;
    };
    if !name.eq_ignore_ascii_case("UTF-8") {
        return false;
    }

    let mut guard = None;
    let mut release_value = None;
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !inst.operands.contains(&array_value) {
            continue;
        }
        let id = InstId::from_raw(raw as u32);
        if id == call_id {
            continue;
        }
        if is_owned_guard(inst) {
            if guard.replace((id, inst.result)).is_some() || inst.result.is_none() {
                return false;
            }
            continue;
        }
        if inst.op == Op::Release && inst.operands.as_slice() == [array_value] {
            release_value = Some(id);
            continue;
        }
        return false;
    }
    let Some((guard_id, guard_result)) = guard else {
        return false;
    };
    let Some(guard_result) = guard_result else {
        return false;
    };
    let mut unguard = None;
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !inst.operands.contains(&guard_result) {
            continue;
        }
        let id = InstId::from_raw(raw as u32);
        if is_owned_unguard(inst) {
            if unguard.replace(id).is_some() {
                return false;
            }
            continue;
        }
        return false;
    }
    let Some(unguard_id) = unguard else {
        return false;
    };

    for block in &function.blocks {
        if block.terminator.as_ref().is_some_and(|term| {
            term.used_values().contains(&array_new_value)
                || term.used_values().contains(&array_value)
                || term.used_values().contains(&guard_result)
        }) {
            return false;
        }
    }
    for id in [
        array_new_id,
        push_id,
        guard_id,
        unguard_id,
        call_id,
        release_array.unwrap_or(call_id),
        release_value.unwrap_or(call_id),
    ] {
        if id != call_id && block_of(function, id) != Some(call_block) {
            return false;
        }
    }

    let Some(call_result) = function.instruction(call_id).and_then(|call| call.result) else {
        return false;
    };
    let mut cast = None;
    let mut call_releases = Vec::new();
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !inst.operands.contains(&call_result) {
            continue;
        }
        let id = InstId::from_raw(raw as u32);
        if inst.op == Op::Cast
            && inst.operands.as_slice() == [call_result]
            && inst.result_php_type.codegen_repr() == crate::types::PhpType::Int
        {
            let Some(result) = inst.result else {
                return false;
            };
            if cast.replace((id, result)).is_some() {
                return false;
            }
            continue;
        }
        if inst.op == Op::Release && inst.operands.as_slice() == [call_result] {
            call_releases.push(id);
            continue;
        }
        return false;
    }
    let Some(call) = function.instruction_mut(call_id) else {
        return false;
    };
    call.immediate = Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
        RuntimeFnId::MbDetectOrderSingle,
    )));
    call.operands = vec![element];
    call.effects = RuntimeFnId::MbDetectOrderSingle.intrinsic_effects();
    if let Some((cast_id, cast_result)) = cast {
        call.result_type = crate::ir::IrType::I64;
        call.result_php_type = crate::types::PhpType::Int;
        call.result_ownership = crate::ir::Ownership::NonHeap;
        if let Some(value) = function.value_mut(call_result) {
            value.ir_type = crate::ir::IrType::I64;
            value.php_type = crate::types::PhpType::Int;
            value.ownership = crate::ir::Ownership::NonHeap;
        }
        replace_all_uses(
            function,
            &std::collections::HashMap::from([(cast_result, call_result)]),
        );
        for id in std::iter::once(cast_id).chain(call_releases) {
            if let Some(inst) = function.instruction_mut(id) {
                neutralize_to_nop(inst);
            }
        }
    }
    for id in std::iter::once(array_new_id)
        .chain(std::iter::once(push_id))
        .chain(acquire_id)
        .chain(std::iter::once(guard_id))
        .chain(std::iter::once(unguard_id))
        .chain(release_array)
        .chain(release_value)
    {
        if let Some(inst) = function.instruction_mut(id) {
            neutralize_to_nop(inst);
        }
    }
    true
}
