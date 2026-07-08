//! Purpose:
//! Declarative eval registry entry for `stream_select`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Direct calls keep their source-sensitive by-reference array path.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "stream_select",
    area: Filesystem,
    params: [
        read: by_ref,
        write: by_ref,
        except: by_ref,
        seconds,
        microseconds = EvalBuiltinDefaultValue::Int(0)
    ],
    by_ref: [read, write, except],
    direct: none,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `stream_select` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_stream_select_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::direct_dispatch::eval_builtin_filesystem_call_impl("stream_select", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `stream_select` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_stream_select_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::values_dispatch::eval_filesystem_values_result_impl("stream_select", evaluated_args, context, values)
}
