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
use super::super::spec::EvalParamSpec;
use elephc_builtin_contract::TypeSpec;

/// Rejects implicit conversion before the selected evaluated builtin hook receives its inputs.
pub(super) fn validate_builtin_value_contracts(
    name: &str, arguments: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if !context.semantic_profile().contains(elephc_semantics::StrictFeature::Coercions) { return Ok(()); }
    let Some(spec) = eval_declared_builtin_spec(name) else { return Ok(()); };
    for (index, (parameter, value)) in spec.params.iter().zip(arguments).enumerate() {
        validate_parameter(name, index, parameter, *value, context, values)?;
    }
    Ok(())
}

/// Checks captured source operands with the same named/reference binder used by builtin adapters.
pub(in crate::interpreter) fn validate_builtin_call_arguments(
    name: &str, arguments: &[EvaluatedCallArg],
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if !context.semantic_profile().contains(elephc_semantics::StrictFeature::Coercions) { return Ok(()); }
    let Some(spec) = eval_declared_builtin_spec(name) else { return Ok(()); };
    let (bound, _) = bind_evaluated_ref_builtin_args(&spec.param_names, arguments, spec.variadic.is_some())?;
    // The existing callee retains its missing-required-argument diagnostic priority.
    if bound.iter().take(spec.required_param_count()).any(Option::is_none) { return Ok(()); }
    for (index, (parameter, argument)) in spec.params.iter().zip(&bound).enumerate() {
        if let Some(argument) = argument {
            validate_parameter(name, index, parameter, argument.value, context, values)?;
        }
    }
    Ok(())
}

/// Shares argument owners with original source handlers and validates only after source evaluation.
pub(in crate::interpreter) fn with_eval_builtin_call_arguments<V: RuntimeValueOps>(
    name: &str, args: &[EvalCallArg], context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut V,
    consume: impl FnOnce(Vec<EvaluatedCallArg>, &mut ElephcEvalContext, &mut ElephcEvalScope, &mut V)
        -> Result<RuntimeCellHandle, EvalStatus>,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let modes = if context.semantic_profile().contains(elephc_semantics::StrictFeature::Coercions) {
        CallArgumentModes::for_builtin(name)
    } else { CallArgumentModes::default() };
    with_eval_call_arguments_with_modes(args, &modes, context, scope, values, |arguments, context, scope, values| {
        validate_builtin_call_arguments(name, &arguments, context, values)?;
        consume(arguments, context, scope, values)
    })
}

/// Applies one selected neutral contract while preserving explicit defaults and output references.
fn validate_parameter(
    name: &str, index: usize, parameter: &EvalParamSpec, value: RuntimeCellHandle,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if parameter.by_ref { return Ok(()); }
    let tag = values.type_tag(value)?;
    if tag == EVAL_TAG_NULL && parameter.default == Some(EvalBuiltinDefaultValue::Null) { return Ok(()); }
    if builtin_type_accepts_exact(parameter.ty, value, tag, context, values)? { return Ok(()); }
    eval_throw_type_error(&format!(
        "{name}(): Argument #{} (${}) must be of type {} (--strict-coercions)",
        index + 1, parameter.name, parameter.ty), context, values)
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
