//! Purpose:
//! Replaces a local assigned once from `mb_chr()` and observed only through `strlen()` with the
//! allocation-free `MbChrLen` runtime call.
//!
//! Called from:
//! - The EIR fixed-point pass driver after the other string length sinks.
//!
//! Key details:
//! - The call result may be observed only by the Mixed clone/acquire/store scaffolding that
//!   binds it to one local slot, plus releases of those temporaries.
//! - Every load of that slot must feed exactly one `Cast` to `Str` plus releases, and every cast
//!   must feed exactly one `StrLen` plus releases. Any value observation (`echo $x`, `=== false`,
//!   another call) keeps the boxed `mb_chr()` result.
//! - Retargeting preserves the original codepoint and encoding operands, so argument evaluation
//!   order and coercion stay at the assignment site. `MbChrLen` owns the explicit-UTF-8 fast
//!   path and the bridge fallback for other encodings.
//! - The slot must have exactly one store. Scope cleanup for the now-unused slot is retired with
//!   the rest of the scaffolding.

use std::collections::HashMap;

use crate::ir::{
    DataPool, Function, Immediate, InstId, IrType, LocalSlotId, Op, Ownership, RuntimeCallTarget,
    RuntimeFnId, ValueId,
};
use crate::types::PhpType;

use super::dominance::{compute_dominance, DominanceInfo};
use super::driver::IrPass;
use super::rewrite::{neutralize_to_nop, replace_all_uses};

/// Rewrites length-only local `mb_chr()` results.
pub struct MbChrLenLocalSink;

impl IrPass for MbChrLenLocalSink {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "mb_chr_len_local_sink"
    }

    /// Skips functions with no boxed `mb_chr()` runtime call.
    fn is_applicable(&self, function: &Function) -> bool {
        function.instructions.iter().any(is_boxed_mb_chr)
    }

    /// Specializes every independently proven call.
    fn run(&self, function: &mut Function, _data: &mut DataPool) -> bool {
        let dominance = compute_dominance(function);
        let candidates: Vec<InstId> = function
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, inst)| is_boxed_mb_chr(inst))
            .map(|(raw, _)| InstId::from_raw(raw as u32))
            .collect();
        let mut changed = false;
        for candidate in candidates {
            if specialize(function, candidate, &dominance) {
                changed = true;
            }
        }
        changed
    }
}

/// Returns whether `inst` is an explicit two-operand `mb_chr()` producing `string|false`.
fn is_boxed_mb_chr(inst: &crate::ir::Instruction) -> bool {
    matches!(target(inst), Some(RuntimeFnId::MbChr))
        && inst.op == Op::RuntimeCall
        && inst.operands.len() == 2
        && inst.result.is_some()
        && inst.result_php_type.codegen_repr() == PhpType::Mixed
}

/// Returns the stable runtime target of a direct or profiled call instruction.
fn target(inst: &crate::ir::Instruction) -> Option<RuntimeFnId> {
    match &inst.immediate {
        Some(Immediate::RuntimeCall(
            RuntimeCallTarget::Function(target)
            | RuntimeCallTarget::ProfiledFunction { target, .. },
        )) => Some(*target),
        _ => None,
    }
}

/// One `strlen($local)` observation and the scaffolding that feeds it.
struct Observation {
    load: InstId,
    cast: InstId,
    length: InstId,
    length_result: ValueId,
    neutral: Vec<InstId>,
}

/// Specializes one call after proving the complete local use graph.
fn specialize(function: &mut Function, candidate: InstId, dominance: &DominanceInfo) -> bool {
    let Some(call) = function.instruction(candidate) else {
        return false;
    };
    let Some(result) = call.result else {
        return false;
    };
    let Some(call_block) = instruction_block(function, result) else {
        return false;
    };
    let Some((store, slot, mut neutral)) = trace_store(function, result) else {
        return false;
    };
    let Some((observations, cleanup)) = analyze_slot(function, slot, store) else {
        return false;
    };
    if observations.is_empty()
        || observations.iter().any(|observation| {
            instruction_block(function, observation.length_result)
                .is_none_or(|block| !dominance.dominates(call_block, block))
        })
    {
        return false;
    }

    let Some(call) = function.instruction_mut(candidate) else {
        return false;
    };
    call.immediate = Some(Immediate::RuntimeCall(match &call.immediate {
        Some(Immediate::RuntimeCall(RuntimeCallTarget::ProfiledFunction {
            arguments,
            strict_php,
            strict_types,
            ..
        })) => RuntimeCallTarget::ProfiledFunction {
            target: RuntimeFnId::MbChrLen,
            arguments: *arguments,
            strict_php: *strict_php,
            strict_types: *strict_types,
        },
        _ => RuntimeCallTarget::Function(RuntimeFnId::MbChrLen),
    }));
    call.result_type = IrType::I64;
    call.result_php_type = PhpType::Int;
    call.result_ownership = Ownership::NonHeap;
    call.effects = RuntimeFnId::MbChrLen.intrinsic_effects();
    if let Some(value) = function.value_mut(result) {
        value.ir_type = IrType::I64;
        value.php_type = PhpType::Int;
        value.ownership = Ownership::NonHeap;
    }

    let replacements: HashMap<ValueId, ValueId> = observations
        .iter()
        .map(|observation| (observation.length_result, result))
        .collect();
    replace_all_uses(function, &replacements);

    for observation in observations {
        neutral.push(observation.load);
        neutral.push(observation.cast);
        neutral.push(observation.length);
        neutral.extend(observation.neutral);
    }
    neutral.extend(cleanup);
    neutral.push(store);
    for id in neutral {
        if id == candidate {
            continue;
        }
        if let Some(inst) = function.instruction_mut(id) {
            neutralize_to_nop(inst);
        }
    }
    true
}

/// Returns the block that defines one instruction result.
fn instruction_block(function: &Function, value: ValueId) -> Option<crate::ir::BlockId> {
    let crate::ir::ValueDef::Instruction { block, .. } = function.value(value)?.def else {
        return None;
    };
    Some(block)
}

/// Traces the single clone/acquire/store chain from the boxed call result to its local slot.
fn trace_store(
    function: &Function,
    result: ValueId,
) -> Option<(InstId, LocalSlotId, Vec<InstId>)> {
    let (clone, mut neutral) = single_consumer(
        function,
        result,
        Op::MixedClone,
    )?;
    let clone_result = function.instruction(clone)?.result?;
    let (acquire, clone_releases) = single_consumer(function, clone_result, Op::Acquire)?;
    neutral.extend(clone_releases);
    let acquire_result = function.instruction(acquire)?.result?;
    let mut store = None;
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !inst.operands.contains(&acquire_result) {
            continue;
        }
        if inst.op != Op::StoreLocal || inst.operands.as_slice() != [acquire_result] {
            return None;
        }
        let Some(Immediate::LocalSlot(slot)) = inst.immediate else {
            return None;
        };
        if store.replace((InstId::from_raw(raw as u32), slot)).is_some() {
            return None;
        }
    }
    neutral.push(clone);
    neutral.push(acquire);
    let (store, slot) = store?;
    Some((store, slot, neutral))
}

/// Returns the one consumer with `op`, plus every direct release of the value.
fn single_consumer(
    function: &Function,
    value: ValueId,
    op: Op,
) -> Option<(InstId, Vec<InstId>)> {
    let mut found = None;
    let mut releases = Vec::new();
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !inst.operands.contains(&value) {
            continue;
        }
        let id = InstId::from_raw(raw as u32);
        if inst.op == op && inst.operands.as_slice() == [value] {
            if found.replace(id).is_some() {
                return None;
            }
        } else if inst.op == Op::Release && inst.operands.as_slice() == [value] {
            releases.push(id);
        } else {
            return None;
        }
    }
    Some((found?, releases))
}

/// Proves every other slot reference is a load feeding one cast and one `strlen`.
fn analyze_slot(
    function: &Function,
    slot: LocalSlotId,
    store: InstId,
) -> Option<(Vec<Observation>, Vec<InstId>)> {
    let mut observations = Vec::new();
    let mut cleanup = Vec::new();
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !matches!(inst.immediate, Some(Immediate::LocalSlot(found)) if found == slot) {
            continue;
        }
        let id = InstId::from_raw(raw as u32);
        if id == store {
            continue;
        }
        match inst.op {
            Op::LoadLocal if inst.operands.is_empty() => {
                observations.push(analyze_load(function, id)?);
            }
            Op::ReleaseLocalSlot => cleanup.push(id),
            _ => return None,
        }
    }
    Some((observations, cleanup))
}

/// Returns the single cast and length observation fed by one slot load.
fn analyze_load(function: &Function, load: InstId) -> Option<Observation> {
    let load_result = function.instruction(load)?.result?;
    let mut cast = None;
    let mut neutral = Vec::new();
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !inst.operands.contains(&load_result) {
            continue;
        }
        let id = InstId::from_raw(raw as u32);
        match inst.op {
            Op::Cast
                if inst.operands.as_slice() == [load_result]
                    && inst.result.is_some()
                    && inst.result_php_type.codegen_repr() == PhpType::Str =>
            {
                if cast.replace(id).is_some() {
                    return None;
                }
            }
            Op::Release if inst.operands.as_slice() == [load_result] => neutral.push(id),
            _ => return None,
        }
    }
    let cast = cast?;
    let cast_result = function.instruction(cast)?.result?;
    let mut length = None;
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
            Op::Release if inst.operands.as_slice() == [cast_result] => neutral.push(id),
            _ => return None,
        }
    }
    let (length, length_result) = length?;
    Some(Observation {
        load,
        cast,
        length,
        length_result,
        neutral,
    })
}
