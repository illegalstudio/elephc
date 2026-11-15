//! Purpose:
//! Applies neutral builtin parameter types before evaluated value hooks can coerce inputs.
//!
//! Called from:
//! - The shared direct, named, spread and callable builtin values dispatcher.
//!
//! Key details:
//! - Contract metadata owns scalar, nullable and union alternatives without a second name table.
//! - Output references keep their callee-owned validation and writeback protocol.
//! - Candidates stay borrowed and all argument owners remain with the existing call boundary.

use super::*;
use elephc_builtin_contract::{DefaultSpec, TypeSpec};

/// Rejects implicit conversion of a fixed builtin input before invoking its implementation.
pub(super) fn validate_builtin_value_contracts(
    name: &str, arguments: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if !context.semantic_profile().contains(elephc_semantics::StrictFeature::Coercions) { return Ok(()); }
    let Some(contract) = elephc_builtin_contract::lookup(name) else { return Ok(()); };
    for (index, (parameter, value)) in contract.params.iter().zip(arguments).enumerate() {
        // A reference can be a write-only output whose previous kind is irrelevant.
        if parameter.by_ref { continue; }
        let tag = values.type_tag(*value)?;
        // A declared null default is an explicit sentinel, including a named-call gap.
        if tag == EVAL_TAG_NULL && parameter.default == Some(DefaultSpec::Null) { continue; }
        if !builtin_type_accepts_exact(parameter.ty, *value, tag, context, values)? {
            return eval_throw_type_error(&format!(
                "{name}(): Argument #{} (${}) must be of type {} (--strict-coercions)",
                index + 1, parameter.name, parameter.ty), context, values);
        }
    }
    Ok(())
}

/// Tests the existing PHP value against a neutral type, without widening or string conversion.
fn builtin_type_accepts_exact(
    ty: TypeSpec, value: RuntimeCellHandle, tag: u64,
    context: &ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    Ok(match ty {
        TypeSpec::Mixed => true,
        TypeSpec::Int | TypeSpec::Ptr => tag == EVAL_TAG_INT,
        TypeSpec::Float => tag == EVAL_TAG_FLOAT,
        TypeSpec::Bool => tag == EVAL_TAG_BOOL,
        TypeSpec::False => tag == EVAL_TAG_BOOL && !values.truthy(value)?,
        TypeSpec::Null => tag == EVAL_TAG_NULL,
        TypeSpec::Str => tag == EVAL_TAG_STRING,
        TypeSpec::Object => matches!(tag, EVAL_TAG_OBJECT | EVAL_TAG_CALLABLE),
        TypeSpec::Array => matches!(tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC),
        TypeSpec::Callable => eval_is_callable_value(value, None, context, values)?,
        TypeSpec::Void => false,
        TypeSpec::Nullable(inner) => tag == EVAL_TAG_NULL
            || builtin_type_accepts_exact(*inner, value, tag, context, values)?,
        TypeSpec::Union(members) => {
            let mut accepted = false;
            for member in members {
                if builtin_type_accepts_exact(*member, value, tag, context, values)? { accepted = true; break; }
            }
            accepted
        }
    })
}
