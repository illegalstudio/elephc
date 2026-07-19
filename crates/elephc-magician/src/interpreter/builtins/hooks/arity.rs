//! Purpose:
//! Shared fixed-arity helpers for declarative builtin values hooks.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks::values`.
//!
//! Key details:
//! - Helpers validate already-bound argument slices before delegating to the
//!   concrete runtime-value operation.

use super::super::super::{EvalStatus, RuntimeCellHandle, RuntimeValueOps};

/// Validates and dispatches one evaluated builtin argument.
pub(super) fn one_arg<V, F>(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut V,
    callback: F,
) -> Result<RuntimeCellHandle, EvalStatus>
where
    V: RuntimeValueOps,
    F: FnOnce(RuntimeCellHandle, &mut V) -> Result<RuntimeCellHandle, EvalStatus>,
{
    let [value] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    callback(*value, values)
}

/// Validates and dispatches two evaluated builtin arguments.
pub(super) fn two_args<V, F>(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut V,
    callback: F,
) -> Result<RuntimeCellHandle, EvalStatus>
where
    V: RuntimeValueOps,
    F: FnOnce(RuntimeCellHandle, RuntimeCellHandle, &mut V) -> Result<RuntimeCellHandle, EvalStatus>,
{
    let [left, right] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    callback(*left, *right, values)
}

/// Validates and dispatches three evaluated builtin arguments.
pub(super) fn three_args<V, F>(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut V,
    callback: F,
) -> Result<RuntimeCellHandle, EvalStatus>
where
    V: RuntimeValueOps,
    F: FnOnce(
        RuntimeCellHandle,
        RuntimeCellHandle,
        RuntimeCellHandle,
        &mut V,
    ) -> Result<RuntimeCellHandle, EvalStatus>,
{
    let [first, second, third] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    callback(*first, *second, *third, values)
}
