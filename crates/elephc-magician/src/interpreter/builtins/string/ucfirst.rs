//! Purpose:
//! Declarative eval registry entry for `ucfirst`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the string-case hook.

eval_builtin! {
    name: "ucfirst",
    area: String,
    params: [string],
    direct: StringCase,
    values: StringCase,
}

use super::super::super::*;

/// Evaluates PHP `ucfirst(...)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_ucfirst(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::strtolower::eval_builtin_string_case_named("ucfirst", args, context, scope, values)
}

/// Applies PHP `ucfirst(...)` to one evaluated string value.
pub(in crate::interpreter) fn eval_ucfirst_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::strtolower::eval_string_case_named_result("ucfirst", value, values)
}
