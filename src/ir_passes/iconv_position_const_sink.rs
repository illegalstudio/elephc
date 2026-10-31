//! Purpose:
//! Folds `iconv_strpos()`/`iconv_strrpos()` to a boxed integer when both operands are
//! immutable compile-time ASCII locals under explicit UTF-8.
//!
//! Called from:
//! - The EIR fixed-point pass driver after the string length sinks.
//!
//! Key details:
//! - ASCII makes PHP's character offset equal to the byte offset, so no Unicode index table
//!   is needed; non-ASCII operands and other encodings stay on the iconv bridge.
//! - Only a literal zero `iconv_strpos()` offset is folded, avoiding PHP's negative-offset and
//!   out-of-range diagnostics; an empty needle or a miss stays on the runtime path.
//! - The runtime call becomes `MixedBox(const_i64(position))`, preserving the declared
//!   `int|false` representation while deleting the bridge call and its dead operand loads.

use crate::ir::{
    BlockId, DataPool, Function, Immediate, InstId, Instruction, IrHeapKind, IrType, Op,
    Ownership, PassOrigin, RuntimeCallTarget, RuntimeFnId, Value, ValueDef, ValueId,
};
use crate::types::PhpType;

use super::driver::IrPass;
use super::rewrite::{count_value_uses, defining_instruction, neutralize_to_nop};
use super::string_len_sink::{const_local_string, const_string_value};

/// Folds constant-local iconv search positions.
pub struct IconvPositionConstSink;

impl IrPass for IconvPositionConstSink {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "iconv_position_const_sink"
    }

    /// Skips functions without an iconv position call.
    fn is_applicable(&self, function: &Function) -> bool {
        function.instructions.iter().any(|inst| {
            matches!(
                direct_target(inst),
                Some(RuntimeFnId::IconvStrpos | RuntimeFnId::IconvStrrpos)
            )
        })
    }

    /// Folds every independently proven call whose search succeeds.
    fn run(&self, function: &mut Function, data: &mut DataPool) -> bool {
        let candidates: Vec<InstId> = function
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, inst)| {
                matches!(
                    direct_target(inst),
                    Some(RuntimeFnId::IconvStrpos | RuntimeFnId::IconvStrrpos)
                )
            })
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
fn direct_target(inst: &Instruction) -> Option<RuntimeFnId> {
    match inst.immediate {
        Some(Immediate::RuntimeCall(
            RuntimeCallTarget::Function(target)
                | RuntimeCallTarget::ProfiledFunction { target, .. },
        )) => Some(target),
        _ => None,
    }
}

/// Returns a literal `i64` value produced by `ConstI64`.
fn const_i64(function: &Function, value: ValueId) -> Option<i64> {
    let inst = defining_instruction(function, value)?;
    match (inst.op, &inst.immediate) {
        (Op::ConstI64, Some(Immediate::I64(value))) => Some(*value),
        _ => None,
    }
}

/// Folds one search when both operands are immutable ASCII literals and the position is known.
fn fold(function: &mut Function, data: &DataPool, call_id: InstId) -> bool {
    let Some(call) = function.instruction(call_id) else {
        return false;
    };
    let Some(target) = direct_target(call) else {
        return false;
    };
    let (encoding_operand, offset) = match target {
        RuntimeFnId::IconvStrpos if call.operands.len() == 4 => {
            let Some(offset) = const_i64(function, call.operands[2]) else {
                return false;
            };
            if offset != 0 {
                return false;
            }
            (call.operands[3], 0usize)
        }
        RuntimeFnId::IconvStrrpos if call.operands.len() == 3 => (call.operands[2], 0usize),
        _ => return false,
    };
    let Some(encoding) = const_string_value(function, data, encoding_operand) else {
        return false;
    };
    if !encoding.eq_ignore_ascii_case("UTF-8") {
        return false;
    }
    let Some((haystack, _)) = const_local_string(function, data, call.operands[0]) else {
        return false;
    };
    let Some((needle, _)) = const_local_string(function, data, call.operands[1]) else {
        return false;
    };
    if !haystack.is_ascii() || !needle.is_ascii() || needle.is_empty() {
        return false;
    }
    let found = match target {
        RuntimeFnId::IconvStrpos => haystack[offset..]
            .find(&needle)
            .map(|position| position + offset),
        RuntimeFnId::IconvStrrpos => haystack.rfind(&needle),
        _ => return false,
    };
    let Some(position) = found else {
        return false;
    };

    let Some(result) = call.result else {
        return false;
    };
    let Some(ValueDef::Instruction { block, index, .. }) = function.value(result).map(|value| value.def)
    else {
        return false;
    };
    let position_value = insert_const_i64(function, block, index as usize, position as i64);
    let Some(call) = function.instruction_mut(call_id) else {
        return false;
    };
    call.op = Op::MixedBox;
    call.operands = vec![position_value];
    call.immediate = None;
    call.result_type = IrType::Heap(IrHeapKind::Mixed);
    call.result_php_type = PhpType::Mixed;
    call.result_ownership = Ownership::Owned;
    call.effects = Op::MixedBox.default_effects();
    if let Some(value) = function.value_mut(result) {
        value.ir_type = IrType::Heap(IrHeapKind::Mixed);
        value.php_type = PhpType::Mixed;
        value.ownership = Ownership::Owned;
    }
    let uses = count_value_uses(function);
    let dead_loads: Vec<InstId> = function
        .instructions
        .iter()
        .enumerate()
        .filter_map(|(raw, inst)| {
            let id = InstId::from_raw(raw as u32);
            let result = inst.result?;
            (inst.op == Op::LoadLocal && uses.get(&result).copied().unwrap_or(0) == 0).then_some(id)
        })
        .collect();
    for id in dead_loads {
        if let Some(load) = function.instruction_mut(id) {
            neutralize_to_nop(load);
        }
    }
    true
}

/// Inserts a `const_i64` immediately before `position` in `block`, returning its value.
fn insert_const_i64(function: &mut Function, block: BlockId, position: usize, value: i64) -> ValueId {
    let inst_id = InstId::from_raw(function.instructions.len() as u32);
    let result = ValueId::from_raw(function.values.len() as u32);
    function.values.push(Value {
        ir_type: IrType::I64,
        php_type: PhpType::Int,
        def: ValueDef::Instruction { block, index: position as u32, inst: inst_id },
        ownership: Ownership::NonHeap,
    });
    let mut inst = Instruction::new(
        Op::ConstI64,
        Vec::new(),
        Some(Immediate::I64(value)),
        Some(result),
        IrType::I64,
        PhpType::Int,
        Ownership::NonHeap,
        Op::ConstI64.default_effects(),
        None,
    );
    inst.origin = Some(PassOrigin::ConstFold);
    function.instructions.push(inst);
    function.blocks[block.as_raw() as usize].instructions.insert(position, inst_id);
    let shifted: Vec<InstId> = function.blocks[block.as_raw() as usize].instructions[position + 1..].to_vec();
    for (offset, shifted_id) in shifted.into_iter().enumerate() {
        if let Some(value) = function.instructions[shifted_id.as_raw() as usize].result {
            function.values[value.as_raw() as usize].def = ValueDef::Instruction {
                block,
                index: (position + 1 + offset) as u32,
                inst: shifted_id,
            };
        }
    }
    result
}
