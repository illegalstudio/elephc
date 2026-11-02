//! Purpose:
//! Eval registry entries and implementations for php 8.5's `array_first` and `array_last`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array` dispatch tables.
//!
//! Key details:
//! - `null` for an empty array, else the value at the first/last iteration position; the
//!   array's internal pointer is not touched.
//! - The compiled program gets these from `elephc::array_edge_prelude`, which interpreted code
//!   cannot reach: a Symfony container generated on php 8.5 calls `\array_first(...)` while it
//!   is interpreted, and `symfony/polyfill-php85` declares nothing on an 8.5 profile.

use super::super::super::*;

eval_builtin! {
    contract: "array_first",
    area: Array,
    direct: Array,
    values: Array,
}

eval_builtin! {
    contract: "array_last",
    area: Array,
    direct: Array,
    values: Array,
}

/// Which end of the array a call reads.
#[derive(Clone, Copy)]
pub(in crate::interpreter) enum ArrayEdge {
    /// `array_first()`.
    First,
    /// `array_last()`.
    Last,
}

/// Dispatches direct eval calls for `array_first()` / `array_last()`.
pub(in crate::interpreter) fn eval_array_edge_declared_call(
    edge: ArrayEdge,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = eval_expr(array, context, scope, values)?;
    eval_array_edge_result(edge, array, values)
}

/// Dispatches evaluated-argument eval calls for `array_first()` / `array_last()`.
pub(in crate::interpreter) fn eval_array_edge_declared_values_result(
    edge: ArrayEdge,
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_array_edge_result(edge, *array, values)
}

/// Answers `array_first()` / `array_last()`.
fn eval_array_edge_result(
    edge: ArrayEdge,
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    if len == 0 {
        return values.null();
    }
    let position = match edge {
        ArrayEdge::First => 0,
        ArrayEdge::Last => len - 1,
    };
    let key = values.array_iter_key(array, position)?;
    values.array_get(array, key)
}
