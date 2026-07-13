//! Purpose:
//! Declarative eval registry entry and implementation for `stream_filter_register`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Eval conservatively accepts registrations without mutating stream bytes.

eval_builtin! {
    name: "stream_filter_register",
    area: Filesystem,
    params: [filter_name, r#class],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Evaluates `stream_filter_register($filter_name, $class)`.
pub(in crate::interpreter) fn eval_stream_filter_register_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filter_name, class] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let filter_name = eval_expr(filter_name, context, scope, values)?;
    let class = eval_expr(class, context, scope, values)?;
    eval_stream_filter_register_result(filter_name, class, values)
}

/// Registers an already evaluated stream filter name and class pair.
pub(in crate::interpreter) fn eval_stream_filter_register_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [filter_name, class] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_stream_filter_register_result(*filter_name, *class, values)
}

/// Evaluates a materialized `stream_filter_register()` call.
pub(in crate::interpreter) fn eval_stream_filter_register_result(
    filter_name: RuntimeCellHandle,
    class: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let _ = values.string_bytes(filter_name)?;
    let _ = values.string_bytes(class)?;
    values.bool_value(true)
}
