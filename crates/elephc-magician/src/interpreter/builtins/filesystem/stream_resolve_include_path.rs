//! Purpose:
//! Declarative eval registry entry for `stream_resolve_include_path`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the include-path resolution helper.

eval_builtin! {
    name: "stream_resolve_include_path",
    area: Filesystem,
    params: [filename],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `stream_resolve_include_path` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_stream_resolve_include_path_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_stream_resolve_include_path(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `stream_resolve_include_path` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_stream_resolve_include_path_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [filename] => eval_stream_resolve_include_path_result(*filename, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `stream_resolve_include_path($filename)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_stream_resolve_include_path(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filename] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filename = eval_expr(filename, context, scope, values)?;
    eval_stream_resolve_include_path_result(filename, values)
}

/// Resolves one filename using elephc's realpath-equivalent include-path semantics.
pub(in crate::interpreter) fn eval_stream_resolve_include_path_result(
    filename: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::realpath::eval_realpath_result(filename, values)
}
