//! Purpose:
//! Folds `mb_strlen($s, "UTF-8")` when `$s` is a local assigned exactly once from a
//! compile-time string and never mutated afterwards.
//!
//! Called from:
//! - The EIR fixed-point pass driver after constant folding and string length sinks.
//!
//! Key details:
//! - The stored value may be acquired before the store; the pass traces that single copy back
//!   to its `ConstStr`.
//! - Every `LocalSlot` reference must be a load, the single entry store, or scope cleanup. Any
//!   other slot operation (by-reference capture, unset, or a second store) leaves the call alone.
//! - The folded length is the UTF-8 codepoint count of the stored literal, matching PHP's
//!   explicit-UTF-8 `mb_strlen()`.

use crate::ir::{DataPool, Function, Immediate, InstId, Op, Ownership, RuntimeCallTarget, RuntimeFnId, ValueId};
use crate::types::PhpType;

use super::driver::IrPass;
use super::rewrite::{count_value_uses, neutralize_to_nop};

/// Folds an explicit-UTF-8 `mb_strlen()` over an immutable literal local.
pub struct MbStrlenConstLocalSink;

impl IrPass for MbStrlenConstLocalSink {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "mb_strlen_const_local_sink"
    }

    /// Skips functions with no explicit-UTF-8 mb_strlen call.
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

/// Returns whether `inst` is a two-operand direct `mb_strlen()` call.
fn is_candidate(inst: &crate::ir::Instruction) -> bool {
    let target = match inst.immediate {
        Some(Immediate::RuntimeCall(
            RuntimeCallTarget::Function(target)
                | RuntimeCallTarget::ProfiledFunction { target, .. },
        )) => target,
        _ => return false,
    };
    inst.op == Op::RuntimeCall
        && inst.result.is_some()
        && match target {
            RuntimeFnId::MbStrlen | RuntimeFnId::MbStrwidth | RuntimeFnId::MbCheckEncoding => {
                inst.operands.len() == 2
            }
            RuntimeFnId::MbSubstrCount | RuntimeFnId::MbStrSplitCount => inst.operands.len() == 3,
            RuntimeFnId::MbStrposInt
            | RuntimeFnId::MbStrrposInt
            | RuntimeFnId::MbStriposInt
            | RuntimeFnId::MbStrriposInt => inst.operands.len() == 4,
            _ => false,
        }
}

/// Returns a literal `i64` value produced by `ConstI64`.
fn const_i64(function: &Function, value: ValueId) -> Option<i64> {
    let (_, inst) = defining_instruction_with_id(function, value)?;
    match (inst.op, &inst.immediate) {
        (Op::ConstI64, Some(Immediate::I64(value))) => Some(*value),
        _ => None,
    }
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

/// Traces one value through a single `Acquire` to its source.
fn through_acquire(function: &Function, value: ValueId) -> Option<ValueId> {
    let (_, inst) = defining_instruction_with_id(function, value)?;
    if inst.op == Op::Acquire && inst.operands.len() == 1 {
        Some(inst.operands[0])
    } else {
        Some(value)
    }
}

/// Returns the literal bytes of a `ConstStr` value, optionally through one `Acquire`.
fn const_string<'a>(function: &Function, data: &'a DataPool, value: ValueId) -> Option<&'a str> {
    let value = through_acquire(function, value)?;
    let (_, inst) = defining_instruction_with_id(function, value)?;
    let (Op::ConstStr, Some(Immediate::Data(data_id))) = (inst.op, &inst.immediate) else {
        return None;
    };
    data.strings.get(data_id.as_raw() as usize).map(String::as_str)
}

/// Returns whether a stored value is a literal string, possibly acquired once.
fn const_stored_string<'a>(function: &Function, data: &'a DataPool, value: ValueId) -> Option<&'a str> {
    const_string(function, data, value)
}

/// Folds one call after proving the slot has a single immutable literal assignment.
fn fold(function: &mut Function, data: &DataPool, call_id: InstId) -> bool {
    let Some(call) = function.instruction(call_id) else {
        return false;
    };
    let target = match call.immediate {
        Some(Immediate::RuntimeCall(
            RuntimeCallTarget::Function(target)
                | RuntimeCallTarget::ProfiledFunction { target, .. },
        )) => target,
        _ => return false,
    };
    let Some(subject_load) = call.operands.first().copied() else {
        return false;
    };
    let encoding_operand = match target {
        RuntimeFnId::MbSubstrCount | RuntimeFnId::MbStrSplitCount => call.operands[2],
        RuntimeFnId::MbStrposInt
        | RuntimeFnId::MbStrrposInt
        | RuntimeFnId::MbStriposInt
        | RuntimeFnId::MbStrriposInt => call.operands[3],
        _ => call.operands[1],
    };
    let Some((_, encoding_inst)) = defining_instruction_with_id(function, encoding_operand) else {
        return false;
    };
    let encoding_value = if encoding_inst.op == Op::Acquire && encoding_inst.operands.len() == 1 {
        encoding_inst.operands[0]
    } else {
        encoding_operand
    };
    let Some((_, encoding_inst)) = defining_instruction_with_id(function, encoding_value) else {
        return false;
    };
    let (Op::ConstStr, Some(Immediate::Data(encoding_data))) =
        (encoding_inst.op, &encoding_inst.immediate)
    else {
        return false;
    };
    let Some(encoding) = data.strings.get(encoding_data.as_raw() as usize) else {
        return false;
    };
    if !encoding.eq_ignore_ascii_case("UTF-8") {
        return false;
    }
    let Some((_, subject_inst)) = defining_instruction_with_id(function, subject_load) else {
        return false;
    };
    if subject_inst.op != Op::LoadLocal {
        return false;
    }
    let Some(Immediate::LocalSlot(slot)) = subject_inst.immediate else {
        return false;
    };

    let Some(entry) = function.blocks.first() else {
        return false;
    };
    let mut stored = None;
    for (raw, inst) in function.instructions.iter().enumerate() {
        let id = InstId::from_raw(raw as u32);
        let touches_slot = matches!(inst.immediate, Some(Immediate::LocalSlot(found)) if found == slot);
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
    let Some(subject) = const_stored_string(function, data, stored) else {
        return false;
    };
    let subject_bytes = crate::string_bytes::literal_bytes(subject);
    let length = match target {
        RuntimeFnId::MbStrlen => {
            let Some(valid) = std::str::from_utf8(&subject_bytes).ok() else {
                return false;
            };
            valid.chars().count() as i64
        }
        RuntimeFnId::MbStrwidth => {
            if !subject_bytes.is_ascii() {
                return false;
            }
            subject_bytes.len() as i64
        }
        RuntimeFnId::MbCheckEncoding => i64::from(std::str::from_utf8(&subject_bytes).is_ok()),
        RuntimeFnId::MbSubstrCount => {
            let Some(needle) = const_stored_string(function, data, call.operands[1]) else {
                return false;
            };
            let needle = crate::string_bytes::literal_bytes(needle);
            if !subject_bytes.is_ascii() || !needle.is_ascii() || needle.is_empty() {
                return false;
            }
            let mut count = 0i64;
            let mut offset = 0usize;
            while offset + needle.len() <= subject_bytes.len() {
                if subject_bytes[offset..].starts_with(&needle) {
                    count += 1;
                    offset += needle.len();
                } else {
                    offset += 1;
                }
            }
            count
        }
        RuntimeFnId::MbStrSplitCount => {
            let Some(chunk) = const_i64(function, call.operands[1]) else {
                return false;
            };
            if !subject_bytes.is_ascii() || chunk <= 0 {
                return false;
            }
            let chunk = chunk as usize;
            subject_bytes.len().div_ceil(chunk) as i64
        }
        RuntimeFnId::MbStrposInt
        | RuntimeFnId::MbStrrposInt
        | RuntimeFnId::MbStriposInt
        | RuntimeFnId::MbStrriposInt => {
            let Some(needle) = const_stored_string(function, data, call.operands[1]) else {
                return false;
            };
            let needle = crate::string_bytes::literal_bytes(needle);
            let Some(offset) = const_i64(function, call.operands[2]) else {
                return false;
            };
            if !subject_bytes.is_ascii() || !needle.is_ascii() || needle.is_empty() || offset < 0 {
                return false;
            }
            let reverse = matches!(target, RuntimeFnId::MbStrrposInt | RuntimeFnId::MbStrriposInt);
            if reverse && offset != 0 {
                return false;
            }
            let offset = offset as usize;
            let insensitive = matches!(target, RuntimeFnId::MbStriposInt | RuntimeFnId::MbStrriposInt);
            let haystack = if insensitive {
                subject_bytes.to_ascii_lowercase()
            } else {
                subject_bytes.clone()
            };
            let needle = if insensitive {
                needle.to_ascii_lowercase()
            } else {
                needle
            };
            if offset > haystack.len() {
                return false;
            }
            let found = if reverse {
                haystack.windows(needle.len()).rposition(|window| window == needle.as_slice())
            } else {
                haystack[offset..]
                    .windows(needle.len())
                    .position(|window| window == needle.as_slice())
                    .map(|position| position + offset)
            };
            found.map_or(0, |position| position as i64)
        }
        _ => return false,
    };

    let Some(call) = function.instruction_mut(call_id) else {
        return false;
    };
    let Some(result) = call.result else {
        return false;
    };
    call.op = Op::ConstI64;
    call.operands.clear();
    call.immediate = Some(Immediate::I64(length));
    call.result_type = crate::ir::IrType::I64;
    call.result_php_type = PhpType::Int;
    call.result_ownership = Ownership::NonHeap;
    call.effects = Op::ConstI64.default_effects();
    if let Some(value) = function.value_mut(result) {
        value.ir_type = crate::ir::IrType::I64;
        value.php_type = PhpType::Int;
        value.ownership = Ownership::NonHeap;
    }
    // Loads that only fed the folded call (subject, needle, or mask) are now dead.
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
