//! Purpose:
//! Declarative eval registry entry for `stripslashes`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the existing slash unescaping hook.

eval_builtin! {
    name: "stripslashes",
    area: String,
    params: [string],
    direct: Slashes,
    values: Slashes,
}

use super::super::super::*;

/// Evaluates PHP `stripslashes(...)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_stripslashes(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_stripslashes_result(value, values)
}

/// Removes backslash quoting using PHP `stripslashes()` byte semantics.
pub(in crate::interpreter) fn eval_stripslashes_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\' {
            index += 1;
            if let Some(byte) = bytes.get(index).copied() {
                output.push(if byte == b'0' { 0 } else { byte });
                index += 1;
            }
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    values.string_bytes_value(&output)
}
