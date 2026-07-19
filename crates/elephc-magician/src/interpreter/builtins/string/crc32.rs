//! Purpose:
//! Declarative eval registry entry for `crc32`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the existing checksum hook.

eval_builtin! {
    name: "crc32",
    area: String,
    params: [string],
    direct: Crc32,
    values: Crc32,
}

use super::super::super::*;
use super::super::eval_crc32_bytes;

/// Evaluates PHP `crc32(...)` over one eval string expression.
pub(in crate::interpreter) fn eval_builtin_crc32(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_crc32_result(value, values)
}

/// Computes PHP's non-negative CRC-32 integer over one converted byte string.
pub(in crate::interpreter) fn eval_crc32_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    values.int(i64::from(eval_crc32_bytes(&bytes)))
}
