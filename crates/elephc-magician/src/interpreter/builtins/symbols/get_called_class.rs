//! Purpose:
//! Eval registry entry and implementation for `get_called_class`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - The result uses the late-static-bound class scope when present, matching PHP.

eval_builtin! {
    name: "get_called_class",
    area: Symbols,
    params: [],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Evaluates direct `get_called_class()` calls.
pub(in crate::interpreter) fn eval_get_called_class_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    _scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_get_called_class(args, context, values)
}

/// Evaluates materialized `get_called_class()` arguments.
pub(in crate::interpreter) fn eval_get_called_class_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if evaluated_args.is_empty() {
        eval_get_called_class_result(context, values)
    } else {
        Err(EvalStatus::RuntimeFatal)
    }
}

/// Evaluates PHP's `get_called_class()` against the current eval method scope.
pub(in crate::interpreter) fn eval_builtin_get_called_class(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_get_called_class_result(context, values)
}

/// Returns the current late-static-bound class name or throws PHP's class-scope error.
pub(in crate::interpreter) fn eval_get_called_class_result(
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(class_name) = context
        .current_called_class_scope()
        .or_else(|| context.current_class_scope())
    else {
        return eval_throw_error(
            "get_called_class() must be called from within a class",
            context,
            values,
        );
    };
    values.string(class_name.trim_start_matches('\\'))
}
