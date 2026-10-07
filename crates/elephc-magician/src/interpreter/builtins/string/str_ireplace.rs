//! Purpose:
//! Declarative eval registry entry for `str_ireplace`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the string-replace hook.

eval_builtin! {
    contract: "str_ireplace",
    area: String,
    direct: StrReplace,
    values: StrReplace,
}

use super::super::super::*;

/// Evaluates PHP `str_ireplace(...)` over search, replacement, and subject expressions.
pub(in crate::interpreter) fn eval_builtin_str_ireplace(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::str_replace::eval_builtin_str_replace("str_ireplace", args, context, scope, values)
}

/// Applies PHP `str_ireplace(...)` to already evaluated search, replacement, and subject values.
pub(in crate::interpreter) fn eval_str_ireplace_result(
    search: RuntimeCellHandle,
    replace: RuntimeCellHandle,
    subject: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::str_replace::eval_str_replace_result(
        "str_ireplace",
        search,
        replace,
        subject,
        context,
        values,
    )
}
