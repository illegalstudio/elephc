//! Purpose:
//! Replaces `strlen(base64_decode(...))` with the allocation-free `__rt_base64_decoded_len` helper when
//! the boxed `string|false` result is never observed as a value.
//!
//! Called from:
//! - The EIR fixed-point pass driver after the position-cast sinks.
//!
//! Key details:
//! - The inner `base64_decode()` result may be observed only by one `Cast` to `Str` plus release
//!   scaffolding. That cast's result may be observed only by one `StrLen` plus releases.
//! - The runtime helper still evaluates and coerces both arguments and keeps the strict
//!   flag/padding rules; it returns the decoded byte count, or zero for PHP's `strlen(false)`.
//! - Any other observer keeps the boxed call, including `=== false` checks and string use.

use crate::ir::{
    DataPool, Function, Immediate, InstId, IrType, Op, Ownership, RuntimeCallTarget,
    RuntimeFnId, ValueId,
};
use crate::types::PhpType;

use super::driver::IrPass;
use super::rewrite::{neutralize_to_nop, replace_all_uses};

/// Rewrites length-only `base64_decode()` observations.
pub struct Base64DecodeLenSink;

impl IrPass for Base64DecodeLenSink {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "base64_decode_len_sink"
    }

    /// Skips functions with no boxed `base64_decode()` runtime call.
    fn is_applicable(&self, function: &Function) -> bool {
        function.instructions.iter().any(is_boxed_base64_decode)
    }

    /// Specializes every independently proven call.
    fn run(&self, function: &mut Function, _data: &mut DataPool) -> bool {
        let candidates: Vec<InstId> = function
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, inst)| is_boxed_base64_decode(inst))
            .map(|(raw, _)| InstId::from_raw(raw as u32))
            .collect();
        let mut changed = false;
        for candidate in candidates {
            if specialize(function, candidate) {
                changed = true;
            }
        }
        changed
    }
}

/// Returns whether `inst` is a two/three-operand `base64_decode()` call producing `string|false`.
fn is_boxed_base64_decode(inst: &crate::ir::Instruction) -> bool {
    inst.op == Op::RuntimeCall
        && matches!(
            inst.immediate,
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                RuntimeFnId::Base64Decode
            )))
        )
        && matches!(inst.operands.len(), 1 | 2)
        && inst.result.is_some()
        && inst.result_php_type.codegen_repr() == PhpType::Mixed
}

/// Specializes one candidate after proving its complete use graph.
fn specialize(function: &mut Function, candidate: InstId) -> bool {
    let Some(call) = function.instruction(candidate) else {
        return false;
    };
    let Some(result) = call.result else {
        return false;
    };
    let Some((cast, cast_result)) = analyze_boxed_uses(function, result) else {
        return false;
    };
    let Some((length, length_result, mut releases)) = analyze_length_uses(function, cast_result) else {
        return false;
    };
    releases.extend(analyze_releases(function, result));

    // `base64_decode()` pins its encoded-string operand because the boxed result may alias it. The
    // length-only helper returns an integer, so retarget the call at the original load and
    // retire the now-unneeded lifetime pin and its hidden temporary slot.
    let Some(operands) = function.instruction(candidate).map(|call| call.operands.clone()) else {
        return false;
    };
    let mut detached = Vec::new();
    for (index, operand) in operands.iter().copied().enumerate().take(2) {
        if let Some(source) = detach_argument_pin(function, operand, candidate) {
            detached.push((index, source));
        }
    }

    let Some(call) = function.instruction_mut(candidate) else {
        return false;
    };
    for (index, source) in detached {
        call.operands[index] = source;
    }
    call.immediate = Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
        RuntimeFnId::Base64DecodedLen,
    )));
    call.result_type = IrType::I64;
    call.result_php_type = PhpType::Int;
    call.result_ownership = Ownership::NonHeap;
    call.effects = RuntimeFnId::Base64DecodedLen.intrinsic_effects();
    if let Some(value) = function.value_mut(result) {
        value.ir_type = IrType::I64;
        value.php_type = PhpType::Int;
        value.ownership = Ownership::NonHeap;
    }
    replace_all_uses(
        function,
        &std::collections::HashMap::from([(length_result, result)]),
    );
    for id in std::iter::once(cast)
        .chain(std::iter::once(length))
        .chain(releases)
    {
        if let Some(inst) = function.instruction_mut(id) {
            neutralize_to_nop(inst);
        }
    }
    true
}

/// Retires the lifetime pin and hidden slot that keep a `base64_decode()` string operand alive.
///
/// Returns the original loaded value when the operand is exactly an `Acquire` lifetime pin
/// and every use of the pin and its local slot is removable scaffolding.
fn detach_argument_pin(function: &mut Function, operand: ValueId, call: InstId) -> Option<ValueId> {
    let (acquire_id, acquire) = defining_instruction_with_id(function, operand)?;
    if acquire.op != Op::Acquire
        || acquire.immediate != Some(Immediate::Bool(true))
        || acquire.operands.len() != 1
    {
        return None;
    }
    let source = acquire.operands[0];
    let Some(slot) = store_slot(function, operand) else {
        return None;
    };
    let mut removable = vec![acquire_id];
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !inst.operands.contains(&operand) {
            continue;
        }
        let id = InstId::from_raw(raw as u32);
        if id == call {
            continue;
        }
        match inst.op {
            Op::Release | Op::StoreLocal | Op::Borrow => removable.push(id),
            _ => return None,
        }
    }
    for (raw, inst) in function.instructions.iter().enumerate() {
        let Some(Immediate::LocalSlot(candidate)) = inst.immediate else {
            continue;
        };
        if candidate != slot {
            continue;
        }
        match inst.op {
            Op::ReleaseLocalSlot
            | Op::StoreLocal
            | Op::PushCallOperandOwner
            | Op::PopCallOperandOwner
            | Op::UnsetLocal => removable.push(InstId::from_raw(raw as u32)),
            _ => return None,
        }
    }
    for id in removable {
        if let Some(inst) = function.instruction_mut(id) {
            neutralize_to_nop(inst);
        }
    }
    Some(source)
}

/// Returns the local slot written by the `StoreLocal` that stores `value`.
fn store_slot(function: &Function, value: ValueId) -> Option<crate::ir::LocalSlotId> {
    for inst in &function.instructions {
        if inst.op == Op::StoreLocal && inst.operands.as_slice() == [value] {
            if let Some(Immediate::LocalSlot(slot)) = inst.immediate {
                return Some(slot);
            }
        }
    }
    None
}

/// Returns the defining instruction and its id when `value` is instruction-defined.
fn defining_instruction_with_id(
    function: &Function,
    value: ValueId,
) -> Option<(InstId, &crate::ir::Instruction)> {
    let crate::ir::ValueDef::Instruction { inst, .. } = function.value(value)?.def else {
        return None;
    };
    Some((inst, function.instruction(inst)?))
}

/// Returns the single `Cast` to `Str` and release scaffolding that observe `result`.
fn analyze_boxed_uses(function: &Function, result: ValueId) -> Option<(InstId, ValueId)> {
    let mut cast = None;
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !inst.operands.contains(&result) {
            continue;
        }
        let id = InstId::from_raw(raw as u32);
        match inst.op {
            Op::Cast
                if inst.operands.as_slice() == [result]
                    && inst.result.is_some()
                    && inst.result_php_type.codegen_repr() == PhpType::Str =>
            {
                if cast.replace((id, inst.result?)).is_some() {
                    return None;
                }
            }
            Op::Release if inst.operands.as_slice() == [result] => {}
            _ => return None,
        }
    }
    let (cast, cast_result) = cast?;
    Some((cast, cast_result))
}

/// Returns the single `StrLen` observer of the cast result and its release scaffolding.
fn analyze_length_uses(
    function: &Function,
    cast_result: ValueId,
) -> Option<(InstId, ValueId, Vec<InstId>)> {
    let mut length = None;
    let mut releases = Vec::new();
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !inst.operands.contains(&cast_result) {
            continue;
        }
        let id = InstId::from_raw(raw as u32);
        match inst.op {
            Op::StrLen if inst.operands.as_slice() == [cast_result] && inst.result.is_some() => {
                if length.replace((id, inst.result?)).is_some() {
                    return None;
                }
            }
            Op::Release if inst.operands.as_slice() == [cast_result] => releases.push(id),
            _ => return None,
        }
    }
    let (length, length_result) = length?;
    Some((length, length_result, releases))
}

/// Returns every release instruction that observes `value`.
fn analyze_releases(function: &Function, value: ValueId) -> Vec<InstId> {
    function
        .instructions
        .iter()
        .enumerate()
        .filter(|(_, inst)| inst.op == Op::Release && inst.operands.as_slice() == [value])
        .map(|(raw, _)| InstId::from_raw(raw as u32))
        .collect()
}
