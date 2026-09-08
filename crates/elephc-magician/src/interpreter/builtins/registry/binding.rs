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
    let evaluated_args = bind_evaluated_builtin_args(name, evaluated_args, context, values)?;
    let Some(result) = eval_builtin_with_values(name, &evaluated_args, context, values)? else {
        return Err(EvalStatus::UnsupportedConstruct);
    };
    Ok(result)
}

/// Validates internal arity and binds evaluated arguments using the shared PHP contract.
pub(in crate::interpreter) fn bind_evaluated_builtin_args(
    name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    if evaluated_args.iter().all(|arg| arg.name.is_none()) {
        validate_builtin_argument_count(name, evaluated_args.len(), context, values)?;
        return Ok(evaluated_args.into_iter().map(|arg| arg.value).collect());
    }

    let params = eval_builtin_param_names(name).ok_or(EvalStatus::RuntimeFatal)?;
    let mut bound_args = vec![None; params.len()];
    let mut next_positional = 0;
    let mut saw_named = false;
    let mut overflow = Vec::new();

    for arg in evaluated_args {
        if let Some(arg_name) = arg.name {
            saw_named = true;
            let Some(index) = params.iter().position(|param| *param == arg_name) else {
                return eval_throw_error(&format!("Unknown named parameter ${arg_name}"), context, values);
            };
            if bound_args[index].is_some() {
                return eval_throw_error(&format!("Named parameter ${arg_name} overwrites previous argument"), context, values);
            }
            bound_args[index] = Some(arg.value);
        } else {
            if saw_named {
                return eval_throw_error("Cannot use positional argument after named argument", context, values);
            }
            if next_positional >= bound_args.len() {
                overflow.push(arg.value);
            } else {
                bind_dynamic_positional_arg(&mut bound_args, &mut next_positional, arg.value)?;
            }
        }
    }

    let mut args = collect_bound_builtin_args(name, bound_args, context, values)?;
    args.extend(overflow);
    validate_builtin_argument_count(name, args.len(), context, values)?;
    Ok(args)
}

/// Raises PHP's catchable arity error without letting an adapter drop extra values.
fn validate_builtin_argument_count(
    name: &str,
    count: usize,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let shape = eval_builtin_signature_shape(name).ok_or(EvalStatus::RuntimeFatal)?;
    let contract = elephc_builtin_contract::lookup(name);
    let minimum = contract.and_then(|contract| contract.min_args)
        .unwrap_or(shape.required_param_count);
    let maximum = contract.and_then(|contract| contract.max_args).or_else(|| {
        shape.variadic.is_none().then_some(shape.required_param_count + shape.default_param_count)
    });
    let too_few = count < minimum;
    if !too_few && maximum.is_none_or(|maximum| count <= maximum) {
        return Ok(());
    }
    let (qualifier, expected) = if maximum == Some(minimum) {
        ("exactly", minimum)
    } else if too_few {
        ("at least", minimum)
    } else {
        ("at most", maximum.expect("excess arguments require a finite maximum"))
    };
    let plural = if expected == 1 { "" } else { "s" };
    eval_throw_argument_count_error(
        &format!("{name}() expects {qualifier} {expected} argument{plural}, {count} given"),
        context,
        values,
    )
}

/// Collects ordered builtin arguments, applying PHP defaults for named-call gaps.
pub(in crate::interpreter) fn collect_bound_builtin_args(
    name: &str,
    bound_args: Vec<Option<RuntimeCellHandle>>,
    context: &mut ElephcEvalContext,
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
            let params = eval_builtin_param_names(name).ok_or(EvalStatus::RuntimeFatal)?;
            return eval_throw_argument_count_error(
                &format!("{name}(): Argument #{} (${}) not passed", index + 1, params[index]),
                context,
                values,
            );
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
        EvalBuiltinDefaultValue::EmptyArray => values.array_new(0),
        EvalBuiltinDefaultValue::ClassConstant { class, name } => values
            .class_constant_get(class, name)?
            .ok_or(EvalStatus::RuntimeFatal),
    }
}

/// Returns PHP parameter names for builtin calls implemented by eval.
pub(in crate::interpreter) fn eval_builtin_param_names(
    name: &str,
) -> Option<Vec<&'static str>> {
    if let Some(params) = eval_declared_builtin_param_names(name) {
        return Some(params.to_vec());
    }
    elephc_builtin_contract::lookup(name)
        .map(|contract| contract.params.iter().map(|param| param.name).collect())
}
