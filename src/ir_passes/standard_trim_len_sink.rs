//! Purpose:
//! Folds `strlen(ltrim($literal))`, `strlen(rtrim($literal))`, and
//! `strlen(trim($literal))` when the subject and optional character mask are
//! compile-time strings, removing the allocation/copy from the observation.
//!
//! Called from:
//! - The EIR fixed-point pass driver after the string length sinks.
//!
//! Key details:
//! - Only the standard one- or two-operand trim forms are eligible. A mask
//!   containing PHP's `..` range syntax stays on the runtime path.
//! - The trim result, including an optional owned persistence, may be observed only
//!   by the `str_len` and release scaffolding; any other use keeps the call.
//! - The folded length is PHP's byte length after applying the mask; the default
//!   mask is `" \t\n\r\0\x0B"`.

use crate::ir::{
    DataPool, Function, Immediate, InstId, Op, RuntimeCallTarget, RuntimeFnId, ValueId,
};
use super::driver::IrPass;
use super::rewrite::neutralize_to_nop;
use super::string_len_sink::{
    const_local_string, const_string_value, trimmed_ascii_len,
};

/// Folds standard trim calls observed only through `strlen()`.
pub struct StandardTrimLenSink;

impl IrPass for StandardTrimLenSink {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "standard_trim_len_sink"
    }

    /// Skips functions without a literal trim observed by `strlen()`.
    fn is_applicable(&self, function: &Function) -> bool {
        function.instructions.iter().any(is_candidate)
    }

    /// Folds every independently proven trim.
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

/// Returns whether `inst` is a `str_len` consuming a standard trim call.
fn is_candidate(inst: &crate::ir::Instruction) -> bool {
    inst.op == Op::StrLen && inst.operands.len() == 1
}

/// Returns the runtime target of a direct or profiled trim call.
fn trim_target(inst: &crate::ir::Instruction) -> Option<RuntimeFnId> {
    let target = match inst.immediate {
        Some(Immediate::RuntimeCall(
            RuntimeCallTarget::Function(target)
                | RuntimeCallTarget::ProfiledFunction { target, .. },
        )) => target,
        _ => return None,
    };
    matches!(
        target,
        RuntimeFnId::Ltrim | RuntimeFnId::Rtrim | RuntimeFnId::Trim
    )
    .then_some(target)
}

/// Resolves a literal value from a direct constant or an immutable local.
fn literal_string(function: &Function, data: &DataPool, value: ValueId) -> Option<String> {
    const_string_value(function, data, value)
        .or_else(|| const_local_string(function, data, value).map(|(literal, _)| literal))
}

/// Folds one `strlen(trim(...))` after proving the complete use graph.
fn fold(function: &mut Function, data: &DataPool, length_id: InstId) -> bool {
    let Some(length) = function.instruction(length_id) else {
        return false;
    };
    let Some(subject_value) = length.operands.first().copied() else {
        return false;
    };
    let Some(definition) = function.value(subject_value) else {
        return false;
    };
    let crate::ir::ValueDef::Instruction {
        inst: mut producer_id, ..
    } = definition.def
    else {
        return false;
    };
    let Some(mut producer) = function.instruction(producer_id) else {
        return false;
    };
    // Argument parking can require an owned copy of a borrowed trim view. Length-only
    // observations do not need that copy, but its complete use graph must be proven too.
    let mut observations = vec![(subject_value, length_id)];
    let persistence = if producer.op == Op::StrPersist && producer.operands.len() == 1 {
        let trim_value = producer.operands[0];
        let Some(crate::ir::ValueDef::Instruction { inst, .. }) =
            function.value(trim_value).map(|value| value.def)
        else {
            return false;
        };
        observations.push((trim_value, producer_id));
        let persistence = producer_id;
        producer_id = inst;
        let Some(trim) = function.instruction(producer_id) else {
            return false;
        };
        producer = trim;
        Some(persistence)
    } else {
        None
    };
    let Some(target) = trim_target(producer) else {
        return false;
    };
    if !matches!(producer.operands.len(), 1 | 2) {
        return false;
    }
    let mut releases = Vec::new();
    for (value, consumer) in observations {
        if function.blocks.iter().any(|block| block.terminator.as_ref()
            .is_some_and(|term| term.used_values().contains(&value)))
        {
            return false;
        }
        for (raw, inst) in function.instructions.iter().enumerate() {
            if !inst.operands.contains(&value) {
                continue;
            }
            let id = InstId::from_raw(raw as u32);
            if id == consumer {
                continue;
            }
            match inst.op {
                Op::Release | Op::ReleaseUnlessAliases
                    if inst.operands.first() == Some(&value) => releases.push(id),
                _ => return false,
            }
        }
    }
    let Some(subject) = literal_string(function, data, producer.operands[0]) else {
        return false;
    };
    let mask = match producer.operands.get(1).copied() {
        None => b" \t\n\r\0\x0B".to_vec(),
        Some(mask_value) => {
            let Some(mask) = literal_string(function, data, mask_value) else {
                return false;
            };
            if mask.contains("..") {
                return false;
            }
            mask.into_bytes()
        }
    };
    let (left, right) = match target {
        RuntimeFnId::Ltrim => (true, false),
        RuntimeFnId::Rtrim => (false, true),
        RuntimeFnId::Trim => (true, true),
        _ => return false,
    };
    let folded = trimmed_ascii_len(subject.as_bytes(), &mask, left, right);
    let Some(length) = function.instruction_mut(length_id) else {
        return false;
    };
    length.op = Op::ConstI64;
    length.operands.clear();
    length.immediate = Some(Immediate::I64(folded));
    length.effects = Op::ConstI64.default_effects();
    if let Some(inst) = function.instruction_mut(producer_id) {
        neutralize_to_nop(inst);
    }
    if let Some(persistence) = persistence {
        if let Some(inst) = function.instruction_mut(persistence) {
            neutralize_to_nop(inst);
        }
    }
    for id in releases {
        if let Some(inst) = function.instruction_mut(id) {
            neutralize_to_nop(inst);
        }
    }
    true
}
