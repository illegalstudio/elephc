//! Purpose:
//! Folds `strlen(gzuncompress(gzcompress($literal)))` and the raw-DEFLATE pair to the
//! literal's byte length, removing the loop's decompression call.
//!
//! Called from:
//! - The EIR fixed-point pass driver after the string length sinks.
//!
//! Key details:
//! - The compressed value must be a local assigned exactly once, in the entry block, from a
//!   one-argument `gzcompress()`/`gzdeflate()` of a compile-time literal. Any other slot use
//!   keeps the call.
//! - Only one-argument outer calls are folded: PHP's optional `max_length` can truncate the
//!   result, while the one-argument round trip always reproduces the literal bytes.
//! - The outer call, its string cast, and their release scaffolding are retired together; the
//!   compressed local stays owned by its slot for normal scope cleanup.

use crate::ir::{
    DataPool, Function, Immediate, InstId, Op, Ownership, RuntimeCallTarget, RuntimeFnId, ValueId,
};
use crate::types::PhpType;

use super::driver::IrPass;
use super::rewrite::{count_value_uses, defining_instruction, neutralize_to_nop};

/// Folds a one-argument gzip round trip observed only through `strlen`.
pub struct GzLiteralRoundtripLenSink;

impl IrPass for GzLiteralRoundtripLenSink {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "gz_literal_roundtrip_len_sink"
    }

    /// Skips functions without any string length observation.
    fn is_applicable(&self, function: &Function) -> bool {
        function.instructions.iter().any(|inst| inst.op == Op::StrLen)
    }

    /// Folds every independently proven round trip.
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

/// Folds one chain when the compressed local has a single literal producer.
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
    let outer_value = cast.operands[0];
    let Some(outer) = defining_instruction(function, outer_value) else {
        return false;
    };
    let Some(outer_target) = direct_target(outer) else {
        return false;
    };
    if !matches!(outer_target, RuntimeFnId::Gzuncompress | RuntimeFnId::Gzinflate)
        || outer.operands.len() != 1
    {
        return false;
    }
    let Some(load) = defining_instruction(function, outer.operands[0]) else {
        return false;
    };
    if load.op != Op::LoadLocal {
        return false;
    }
    let Some(Immediate::LocalSlot(slot)) = load.immediate else {
        return false;
    };

    let Some(entry) = function.blocks.first() else {
        return false;
    };
    let mut stored = None;
    for (raw, inst) in function.instructions.iter().enumerate() {
        let id = InstId::from_raw(raw as u32);
        let touches_slot =
            matches!(inst.immediate, Some(Immediate::LocalSlot(found)) if found == slot);
        if !touches_slot {
            continue;
        }
        match inst.op {
            Op::StoreLocal if inst.operands.len() == 1 => {
                if !entry.instructions.contains(&id) || stored.replace(inst.operands[0]).is_some() {
                    return false;
                }
            }
            Op::LoadLocal | Op::ReleaseLocalSlot => {}
            _ => return false,
        }
    }
    let Some(stored) = stored else {
        return false;
    };
    let Some(producer_value) = through_acquire(function, stored) else {
        return false;
    };
    let Some(producer) = defining_instruction(function, producer_value) else {
        return false;
    };
    let Some(producer_target) = direct_target(producer) else {
        return false;
    };
    if !matches!(producer_target, RuntimeFnId::Gzcompress | RuntimeFnId::Gzdeflate)
        || producer.operands.len() != 1
    {
        return false;
    }
    let Some(literal) = const_string(function, data, producer.operands[0])
        .map(crate::string_bytes::literal_byte_len)
    else {
        return false;
    };

    // Every observer of the boxed inflate result must be the cast or a release, and every
    // observer of the cast must be this length or a release. Anything else keeps the call.
    let uses = count_value_uses(function);
    if uses.get(&outer_value).copied().unwrap_or(0) == 0
        || uses.get(&cast_value).copied().unwrap_or(0) == 0
    {
        return false;
    }
    let mut releases = Vec::new();
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !inst.operands.contains(&outer_value) && !inst.operands.contains(&cast_value) {
            continue;
        }
        if inst.op == Op::Release && inst.operands.len() == 1 {
            releases.push(InstId::from_raw(raw as u32));
            continue;
        }
        let defines_cast = inst.op == Op::Cast
            && inst.operands.len() == 1
            && inst.operands[0] == outer_value
            && inst.result == Some(cast_value);
        let observes_length = inst.op == Op::StrLen
            && inst.operands.len() == 1
            && inst.operands[0] == cast_value;
        if !defines_cast && !observes_length {
            return false;
        }
    }

    let Some(length) = function.instruction_mut(length_id) else {
        return false;
    };
    let Some(result) = length.result else {
        return false;
    };
    length.op = Op::ConstI64;
    length.operands.clear();
    length.immediate = Some(Immediate::I64(literal as i64));
    length.result_type = crate::ir::IrType::I64;
    length.result_php_type = PhpType::Int;
    length.result_ownership = Ownership::NonHeap;
    length.effects = Op::ConstI64.default_effects();
    if let Some(value) = function.value_mut(result) {
        value.ir_type = crate::ir::IrType::I64;
        value.php_type = PhpType::Int;
        value.ownership = Ownership::NonHeap;
    }

    let Some(cast_id) = defining_instruction_id(function, cast_value) else {
        return false;
    };
    let Some(outer_id) = defining_instruction_id(function, outer_value) else {
        return false;
    };
    for id in [cast_id, outer_id].into_iter().chain(releases) {
        if let Some(inst) = function.instruction_mut(id) {
            neutralize_to_nop(inst);
        }
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

/// Follows one `Acquire` in front of a stored value.
fn through_acquire(function: &Function, value: ValueId) -> Option<ValueId> {
    let inst = defining_instruction(function, value)?;
    if inst.op == Op::Acquire && inst.operands.len() == 1 {
        Some(inst.operands[0])
    } else {
        Some(value)
    }
}

/// Returns the literal text of a `ConstStr` value, optionally through one `Acquire`.
fn const_string<'a>(function: &Function, data: &'a DataPool, value: ValueId) -> Option<&'a str> {
    let value = through_acquire(function, value)?;
    let inst = defining_instruction(function, value)?;
    if inst.op != Op::ConstStr {
        return None;
    }
    let Some(Immediate::Data(data_id)) = inst.immediate else {
        return None;
    };
    data.strings.get(data_id.as_raw() as usize).map(String::as_str)
}

/// Returns the defining instruction identifier for one SSA value.
fn defining_instruction_id(function: &Function, value: ValueId) -> Option<InstId> {
    let definition = function.value(value)?;
    let crate::ir::ValueDef::Instruction { inst, .. } = definition.def else {
        return None;
    };
    Some(inst)
}
