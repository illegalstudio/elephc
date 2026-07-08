//! Purpose:
//! Declarative eval registry entry for `addslashes`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the existing slash escaping hook.

eval_builtin! {
    name: "addslashes",
    area: String,
    params: [string],
    direct: Slashes,
    values: Slashes,
}

use super::super::super::*;

/// Evaluates PHP `addslashes(...)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_addslashes(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_addslashes_result(value, values)
}

/// Escapes NUL, quotes, and backslashes using PHP `addslashes()` byte semantics.
pub(in crate::interpreter) fn eval_addslashes_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let mut output = Vec::with_capacity(bytes.len());
    for byte in bytes {
        match byte {
            0 => output.extend_from_slice(b"\\0"),
            b'\'' | b'"' | b'\\' => {
                output.push(b'\\');
                output.push(byte);
            }
            _ => output.push(byte),
        }
    }
    values.string_bytes_value(&output)
}
