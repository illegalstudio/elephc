//! Purpose:
//! Declarative eval registry entry for `ctype_digit`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the existing ASCII ctype hook.

eval_builtin! {
    name: "ctype_digit",
    area: String,
    params: [text],
    direct: Ctype,
    values: Ctype,
}

use super::super::super::*;

/// Evaluates PHP `ctype_digit(...)` over one eval string expression.
pub(in crate::interpreter) fn eval_builtin_ctype_digit(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::ctype_alnum::eval_builtin_ctype_named("ctype_digit", args, context, scope, values)
}

/// Returns the PHP boolean result for `ctype_digit(...)` from one evaluated value.
pub(in crate::interpreter) fn eval_ctype_digit_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::ctype_alnum::eval_ctype_named_result("ctype_digit", value, values)
}
