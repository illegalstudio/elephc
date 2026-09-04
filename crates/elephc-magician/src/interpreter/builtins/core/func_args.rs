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

/// Returns the canonical `func_*` name selected by a literal callback expression.
pub(super) fn eval_literal_func_args_callback(callback: &EvalExpr) -> Option<&'static str> {
    let EvalExpr::Const(EvalConst::String(name)) = callback else {
        return None;
    };
    if name.eq_ignore_ascii_case("func_get_arg") {
        Some("func_get_arg")
    } else if name.eq_ignore_ascii_case("func_get_args") {
        Some("func_get_args")
    } else if name.eq_ignore_ascii_case("func_num_args") {
        Some("func_num_args")
    } else {
        None
    }
}

/// Throws the PHP global-scope error shared by `func_get_arg()` and `func_get_args()`.
pub(super) fn eval_throw_func_get_global_scope<T>(
    name: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    eval_throw_error(
        &format!("{name}() must be called from a function context"),
        context,
        values,
    )
}
