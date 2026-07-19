//! Purpose:
//! Named and spread argument binding for builtin calls.
//!
//! Called from:
//! - `crate::interpreter::builtins::registry` re-exports.
//!
//! Key details:
//! - Helpers are scoped to the eval interpreter and operate on already parsed
//!   EvalIR call metadata or evaluated runtime-cell handles.

use super::*;

/// Evaluates a direct PHP-visible builtin call with named or spread arguments.
pub(in crate::interpreter) fn eval_builtin_call(
    name: &str,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let evaluated_args = eval_call_arg_values(args, context, scope, values)?;
    let evaluated_args = bind_evaluated_builtin_args(name, evaluated_args, values)?;
    let Some(result) = eval_builtin_with_values(name, &evaluated_args, context, values)? else {
        return Err(EvalStatus::UnsupportedConstruct);
    };
    Ok(result)
}

/// Binds evaluated builtin arguments to PHP parameter order when names are used.
pub(in crate::interpreter) fn bind_evaluated_builtin_args(
    name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    if evaluated_args.iter().all(|arg| arg.name.is_none()) {
        return Ok(evaluated_args.into_iter().map(|arg| arg.value).collect());
    }

    let params = eval_builtin_param_names(name).ok_or(EvalStatus::RuntimeFatal)?;
    let mut bound_args = vec![None; params.len()];
    let mut next_positional = 0;

    for arg in evaluated_args {
        if let Some(name) = arg.name {
            bind_builtin_named_arg(params, &mut bound_args, &name, arg.value)?;
        } else {
            bind_dynamic_positional_arg(&mut bound_args, &mut next_positional, arg.value)?;
        }
    }

    collect_bound_builtin_args(name, bound_args, values)
}

/// Binds one named builtin-call value to the matching PHP parameter slot.
pub(in crate::interpreter) fn bind_builtin_named_arg(
    params: &[&str],
    bound_args: &mut [Option<RuntimeCellHandle>],
    name: &str,
    value: RuntimeCellHandle,
) -> Result<(), EvalStatus> {
    let Some(param_index) = params.iter().position(|param| *param == name) else {
        return Err(EvalStatus::RuntimeFatal);
    };
    if bound_args[param_index].is_some() {
        return Err(EvalStatus::RuntimeFatal);
    }
    bound_args[param_index] = Some(value);
    Ok(())
}

/// Collects ordered builtin arguments, applying PHP defaults for named-call gaps.
pub(in crate::interpreter) fn collect_bound_builtin_args(
    name: &str,
    bound_args: Vec<Option<RuntimeCellHandle>>,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    if !bound_args.iter().any(Option::is_some) {
        return Ok(Vec::new());
    }

    let shape = eval_builtin_signature_shape(name).ok_or(EvalStatus::RuntimeFatal)?;
    let last_index = bound_args
        .iter()
        .rposition(Option::is_some)
        .expect("non-empty bound args has a last supplied arg");
    let mut args = Vec::with_capacity(last_index + 1);

    for (index, arg) in bound_args.into_iter().take(last_index + 1).enumerate() {
        if let Some(value) = arg {
            args.push(value);
        } else if index >= shape.required_param_count {
            args.push(eval_builtin_default_arg(name, index, values)?);
        } else {
            return Err(EvalStatus::RuntimeFatal);
        }
    }

    Ok(args)
}

/// Materializes one builtin default argument as a runtime cell.
fn eval_builtin_default_arg(
    name: &str,
    index: usize,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match eval_builtin_default_value(name, index).ok_or(EvalStatus::RuntimeFatal)? {
        EvalBuiltinDefaultValue::Null => values.null(),
        EvalBuiltinDefaultValue::Bool(value) => values.bool_value(value),
        EvalBuiltinDefaultValue::Int(value) => values.int(value),
        EvalBuiltinDefaultValue::Float(value) => values.float(value),
        EvalBuiltinDefaultValue::String(value) => values.string(value),
        EvalBuiltinDefaultValue::Bytes(value) => values.string_bytes_value(value),
        EvalBuiltinDefaultValue::EmptyArray => values.array_new(0),
    }
}

/// Returns PHP parameter names for builtin calls implemented by eval.
pub(in crate::interpreter) fn eval_builtin_param_names(
    name: &str,
) -> Option<&'static [&'static str]> {
    if let Some(params) = eval_declared_builtin_param_names(name) {
        return Some(params);
    }

    None
}
