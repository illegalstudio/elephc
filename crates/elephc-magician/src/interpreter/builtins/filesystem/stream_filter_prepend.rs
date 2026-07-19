//! Purpose:
//! Declarative eval registry entry and implementation for `stream_filter_prepend`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Creates eval-local filter resources without transforming stream bytes.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "stream_filter_prepend",
    area: Filesystem,
    params: [
        stream,
        filtername,
        read_write = EvalBuiltinDefaultValue::Int(3),
        params = EvalBuiltinDefaultValue::Null
    ],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Evaluates `stream_filter_prepend($stream, $filtername, $read_write = 3, $params = null)`.
pub(in crate::interpreter) fn eval_stream_filter_prepend_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(2..=4).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let stream = eval_expr(&args[0], context, scope, values)?;
    let filter_name = eval_expr(&args[1], context, scope, values)?;
    for arg in &args[2..] {
        let _ = eval_expr(arg, context, scope, values)?;
    }
    super::stream_filter_append::eval_stream_filter_attach_result(
        "stream_filter_prepend",
        stream,
        filter_name,
        context,
        values,
    )
}

/// Prepends a filter from already evaluated stream filter arguments.
pub(in crate::interpreter) fn eval_stream_filter_prepend_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(2..=4).contains(&evaluated_args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    super::stream_filter_append::eval_stream_filter_attach_result(
        "stream_filter_prepend",
        evaluated_args[0],
        evaluated_args[1],
        context,
        values,
    )
}
