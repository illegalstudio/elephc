//! Purpose:
//! Replaces `count(mb_encoding_aliases(...))` and `count(mb_list_encodings())` with
//! allocation-free catalog-count operations when the temporary array is never observed.
//!
//! Called from:
//! - The EIR fixed-point pass driver after the mbstring length sinks.
//!
//! Key details:
//! - The catalog result may be observed only by one `Count` and release scaffolding.
//! - The count keeps the encoding operand, so argument coercion remains at the original point.
//! - Any other observer keeps the full array-producing call.

use crate::ir::{DataPool, Function, Immediate, InstId, IrType, Op, Ownership, RuntimeCallTarget, RuntimeFnId, ValueId};
use crate::types::PhpType;

use super::driver::IrPass;
use super::rewrite::neutralize_to_nop;

/// Rewrites count-only mbstring catalog queries.
pub struct MbCatalogCountSink;

impl IrPass for MbCatalogCountSink {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "mb_catalog_count_sink"
    }

    /// Skips functions with no catalog count candidate.
    fn is_applicable(&self, function: &Function) -> bool {
        function.instructions.iter().any(is_count_call)
    }

    /// Specializes every independently proven count.
    fn run(&self, function: &mut Function, _data: &mut DataPool) -> bool {
        let candidates: Vec<InstId> = function
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, inst)| is_count_call(inst))
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

/// Returns whether `inst` is a one-operand `count()` call.
fn is_count_call(inst: &crate::ir::Instruction) -> bool {
    inst.op == Op::RuntimeCall
        && matches!(
            inst.immediate,
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                RuntimeFnId::Count
            )))
        )
        && inst.operands.len() == 1
        && inst.result.is_some()
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

/// Specializes one count observation after proving the array has no other observer.
fn specialize(function: &mut Function, count_id: InstId) -> bool {
    let Some(count) = function.instruction(count_id) else {
        return false;
    };
    let Some(array_value) = count.operands.first().copied() else {
        return false;
    };
    let Some((query_id, query)) = defining_instruction_with_id(function, array_value) else {
        return false;
    };
    let count_target = match direct_target(query) {
        Some(RuntimeFnId::MbEncodingAliases) if query.operands.len() == 1 => {
            RuntimeFnId::MbEncodingAliasesCount
        }
        Some(RuntimeFnId::MbListEncodings) if query.operands.is_empty() => {
            RuntimeFnId::MbListEncodingsCount
        }
        _ => return false,
    };
    let mut releases = Vec::new();
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !inst.operands.contains(&array_value) {
            continue;
        }
        let id = InstId::from_raw(raw as u32);
        if id == count_id && inst.op == Op::RuntimeCall {
            continue;
        }
        if matches!(inst.op, Op::Release | Op::StoreLocal)
            && inst.operands.as_slice() == [array_value]
        {
            releases.push(id);
            continue;
        }
        return false;
    }
    for block in &function.blocks {
        if block
            .terminator
            .as_ref()
            .is_some_and(|term| term.used_values().contains(&array_value))
        {
            return false;
        }
    }
    let operands = query.operands.clone();
    let Some(count) = function.instruction_mut(count_id) else {
        return false;
    };
    count.immediate = Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
        count_target,
    )));
    count.operands = operands;
    count.result_type = IrType::I64;
    count.result_php_type = PhpType::Int;
    count.result_ownership = Ownership::NonHeap;
    count.effects = count_target.intrinsic_effects();
    for id in std::iter::once(query_id).chain(releases) {
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
