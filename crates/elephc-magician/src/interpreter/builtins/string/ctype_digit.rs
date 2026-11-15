//! Purpose:
//! Declarative eval registry entry for `ctype_digit`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the existing ASCII ctype hook.

eval_builtin! {
    contract: "ctype_digit",
    area: String,
    direct: Ctype,
    values: Ctype,
}

use super::super::super::*;

/// Evaluates PHP `ctype_digit(...)` over one eval argument expression.
pub(in crate::interpreter) fn eval_builtin_ctype_digit(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::ctype_alnum::eval_builtin_ctype_named("ctype_digit", args, context, scope, values)
}

/// Returns the PHP boolean result for `ctype_digit(...)` from one evaluated value of any type.
pub(in crate::interpreter) fn eval_ctype_digit_result(
    value: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::ctype_alnum::eval_ctype_named_result("ctype_digit", value, context, values)
}
