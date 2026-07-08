//! Purpose:
//! Declarative eval registry entry for `stream_context_get_default`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Returns eval's shared default stream context resource.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "stream_context_get_default",
    area: Filesystem,
    params: [options = EvalBuiltinDefaultValue::Null],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Evaluates `stream_context_get_default($options = null)`.
pub(in crate::interpreter) fn eval_stream_context_get_default_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() > 1 {
        return Err(EvalStatus::RuntimeFatal);
    }
    for arg in args {
        eval_expr(arg, context, scope, values)?;
    }
    eval_stream_context_get_default_result(context, values)
}

/// Returns the default context after validating already evaluated arguments.
pub(in crate::interpreter) fn eval_stream_context_get_default_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if evaluated_args.len() > 1 {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_stream_context_get_default_result(context, values)
}

/// Returns eval's default stream context resource.
pub(in crate::interpreter) fn eval_stream_context_get_default_result(
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = context.stream_resources_mut().default_stream_context();
    values.resource(id)
}
