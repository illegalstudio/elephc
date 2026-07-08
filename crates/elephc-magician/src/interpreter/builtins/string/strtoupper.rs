//! Purpose:
//! Declarative eval registry entry for `strtoupper`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the string-case hook.

eval_builtin! {
    name: "strtoupper",
    area: String,
    params: [string],
    direct: StringCase,
    values: StringCase,
}

use super::super::super::*;

/// Evaluates PHP `strtoupper(...)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_strtoupper(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::strtolower::eval_builtin_string_case_named("strtoupper", args, context, scope, values)
}

/// Applies PHP `strtoupper(...)` to one evaluated string value.
pub(in crate::interpreter) fn eval_strtoupper_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::strtolower::eval_string_case_named_result("strtoupper", value, values)
}
