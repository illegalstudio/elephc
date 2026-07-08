//! Purpose:
//! Declarative eval registry entry for `get_called_class`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the current-class scope helper.

eval_builtin! {
    name: "get_called_class",
    area: Symbols,
    params: [],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `get_called_class` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_get_called_class_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    _scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::super::eval_builtin_get_called_class(args, context, values)
}

/// Dispatches evaluated-argument calls for the `get_called_class` symbol builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_get_called_class_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if evaluated_args.is_empty() { super::super::eval_get_called_class_result(context, values) } else { Err(EvalStatus::RuntimeFatal) }
}
