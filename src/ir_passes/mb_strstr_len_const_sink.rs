//! Purpose:
//! Folds the allocation-free `strlen(mb_strstr(...))` family to a constant when both
//! operands are immutable compile-time ASCII locals.
//!
//! Called from:
//! - The EIR fixed-point pass driver after `StrstrLenSink` rewrote the boxed search.
//!
//! Key details:
//! - `strstr`/`stristr` search the full needle; `strrchr`/`strrichr` search its last
//!   occurrence. A missing match follows PHP's `strlen(false)` zero.
//! - Only byte-oriented ASCII operands are folded, so case folding and offsets remain exact.
//! - The before-needle flag is honored for the forward searches; reverse searches with a
//!   prefix flag stay on the runtime path.

use crate::ir::{DataPool, Function, Immediate, InstId, Op, Ownership, RuntimeCallTarget, RuntimeFnId, ValueId};
use crate::types::PhpType;

use super::driver::IrPass;
use super::string_len_sink::{const_local_string, const_string_value};
use super::rewrite::{count_value_uses, neutralize_to_nop};

/// Folds constant-local mbstring search lengths.
pub struct MbStrstrLenConstSink;

impl IrPass for MbStrstrLenConstSink {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "mb_strstr_len_const_sink"
    }

    /// Skips functions with no candidate search-length call.
    fn is_applicable(&self, function: &Function) -> bool {
        function.instructions.iter().any(is_candidate)
    }

    /// Folds every independently proven call.
    fn run(&self, function: &mut Function, data: &mut DataPool) -> bool {
        let candidates: Vec<InstId> = function
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, inst)| is_candidate(inst))
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

/// Returns whether `inst` is a four-operand mbstring search-length call.
fn is_candidate(inst: &crate::ir::Instruction) -> bool {
    inst.op == Op::RuntimeCall
        && inst.operands.len() == 4
        && inst.result.is_some()
        && matches!(
            direct_target(inst),
            Some(
                RuntimeFnId::MbStrstrLen
                    | RuntimeFnId::MbStristrLen
                    | RuntimeFnId::MbStrrchrLen
                    | RuntimeFnId::MbStrrichrLen
            )
        )
}

/// Returns a literal boolean operand produced by `ConstBool`.
fn const_bool(function: &Function, value: ValueId) -> Option<bool> {
    let definition = function.value(value)?;
    let crate::ir::ValueDef::Instruction { inst, .. } = definition.def else {
        return None;
    };
    let inst = function.instruction(inst)?;
    match (inst.op, &inst.immediate) {
        (Op::ConstBool, Some(Immediate::Bool(value))) => Some(*value),
        _ => None,
    }
}

/// Folds one search length when both operands and the flag are compile-time values.
fn fold(function: &mut Function, data: &DataPool, call_id: InstId) -> bool {
    let Some(call) = function.instruction(call_id) else {
        return false;
    };
    let Some(target) = direct_target(call) else {
        return false;
    };
    let Some((haystack, _)) = const_local_string(function, data, call.operands[0]) else {
        return false;
    };
    let Some((needle, _)) = const_local_string(function, data, call.operands[1]) else {
        return false;
    };
    let Some(before) = const_bool(function, call.operands[2]) else {
        return false;
    };
    let Some(encoding) = const_string_value(function, data, call.operands[3]) else {
        return false;
    };
    if !haystack.is_ascii() || !needle.is_ascii() || !encoding.eq_ignore_ascii_case("UTF-8") {
        return false;
    }
    let insensitive = matches!(target, RuntimeFnId::MbStristrLen | RuntimeFnId::MbStrrichrLen);
    let haystack = if insensitive { haystack.to_ascii_lowercase() } else { haystack };
    let needle = if insensitive { needle.to_ascii_lowercase() } else { needle };
    let length = match target {
        RuntimeFnId::MbStrstrLen | RuntimeFnId::MbStristrLen => {
            if needle.is_empty() {
                if before { 0 } else { haystack.len() }
            } else {
                haystack.find(&needle).map_or(0, |position| {
                    if before { position } else { haystack.len() - position }
                })
            }
        }
        RuntimeFnId::MbStrrchrLen | RuntimeFnId::MbStrrichrLen => {
            if before || needle.is_empty() {
                return false;
            }
            haystack.rfind(&needle).map_or(0, |position| haystack.len() - position)
        }
        _ => return false,
    };
    let Some(result) = call.result else {
        return false;
    };
    let Some(call) = function.instruction_mut(call_id) else {
        return false;
    };
    call.op = Op::ConstI64;
    call.operands.clear();
    call.immediate = Some(Immediate::I64(length as i64));
    call.result_type = crate::ir::IrType::I64;
    call.result_php_type = PhpType::Int;
    call.result_ownership = Ownership::NonHeap;
    call.effects = Op::ConstI64.default_effects();
    if let Some(value) = function.value_mut(result) {
        value.ir_type = crate::ir::IrType::I64;
        value.php_type = PhpType::Int;
        value.ownership = Ownership::NonHeap;
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
