//! Purpose:
//! Folds `iconv_mime_decode()` length and single-header `iconv_mime_decode_headers()`
//! count observations over compile-time literals under explicit UTF-8.
//!
//! Called from:
//! - The EIR fixed-point pass driver after constant locals and call staging are lowered.
//!
//! Key details:
//! - Only a single RFC 2047 `=?UTF-8?B?...?=` word is decoded for the length fold.
//! - The header count fold accepts one literal line with an ASCII token name and no CR/LF;
//!   folded lines, duplicate headers, and dynamic inputs keep the iconv bridge.
//! - The call's temporary argument slots, pushes, stores and releases are removed together
//!   so the loop does not keep paying the bridge marshalling cost.

use std::collections::HashSet;

use crate::ir::{
    DataPool, Function, Immediate, InstId, Op, Ownership, RuntimeCallTarget, RuntimeFnId, ValueId,
};
use crate::types::PhpType;

use super::driver::IrPass;
use super::rewrite::{defining_instruction, neutralize_to_nop};
use super::string_len_sink::{const_local_string, const_string_value};

/// Folds literal iconv MIME decode length and header count observations.
pub struct IconvMimeLiteralSink;

impl IrPass for IconvMimeLiteralSink {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "iconv_mime_literal_sink"
    }

    /// Skips functions without an iconv MIME call.
    fn is_applicable(&self, function: &Function) -> bool {
        function.instructions.iter().any(|inst| {
            matches!(
                direct_target(inst),
                Some(RuntimeFnId::IconvMimeDecode | RuntimeFnId::IconvMimeDecodeHeaders)
            )
        })
    }

    /// Folds every independently proven literal call.
    fn run(&self, function: &mut Function, data: &mut DataPool) -> bool {
        let mut changed = false;
        let length_candidates: Vec<InstId> = function
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, inst)| inst.op == Op::StrLen && inst.operands.len() == 1)
            .map(|(raw, _)| InstId::from_raw(raw as u32))
            .collect();
        for candidate in length_candidates {
            if fold_decode_length(function, data, candidate) {
                changed = true;
            }
        }
        let count_candidates: Vec<InstId> = function
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, inst)| {
                inst.op == Op::RuntimeCall
                    && inst.operands.len() == 1
                    && matches!(direct_target(inst), Some(RuntimeFnId::Count))
            })
            .map(|(raw, _)| InstId::from_raw(raw as u32))
            .collect();
        for candidate in count_candidates {
            if fold_header_count(function, data, candidate) {
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

/// Returns a literal `i64` value produced by `ConstI64`.
fn const_i64(function: &Function, value: ValueId) -> Option<i64> {
    let inst = defining_instruction(function, value)?;
    match (inst.op, &inst.immediate) {
        (Op::ConstI64, Some(Immediate::I64(value))) => Some(*value),
        _ => None,
    }
}

/// Folds `strlen((string)iconv_mime_decode($literal, 0, "UTF-8"))` for one B-encoded word.
fn fold_decode_length(function: &mut Function, data: &DataPool, length_id: InstId) -> bool {
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
    let Some(call_id) = defining_id(function, call_value) else {
        return false;
    };
    let Some(call) = function.instruction(call_id) else {
        return false;
    };
    if direct_target(call) != Some(RuntimeFnId::IconvMimeDecode) || call.operands.len() != 3 {
        return false;
    }
    if const_i64(function, call.operands[1]) != Some(0) {
        return false;
    }
    let Some(encoding) = const_string_value(function, data, call.operands[2]) else {
        return false;
    };
    if !encoding.eq_ignore_ascii_case("UTF-8") {
        return false;
    }
    let Some((encoded, _)) = const_local_string(function, data, call.operands[0]) else {
        return false;
    };
    let Some(decoded) = crate::optimize::fold::expr::decode_mime_b_word(&encoded) else {
        return false;
    };
    if count_uses_outside(function, call_value, &[Op::Cast, Op::Release]) > 0
        || count_uses_outside(function, cast_value, &[Op::StrLen, Op::Release]) > 0
    {
        return false;
    }
    let Some(result) = length.result else {
        return false;
    };
    let Some(length) = function.instruction_mut(length_id) else {
        return false;
    };
    length.op = Op::ConstI64;
    length.operands.clear();
    length.immediate = Some(Immediate::I64(decoded.len() as i64));
    length.result_type = crate::ir::IrType::I64;
    length.result_php_type = PhpType::Int;
    length.result_ownership = Ownership::NonHeap;
    length.effects = Op::ConstI64.default_effects();
    if let Some(value) = function.value_mut(result) {
        value.ir_type = crate::ir::IrType::I64;
        value.php_type = PhpType::Int;
        value.ownership = Ownership::NonHeap;
    }
    let Some(cast_id) = defining_id(function, cast_value) else {
        return false;
    };
    neutralize_call_staging(function, call_id);
    for id in [call_id, cast_id].into_iter().chain(releases_of(function, call_value)).chain(releases_of(function, cast_value)) {
        if let Some(inst) = function.instruction_mut(id) {
            neutralize_to_nop(inst);
        }
    }
    cleanup_dead_loads(function);
    true
}

/// Folds `count(iconv_mime_decode_headers($single_header, 0, "UTF-8"))` to one.
fn fold_header_count(function: &mut Function, data: &DataPool, count_id: InstId) -> bool {
    let Some(count) = function.instruction(count_id) else {
        return false;
    };
    let Some(value) = count.operands.first().copied() else {
        return false;
    };
    let Some(headers_id) = defining_id(function, value) else {
        return false;
    };
    let Some(headers) = function.instruction(headers_id) else {
        return false;
    };
    if direct_target(headers) != Some(RuntimeFnId::IconvMimeDecodeHeaders)
        || headers.operands.len() != 3
        || const_i64(function, headers.operands[1]) != Some(0)
    {
        return false;
    }
    let Some(encoding) = const_string_value(function, data, headers.operands[2]) else {
        return false;
    };
    if !encoding.eq_ignore_ascii_case("UTF-8") {
        return false;
    }
    let Some((header, _)) = const_local_string(function, data, headers.operands[0]) else {
        return false;
    };
    if header.contains('\r') || header.contains('\n') {
        return false;
    }
    let Some(colon) = header.find(':') else {
        return false;
    };
    if colon == 0 || !header[..colon].bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-') {
        return false;
    }
    if count_uses_outside(
        function,
        value,
        &[
            Op::RuntimeCall,
            Op::Release,
            Op::StoreLocal,
            Op::LoadLocal,
            Op::PushCallOperandOwner,
            Op::PopCallOperandOwner,
            Op::ReleaseLocalSlot,
            Op::UnsetLocal,
        ],
    ) > 0
    {
        return false;
    }
    let Some(result) = count.result else {
        return false;
    };
    // Remove the argument staging while both calls still expose their operands.
    neutralize_call_staging(function, count_id);
    neutralize_call_staging(function, headers_id);
    let Some(count) = function.instruction_mut(count_id) else {
        return false;
    };
    count.op = Op::ConstI64;
    count.operands.clear();
    count.immediate = Some(Immediate::I64(1));
    count.result_type = crate::ir::IrType::I64;
    count.result_php_type = PhpType::Int;
    count.result_ownership = Ownership::NonHeap;
    count.effects = Op::ConstI64.default_effects();
    if let Some(value) = function.value_mut(result) {
        value.ir_type = crate::ir::IrType::I64;
        value.php_type = PhpType::Int;
        value.ownership = Ownership::NonHeap;
    }
    for id in [headers_id].into_iter().chain(releases_of(function, value)) {
        if let Some(inst) = function.instruction_mut(id) {
            neutralize_to_nop(inst);
        }
    }
    cleanup_dead_loads(function);
    true
}

/// Removes the temporary slot stores, push/pop owners and acquired value plumbing for one call.
pub(super) fn neutralize_call_staging(function: &mut Function, call_id: InstId) {
    let Some(call) = function.instruction(call_id) else {
        return;
    };
    let operands = call.operands.clone();
    let mut slots = HashSet::new();
    let mut staged = HashSet::new();
    for operand in operands {
        let base = through_acquire(function, operand).unwrap_or(operand);
        for inst in &function.instructions {
            if inst.op != Op::StoreLocal || inst.operands.len() != 1 {
                continue;
            }
            if inst.operands[0] != operand && inst.operands[0] != base {
                continue;
            }
            if let Some(Immediate::LocalSlot(slot)) = inst.immediate {
                slots.insert(slot);
                staged.insert(inst.operands[0]);
            }
        }
    }
    if slots.is_empty() {
        return;
    }
    let mut unknown = false;
    for inst in &function.instructions {
        let Some(Immediate::LocalSlot(slot)) = inst.immediate else {
            continue;
        };
        if !slots.contains(&slot) {
            continue;
        }
        if !matches!(
            inst.op,
            Op::StoreLocal
                | Op::LoadLocal
                | Op::ReleaseLocalSlot
                | Op::UnsetLocal
                | Op::PushCallOperandOwner
                | Op::PopCallOperandOwner
        ) {
            unknown = true;
            break;
        }
    }
    if unknown {
        return;
    }
    for inst in &mut function.instructions {
        let Some(Immediate::LocalSlot(slot)) = inst.immediate else {
            continue;
        };
        if !slots.contains(&slot) {
            continue;
        }
        if matches!(
            inst.op,
            Op::StoreLocal
                | Op::LoadLocal
                | Op::ReleaseLocalSlot
                | Op::UnsetLocal
                | Op::PushCallOperandOwner
                | Op::PopCallOperandOwner
        ) {
            neutralize_to_nop(inst);
        }
    }
    for value in staged {
        if let Some(id) = defining_id(function, value) {
            if function.instruction(id).is_some_and(|inst| inst.op == Op::Acquire) {
                if let Some(inst) = function.instruction_mut(id) {
                    neutralize_to_nop(inst);
                }
            }
        }
        for inst in &mut function.instructions {
            if inst.op == Op::Release && inst.operands.len() == 1 && inst.operands[0] == value {
                neutralize_to_nop(inst);
            }
        }
    }
}

/// Follows one `Acquire` in front of a value.
fn through_acquire(function: &Function, value: ValueId) -> Option<ValueId> {
    let inst = defining_instruction(function, value)?;
    if inst.op == Op::Acquire && inst.operands.len() == 1 {
        Some(inst.operands[0])
    } else {
        Some(value)
    }
}

/// Neutralizes now-unused local loads after staging is removed.
pub(super) fn cleanup_dead_loads(function: &mut Function) {
    let uses = function.value_use_counts();
    let dead: Vec<InstId> = function
        .instructions
        .iter()
        .enumerate()
        .filter_map(|(raw, inst)| {
            (inst.op == Op::LoadLocal
                && inst.result.is_some_and(|result| uses.get(&result).copied().unwrap_or(0) == 0))
            .then_some(InstId::from_raw(raw as u32))
        })
        .collect();
    for id in dead {
        if let Some(inst) = function.instruction_mut(id) {
            neutralize_to_nop(inst);
        }
    }
}

/// Counts uses of `value` whose opcode is not in `allowed`.
fn count_uses_outside(function: &Function, value: ValueId, allowed: &[Op]) -> usize {
    function
        .instructions
        .iter()
        .filter(|inst| inst.operands.contains(&value) && !allowed.contains(&inst.op))
        .count()
}

/// Returns every release of `value`.
fn releases_of(function: &Function, value: ValueId) -> Vec<InstId> {
    function
        .instructions
        .iter()
        .enumerate()
        .filter_map(|(raw, inst)| {
            (inst.op == Op::Release && inst.operands.len() == 1 && inst.operands[0] == value)
                .then_some(InstId::from_raw(raw as u32))
        })
        .collect()
}

/// Returns the defining instruction identifier for one SSA value.
fn defining_id(function: &Function, value: ValueId) -> Option<InstId> {
    let definition = function.value(value)?;
    let crate::ir::ValueDef::Instruction { inst, .. } = definition.def else {
        return None;
    };
    Some(inst)
}
