//! Purpose:
//! Declarative eval registry entry for `ord`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the existing byte introspection hook.

eval_builtin! {
    name: "ord",
    area: String,
    params: [character],
    direct: Ord,
    values: Ord,
}

use super::super::super::*;

/// Evaluates the builtin `ord(...)` for the first byte of one coerced string.
pub(in crate::interpreter) fn eval_builtin_ord(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_ord_result(value, values)
}

/// Returns the first byte of one converted string, or zero for an empty string.
pub(in crate::interpreter) fn eval_ord_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    values.int(i64::from(bytes.first().copied().unwrap_or(0)))
}
