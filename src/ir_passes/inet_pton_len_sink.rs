//! Purpose:
//! Replaces `strlen(inet_pton(...))` with the allocation-free `InetPtonLen` helper when
//! the boxed `string|false` result is never observed as a value.
//!
//! Called from:
//! - The EIR fixed-point pass driver after the other length sinks.
//!
//! Key details:
//! - The inner `inet_pton()` result may be observed only by one `Cast` to `Str` plus release
//!   scaffolding. That cast's result may be observed only by one `StrLen` plus releases.
//! - The helper still validates the address and returns 4 for IPv4, 16 for IPv6, or zero for
//!   PHP's `strlen(false)`.
//! - Any other observer keeps the boxed call, including an `=== false` check.

use crate::ir::{
    DataPool, Function, Immediate, InstId, IrType, Op, Ownership, RuntimeCallTarget,
    RuntimeFnId, ValueId,
};
use crate::types::PhpType;

use super::driver::IrPass;
use super::rewrite::{neutralize_to_nop, replace_all_uses};

/// Rewrites length-only `inet_pton()` observations.
pub struct InetPtonLenSink;

impl IrPass for InetPtonLenSink {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "inet_pton_len_sink"
    }

    /// Skips functions with no boxed `inet_pton()` runtime call.
    fn is_applicable(&self, function: &Function) -> bool {
        function.instructions.iter().any(is_boxed_inet_pton)
    }

    /// Specializes every independently proven call.
    fn run(&self, function: &mut Function, _data: &mut DataPool) -> bool {
        let candidates: Vec<InstId> = function
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, inst)| is_boxed_inet_pton(inst))
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

/// Returns whether `inst` is a one-operand `inet_pton()` call producing `string|false`.
fn is_boxed_inet_pton(inst: &crate::ir::Instruction) -> bool {
    matches!(inet_pton_target(inst), Some(RuntimeFnId::InetPton))
        && inst.op == Op::RuntimeCall
        && inst.operands.len() == 1
        && inst.result.is_some()
        && inst.result_php_type.codegen_repr() == PhpType::Mixed
}

/// Returns the stable runtime target of a direct or profiled call instruction.
fn inet_pton_target(inst: &crate::ir::Instruction) -> Option<RuntimeFnId> {
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
    let Some((cast, cast_result)) = analyze_boxed_uses(function, result) else {
        return false;
    };
    let Some((length, length_result, mut releases)) = analyze_length_uses(function, cast_result) else {
        return false;
    };
    releases.extend(analyze_releases(function, result));

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
            target: RuntimeFnId::InetPtonLen,
            arguments: *arguments,
            strict_php: *strict_php,
            strict_types: *strict_types,
        },
        _ => RuntimeCallTarget::Function(RuntimeFnId::InetPtonLen),
    }));
    call.result_type = IrType::I64;
    call.result_php_type = PhpType::Int;
    call.result_ownership = Ownership::NonHeap;
    call.effects = RuntimeFnId::InetPtonLen.intrinsic_effects();
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

/// Returns every direct release of the boxed call result.
fn analyze_releases(function: &Function, result: ValueId) -> Vec<InstId> {
    function
        .instructions
        .iter()
        .enumerate()
        .filter(|(_, inst)| inst.op == Op::Release && inst.operands.as_slice() == [result])
        .map(|(raw, _)| InstId::from_raw(raw as u32))
        .collect()
}
