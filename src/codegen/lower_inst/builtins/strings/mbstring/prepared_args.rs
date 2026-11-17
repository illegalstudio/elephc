//! Purpose:
//! Restricts mbstring fast paths to values that need no PHP argument preparation.
//!
//! Called from:
//! - The shared mbstring codegen coordinator before native shortcut selection.
//!
//! Key details:
//! - Parameter types come from the builtin contract, not a second function-name table.
//! - Coercions, Stringable callbacks and diagnostics remain with the shared invoker.
//! - Unknown, reference and container shapes conservatively retain preparation.

use super::{CodegenIrError, FunctionContext, Instruction, PhpType, Result};
use elephc_builtin_contract::{RuntimeBuiltinId, TypeSpec};

/// Proves every supplied argument already has a canonical declared scalar type.
pub(super) fn already_prepared(
    ctx: &FunctionContext<'_>,
    inst: &Instruction,
    operation: RuntimeBuiltinId,
) -> Result<bool> {
    let contract = elephc_builtin_contract::lookup_id(operation.builtin_id())
        .expect("mbstring contract");
    for (index, value) in inst.operands.iter().enumerate() {
        let Some(parameter) = contract.params.get(index) else { return Ok(false); };
        let actual = &ctx.function.value(*value)
            .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))?.php_type;
        if parameter.by_ref || !matches_without_coercion(actual, parameter.ty) {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Returns whether one `mb_chr()` observation can use the allocation-free encoder.
///
/// The codepoint operand may be a tagged scalar: a packed `array<int>` read narrows to
/// `TaggedScalar` (int or missing null), and `load_as_int` converts it without allocating.
/// The encoding must still be an exact `string` so the explicit-UTF-8 check can see it.
pub(super) fn chr_operands_prepared(ctx: &FunctionContext<'_>, inst: &Instruction) -> Result<bool> {
    if inst.operands.len() != 2 {
        return Ok(false);
    }
    let codepoint = &ctx.function.value(inst.operands[0])
        .ok_or_else(|| CodegenIrError::missing_entry("value", inst.operands[0].as_raw()))?
        .php_type;
    if !matches!(
        codepoint.codegen_repr(),
        PhpType::Int | PhpType::Bool | PhpType::TaggedScalar
    ) {
        return Ok(false);
    }
    let encoding = &ctx.function.value(inst.operands[1])
        .ok_or_else(|| CodegenIrError::missing_entry("value", inst.operands[1].as_raw()))?
        .php_type;
    Ok(*encoding == PhpType::Str)
}

/// Checks type identity only; the invoker remains authoritative for all PHP conversions.
fn matches_without_coercion(actual: &PhpType, expected: TypeSpec) -> bool {
    if let PhpType::Union(alternatives) = actual {
        return !alternatives.is_empty()
            && alternatives.iter().all(|ty| matches_without_coercion(ty, expected));
    }
    match expected {
        TypeSpec::Int => *actual == PhpType::Int,
        TypeSpec::Float => *actual == PhpType::Float,
        TypeSpec::Str => *actual == PhpType::Str,
        TypeSpec::Bool => matches!(actual, PhpType::Bool | PhpType::False),
        // Null can select a getter or a request-dependent default instead of a setter.
        TypeSpec::Null => false,
        TypeSpec::Nullable(inner) => matches_without_coercion(actual, *inner),
        TypeSpec::Union(alternatives) => alternatives.iter()
            .any(|ty| matches_without_coercion(actual, *ty)),
        _ => false,
    }
}
