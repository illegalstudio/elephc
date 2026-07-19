//! Purpose:
//! Eval registry entry and raw pointer conversion helpers for `ptr`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks` and sibling raw-memory builtins.
//!
//! Key details:
//! - Eval keeps `ptr(...)` unsupported because by-value cells do not expose raw
//!   lvalue storage addresses safely.

use super::super::super::*;


eval_builtin! {
    name: "ptr",
    area: RawMemory,
    params: [value],
    direct: Ptr,
    values: Ptr,
}

/// Evaluates PHP `ptr()` and rejects unsupported eval lvalue-address extraction.
pub(in crate::interpreter) fn eval_builtin_ptr(
    args: &[EvalExpr],
    _context: &mut ElephcEvalContext,
    _scope: &mut ElephcEvalScope,
    _values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [_value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    Err(EvalStatus::UnsupportedConstruct)
}

/// Dispatches by-value `ptr()` calls after argument binding.
pub(in crate::interpreter) fn eval_ptr_values_result(
    evaluated_args: &[RuntimeCellHandle],
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [_value] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    Err(EvalStatus::UnsupportedConstruct)
}

/// Converts a runtime cell to a raw pointer address encoded as a PHP integer.
pub(super) fn eval_pointer_address(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<usize, EvalStatus> {
    let address = eval_int_value(value, values)?;
    usize::try_from(address).map_err(|_| EvalStatus::RuntimeFatal)
}

/// Converts a runtime cell to a non-null raw pointer.
pub(super) fn eval_non_null_pointer(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<*mut u8, EvalStatus> {
    let address = eval_pointer_address(value, values)?;
    if address == 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(address as *mut u8)
}

/// Boxes a raw pointer address as a PHP integer cell.
pub(super) fn eval_address_value(
    address: usize,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.int(i64::try_from(address).map_err(|_| EvalStatus::RuntimeFatal)?)
}
