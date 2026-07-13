//! Purpose:
//! Declarative eval registry entry for `strcasecmp`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the string-compare hook.

eval_builtin! {
    name: "strcasecmp",
    area: String,
    params: [string1, string2],
    direct: StringCompare,
    values: StringCompare,
}

use super::super::super::*;

/// Evaluates PHP `strcasecmp(...)` over two eval expressions.
pub(in crate::interpreter) fn eval_builtin_strcasecmp(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::strcmp::eval_builtin_string_compare_named("strcasecmp", args, context, scope, values)
}

/// Applies PHP `strcasecmp(...)` to two evaluated string values.
pub(in crate::interpreter) fn eval_strcasecmp_result(
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::strcmp::eval_string_compare_named_result("strcasecmp", left, right, values)
}
