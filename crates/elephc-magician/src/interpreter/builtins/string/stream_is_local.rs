//! Purpose:
//! Declarative eval registry entry for `stream_is_local`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the stream boolean predicate helper.

eval_builtin! {
    contract: "stream_is_local",
    area: String,
    direct: StreamBoolPredicate,
    values: StreamBoolPredicate,
}

use super::super::super::*;

/// Evaluates PHP `stream_is_local(...)` over one stream expression.
pub(in crate::interpreter) fn eval_builtin_stream_is_local(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::stream_is_local::eval_builtin_stream_bool_predicate_named("stream_is_local", args, context, scope, values)
}

/// Builds the result for PHP `stream_is_local(...)` from one evaluated stream value.
pub(in crate::interpreter) fn eval_stream_is_local_result(
    stream: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::stream_is_local::eval_stream_bool_predicate_named_result("stream_is_local", stream, values)
}

/// Evaluates a named stream boolean predicate over one eval expression.
pub(in crate::interpreter) fn eval_builtin_stream_bool_predicate_named(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let max_args = if name == "stream_is_local" { 2 } else { 1 };
    if args.is_empty() || args.len() > max_args {
        return Err(EvalStatus::RuntimeFatal);
    }
    let stream = eval_expr(&args[0], context, scope, values)?;
    if let Some(context_arg) = args.get(1) {
        let _ = eval_expr(context_arg, context, scope, values)?;
    }
    eval_stream_bool_predicate_named_result(name, stream, values)
}

/// Returns elephc's fixed stream-locality and lock-support predicate values.
pub(in crate::interpreter) fn eval_stream_bool_predicate_named_result(
    name: &str,
    stream: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "stream_is_local" => values.bool_value(true),
        "stream_supports_lock" => {
            if values.type_tag(stream)? != EVAL_TAG_RESOURCE {
                return Err(EvalStatus::RuntimeFatal);
            }
            values.bool_value(true)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
