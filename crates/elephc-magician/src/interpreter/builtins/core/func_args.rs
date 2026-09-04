//! Purpose:
//! Shares eval activation-frame reads for PHP's three `func_*` introspection builtins.
//!
//! Called from:
//! - `func_get_arg`, `func_get_args`, and `func_num_args` eval builtin homes.
//!
//! Key details:
//! - Fixed parameters are read from the current scope so reassignment and references remain visible.
//! - Positional surplus arguments come from the immutable activation snapshot.

use super::super::super::*;
use crate::context::EvalFunctionArgsFrame;

/// Returns one current PHP argument value by its zero-based call position.
pub(in crate::interpreter) fn eval_current_function_arg(
    position: usize,
    frame: &EvalFunctionArgsFrame,
    scope: &ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(param) = frame.regular_param(position) {
        return match scope.visible_cell(param) {
            Some(value) => values.retain(value),
            None => values.null(),
        };
    }
    let surplus_position = position
        .checked_sub(frame.regular_param_count())
        .ok_or(EvalStatus::RuntimeFatal)?;
    let value = frame
        .surplus_arg(surplus_position)
        .ok_or(EvalStatus::RuntimeFatal)?;
    values.retain(value)
}

/// Throws the PHP global-scope error for a `func_get_arg*` builtin.
pub(super) fn eval_throw_func_get_global_scope<T>(
    name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_error(
        &format!("{name}() cannot be called from the global scope"),
        context,
        values,
    )
}
