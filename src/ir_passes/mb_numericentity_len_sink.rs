//! Purpose:
//! Replaces `strlen()` of mbstring numeric-entity and regex-replace producers with
//! allocation-free length operations when the produced string is never observed.
//!
//! Called from:
//! - The EIR fixed-point pass driver after the other mbstring length sinks.
//!
//! Key details:
//! - The entity result may be observed only by one `StrLen` and release scaffolding.
//! - The length-only operations keep the original subject, map, and encoding operands, so the
//!   codegen can verify the map and fall back to the shared bridge for other maps or invalid
//!   input.
//! - Any other observer keeps the full string-producing call.

use crate::ir::{DataPool, Function, Immediate, InstId, IrType, Op, Ownership, RuntimeCallTarget, RuntimeFnId, ValueId};
use crate::types::PhpType;

use super::driver::IrPass;
use super::rewrite::{neutralize_to_nop, replace_all_uses};

/// Rewrites length-only numeric-entity observations.
pub struct MbNumericentityLenSink;

impl IrPass for MbNumericentityLenSink {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "mb_numericentity_len_sink"
    }

    /// Skips functions with no `strlen(mb_*_numericentity(...))` candidate.
    fn is_applicable(&self, function: &Function) -> bool {
        function.instructions.iter().any(is_length_candidate)
    }

    /// Specializes every independently proven call.
    fn run(&self, function: &mut Function, _data: &mut DataPool) -> bool {
        let candidates: Vec<InstId> = function
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, inst)| is_length_candidate(inst))
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

/// Returns whether `inst` is a `StrLen` observation.
fn is_length_candidate(inst: &crate::ir::Instruction) -> bool {
    inst.op == Op::StrLen && inst.operands.len() == 1 && inst.result.is_some()
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

/// Specializes one length observation after proving the string has no other observer.
fn specialize(function: &mut Function, length_id: InstId) -> bool {
    let Some(length) = function.instruction(length_id) else {
        return false;
    };
    let Some(string_value) = length.operands.first().copied() else {
        return false;
    };
    let Some(length_result) = length.result else {
        return false;
    };
    let Some((call_id, call)) = defining_instruction_with_id(function, string_value) else {
        return false;
    };
    let length_target = match direct_target(call) {
        Some(RuntimeFnId::MbEncodeNumericentity) if call.operands.len() == 3 => {
            RuntimeFnId::MbEncodeNumericentityLen
        }
        Some(RuntimeFnId::MbDecodeNumericentity) if call.operands.len() == 3 => {
            RuntimeFnId::MbDecodeNumericentityLen
        }
        _ => return false,
    };
    let mut releases = Vec::new();
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !inst.operands.contains(&string_value) {
            continue;
        }
        let id = InstId::from_raw(raw as u32);
        if id == length_id && inst.op == Op::StrLen {
            continue;
        }
        if inst.op == Op::Release && inst.operands.as_slice() == [string_value] {
            releases.push(id);
            continue;
        }
        return false;
    }
    for block in &function.blocks {
        if block
            .terminator
            .as_ref()
            .is_some_and(|term| term.used_values().contains(&string_value))
        {
            return false;
        }
    }
    let Some(call) = function.instruction_mut(call_id) else {
        return false;
    };
    call.immediate = Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
        length_target,
    )));
    call.result_type = IrType::I64;
    call.result_php_type = PhpType::Int;
    call.result_ownership = Ownership::NonHeap;
    call.effects = length_target.intrinsic_effects();
    if let Some(value) = function.value_mut(string_value) {
        value.ir_type = IrType::I64;
        value.php_type = PhpType::Int;
        value.ownership = Ownership::NonHeap;
    }
    replace_all_uses(
        function,
        &std::collections::HashMap::from([(length_result, string_value)]),
    );
    for id in std::iter::once(length_id).chain(releases) {
        if let Some(inst) = function.instruction_mut(id) {
            neutralize_to_nop(inst);
        }
    }
    true
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
