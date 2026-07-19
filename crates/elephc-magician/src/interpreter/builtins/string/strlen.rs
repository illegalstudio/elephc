//! Purpose:
//! Declarative eval registry entry for `strlen`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the existing string-length hook.

eval_builtin! {
    name: "strlen",
    area: String,
    params: [string],
    direct: Strlen,
    values: Strlen,
}

use super::super::super::*;

/// Evaluates the builtin `strlen(...)` for one PHP-coerced string argument.
pub(in crate::interpreter) fn eval_builtin_strlen(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_strlen_result(value, values)
}

/// Returns the byte length of one materialized eval string.
pub(in crate::interpreter) fn eval_strlen_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let len = i64::try_from(bytes.len()).map_err(|_| EvalStatus::RuntimeFatal)?;
    values.int(len)
}
