//! Purpose:
//! Shared scalar formatting coercions used by formatting-related eval builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins::formatting` number-format and printf helpers.
//!
//! Key details:
//! - Float conversion delegates to runtime coercion first, then parses the runtime
//!   string payload into the scalar value used by formatting logic.

use super::super::super::*;

/// Converts one eval value to PHP float and returns the scalar payload.
pub(in crate::interpreter) fn eval_float_value(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<f64, EvalStatus> {
    let value = values.cast_float(value)?;
    let bytes = values.string_bytes(value)?;
    std::str::from_utf8(&bytes)
        .map_err(|_| EvalStatus::RuntimeFatal)?
        .parse::<f64>()
        .map_err(|_| EvalStatus::RuntimeFatal)
}
