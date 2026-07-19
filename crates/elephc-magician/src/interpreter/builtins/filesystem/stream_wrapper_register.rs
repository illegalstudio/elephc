//! Purpose:
//! Declarative eval registry entry and implementation for `stream_wrapper_register`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Registers protocols in the eval stream wrapper registry.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "stream_wrapper_register",
    area: Filesystem,
    params: [
        protocol,
        r#class,
        flags = EvalBuiltinDefaultValue::Int(0)
    ],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Evaluates `stream_wrapper_register($protocol, $class, $flags = 0)`.
pub(in crate::interpreter) fn eval_stream_wrapper_register_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(2..=3).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut evaluated_args = Vec::with_capacity(args.len());
    for arg in args {
        evaluated_args.push(eval_expr(arg, context, scope, values)?);
    }
    eval_stream_wrapper_register_result(&evaluated_args, context, values)
}

/// Registers an already evaluated stream wrapper protocol and class.
pub(in crate::interpreter) fn eval_stream_wrapper_register_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_stream_wrapper_register_result(evaluated_args, context, values)
}

/// Registers a materialized stream wrapper protocol and class.
pub(in crate::interpreter) fn eval_stream_wrapper_register_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(2..=3).contains(&evaluated_args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let protocol = eval_stream_wrapper_protocol(evaluated_args[0], values)?;
    let class_name = eval_stream_wrapper_class(evaluated_args[1], context, values)?;
    values.bool_value(context.stream_resources_mut().register_stream_wrapper(
        &protocol,
        &class_name,
        EVAL_STREAM_WRAPPERS,
    ))
}

/// Coerces one stream wrapper protocol argument into an owned string.
pub(in crate::interpreter) fn eval_stream_wrapper_protocol(
    protocol: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let bytes = values.string_bytes(protocol)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Coerces one stream wrapper class argument into a resolved class-name string.
fn eval_stream_wrapper_class(
    class_name: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let bytes = values.string_bytes(class_name)?;
    let class_name = String::from_utf8_lossy(&bytes).into_owned();
    Ok(context
        .resolve_class_name(&class_name)
        .unwrap_or_else(|| class_name.trim_start_matches('\\').to_string()))
}
