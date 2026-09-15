//! Purpose:
//! Implements PHP `func_num_args()` for eval-declared callable activations.
//!
//! Called from:
//! - `crate::interpreter::builtins::core` direct and by-value dispatch.
//!
//! Key details:
//! - The count excludes unknown named values captured by a variadic parameter, matching PHP.

use super::super::super::*;

eval_builtin! {
    contract: "func_num_args",
    area: Core,
    direct: Core,
    values: Core,
}

/// Evaluates a direct `func_num_args()` call against the current activation.
pub(in crate::interpreter) fn eval_builtin_func_num_args(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_func_num_args_result(context, values)
}

/// Evaluates `func_num_args()` after callable dispatch materializes its empty argument list.
pub(in crate::interpreter) fn eval_func_num_args_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    eval_func_num_args_result(context, values)
}

/// Returns the active frame's PHP-visible argument count.
fn eval_func_num_args_result(
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(frame) = context.current_function_args() else {
        return eval_throw_error(
            "func_num_args() must be called from a function context",
            context,
            values,
        );
    };
    values.int(i64::try_from(frame.actual_count()).map_err(|_| EvalStatus::RuntimeFatal)?)
}
