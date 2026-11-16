//! Purpose:
//! Replaces `strlen(strip_tags(...))` with the count-only mode of `__rt_strip_tags`
//! when the stripped string is never observed as a value.
//!
//! Called from:
//! - The EIR fixed-point pass driver after the other length sinks.
//!
//! Key details:
//! - The `strip_tags()` result may be observed only by one `StrLen` and its release
//!   scaffolding. Any other observer, including a store or a string context, keeps the
//!   allocating helper.
//! - The runtime helper still evaluates the subject and the allow-list exactly once and
//!   keeps PHP's array/Mixed allow-list coercion, tag matching, and diagnostics.
//! - The helper returns a non-heap integer, so the produced string's cleanup is retired.

use crate::ir::{
    DataPool, Function, Immediate, InstId, IrType, Op, Ownership, RuntimeCallTarget,
    RuntimeFnId, ValueId,
};
use crate::types::PhpType;

use super::driver::IrPass;
use super::rewrite::{neutralize_to_nop, replace_all_uses};

/// Rewrites length-only `strip_tags()` observations.
pub struct StripTagsLenSink;

impl IrPass for StripTagsLenSink {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "strip_tags_len_sink"
    }

    /// Skips functions with no `strip_tags()` runtime call.
    fn is_applicable(&self, function: &Function) -> bool {
        function.instructions.iter().any(is_strip_tags_call)
    }

    /// Specializes every independently proven call.
    fn run(&self, function: &mut Function, _data: &mut DataPool) -> bool {
        let candidates: Vec<InstId> = function
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, inst)| is_strip_tags_call(inst))
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

/// Returns whether `inst` is a one/two-operand `strip_tags()` call producing a string.
fn is_strip_tags_call(inst: &crate::ir::Instruction) -> bool {
    matches!(strip_tags_target(inst), Some(RuntimeFnId::StripTags))
        && matches!(inst.operands.len(), 1 | 2)
        && inst.result.is_some()
        && inst.result_php_type.codegen_repr() == PhpType::Str
}

/// Returns the stable runtime target of a direct or profiled call instruction.
fn strip_tags_target(inst: &crate::ir::Instruction) -> Option<RuntimeFnId> {
    let Some(Immediate::RuntimeCall(target)) = &inst.immediate else {
        return None;
    };
    match target {
        RuntimeCallTarget::Function(target)
        | RuntimeCallTarget::ProfiledFunction { target, .. } => Some(*target),
        _ => None,
    }
}

/// Specializes one candidate after proving its complete use graph.
fn specialize(function: &mut Function, candidate: InstId) -> bool {
    let Some(call) = function.instruction(candidate) else {
        return false;
    };
    let Some(result) = call.result else {
        return false;
    };
    let Some((length, length_result, releases)) = analyze_uses(function, result) else {
        return false;
    };

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
            target: RuntimeFnId::StripTagsLen,
            arguments: *arguments,
            strict_php: *strict_php,
            strict_types: *strict_types,
        },
        _ => RuntimeCallTarget::Function(RuntimeFnId::StripTagsLen),
    }));
    call.result_type = IrType::I64;
    call.result_php_type = PhpType::Int;
    call.result_ownership = Ownership::NonHeap;
    call.effects = RuntimeFnId::StripTagsLen.intrinsic_effects();
    if let Some(value) = function.value_mut(result) {
        value.ir_type = IrType::I64;
        value.php_type = PhpType::Int;
        value.ownership = Ownership::NonHeap;
    }
    replace_all_uses(
        function,
        &std::collections::HashMap::from([(length_result, result)]),
    );
    for id in std::iter::once(length).chain(releases) {
        if let Some(inst) = function.instruction_mut(id) {
            neutralize_to_nop(inst);
        }
    }
    true
}

/// Returns the single `StrLen` observer of `result` and the releases of the string value.
fn analyze_uses(function: &Function, result: ValueId) -> Option<(InstId, ValueId, Vec<InstId>)> {
    let mut length = None;
    let mut releases = Vec::new();
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !inst.operands.contains(&result) {
            continue;
        }
        let id = InstId::from_raw(raw as u32);
        match inst.op {
            Op::StrLen if inst.operands.as_slice() == [result] && inst.result.is_some() => {
                if length.replace((id, inst.result?)).is_some() {
                    return None;
                }
            }
            Op::Release if inst.operands.as_slice() == [result] => releases.push(id),
            _ => return None,
        }
    }
    let (length, length_result) = length?;
    Some((length, length_result, releases))
}
