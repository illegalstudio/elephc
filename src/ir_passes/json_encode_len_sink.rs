//! Purpose:
//! Removes the Mixed boxing and string-cast copies when `json_encode()` is observed only
//! through `strlen((string) json_encode(...))`.
//!
//! Called from:
//! - The EIR fixed-point pass driver after the length sinks.
//!
//! Key details:
//! - The encoder still runs with its original flags, depth, error state and throws; only the
//!   result representation changes to the encoder's raw string.
//! - The raw string is consumed immediately by one `str_len`; a scratch-backed result stays
//!   valid across that leaf and a grown heap block is freed by the string's own release.
//! - A JSON error without `JSON_PARTIAL_OUTPUT_ON_ERROR` lowers to the empty string, exactly
//!   like `(string) false`, so the observed length remains zero.
//! - Any other observer keeps the boxed `json_encode()` call.

use crate::ir::{
    DataPool, Function, Immediate, InstId, IrType, Op, Ownership, RuntimeCallTarget, RuntimeFnId,
    ValueId,
};
use crate::types::PhpType;

use super::driver::IrPass;
use super::rewrite::{neutralize_to_nop, replace_all_uses};

/// Rewrites length-only JSON encodings to the raw string encoder.
pub struct JsonEncodeLenSink;

impl IrPass for JsonEncodeLenSink {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "json_encode_len_sink"
    }

    /// Skips functions with no boxed `json_encode()` runtime call.
    fn is_applicable(&self, function: &Function) -> bool {
        function.instructions.iter().any(is_boxed_json_encode)
    }

    /// Specializes every independently proven call.
    fn run(&self, function: &mut Function, _data: &mut DataPool) -> bool {
        let candidates: Vec<InstId> = function
            .instructions
            .iter()
            .enumerate()
            .filter(|(_, inst)| is_boxed_json_encode(inst))
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

/// Returns whether `inst` is a one-to-three-operand `json_encode()` call producing Mixed.
fn is_boxed_json_encode(inst: &crate::ir::Instruction) -> bool {
    inst.op == Op::RuntimeCall
        && matches!(
            inst.immediate,
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                RuntimeFnId::JsonEncode
            )))
        )
        && matches!(inst.operands.len(), 1 | 2 | 3)
        && inst.result.is_some()
        && inst.result_php_type.codegen_repr() == PhpType::Mixed
}

/// Returns the single Mixed-to-string cast and the removable releases of the encoder result.
fn analyze_encode_uses(
    function: &Function,
    result: ValueId,
) -> Option<(InstId, ValueId, Vec<InstId>)> {
    let mut cast = None;
    let mut releases = Vec::new();
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !inst.operands.contains(&result) {
            continue;
        }
        let id = InstId::from_raw(raw as u32);
        match inst.op {
            Op::Cast
                if inst.operands.as_slice() == [result]
                    && inst.immediate == Some(Immediate::CastTarget(IrType::Str))
                    && inst.result_php_type.codegen_repr() == PhpType::Str =>
            {
                let cast_result = inst.result?;
                if cast.replace((id, cast_result)).is_some() {
                    return None;
                }
            }
            Op::Release if inst.operands.as_slice() == [result] => releases.push(id),
            _ => return None,
        }
    }
    for block in &function.blocks {
        if block
            .terminator
            .as_ref()
            .is_some_and(|term| term.used_values().contains(&result))
        {
            return None;
        }
    }
    let (cast, cast_result) = cast?;
    Some((cast, cast_result, releases))
}

/// Returns the single `str_len` observer and exactly one release of the string result.
fn analyze_str_uses(function: &Function, result: ValueId) -> Option<(InstId, InstId)> {
    let mut length = None;
    let mut release = None;
    for (raw, inst) in function.instructions.iter().enumerate() {
        if !inst.operands.contains(&result) {
            continue;
        }
        let id = InstId::from_raw(raw as u32);
        match inst.op {
            Op::StrLen if inst.operands.as_slice() == [result] && inst.result.is_some() => {
                if length.replace(id).is_some() {
                    return None;
                }
            }
            Op::Release if inst.operands.as_slice() == [result] => {
                if release.replace(id).is_some() {
                    return None;
                }
            }
            _ => return None,
        }
    }
    for block in &function.blocks {
        if block
            .terminator
            .as_ref()
            .is_some_and(|term| term.used_values().contains(&result))
        {
            return None;
        }
    }
    Some((length?, release?))
}

/// Specializes one candidate after proving the cast, the length observer and ownership.
fn specialize(function: &mut Function, candidate: InstId) -> bool {
    let Some(call) = function.instruction(candidate) else {
        return false;
    };
    let Some(result) = call.result else {
        return false;
    };
    let Some((cast, cast_result, encode_releases)) = analyze_encode_uses(function, result) else {
        return false;
    };
    if analyze_str_uses(function, cast_result).is_none() {
        return false;
    }

    let Some(call) = function.instruction_mut(candidate) else {
        return false;
    };
    call.immediate = Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
        RuntimeFnId::JsonEncodeStr,
    )));
    call.result_type = IrType::Str;
    call.result_php_type = PhpType::Str;
    call.result_ownership = Ownership::Owned;
    call.effects = RuntimeFnId::JsonEncodeStr.intrinsic_effects();
    if let Some(value) = function.value_mut(result) {
        value.ir_type = IrType::Str;
        value.php_type = PhpType::Str;
        value.ownership = Ownership::Owned;
    }
    replace_all_uses(
        function,
        &std::collections::HashMap::from([(cast_result, result)]),
    );
    if let Some(inst) = function.instruction_mut(cast) {
        neutralize_to_nop(inst);
    }
    for release in encode_releases {
        if let Some(inst) = function.instruction_mut(release) {
            neutralize_to_nop(inst);
        }
    }
    true
}
