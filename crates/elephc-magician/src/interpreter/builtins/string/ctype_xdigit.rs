//! Purpose:
//! Declarative eval registry entry for `ctype_xdigit`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the existing ASCII ctype hook.

eval_builtin! {
    contract: "ctype_xdigit",
    area: String,
    direct: Ctype,
    values: Ctype,
}

use super::super::super::*;

/// Evaluates PHP `ctype_xdigit(...)` over one eval string expression.
pub(in crate::interpreter) fn eval_builtin_ctype_xdigit(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::ctype_alnum::eval_builtin_ctype_named("ctype_xdigit", args, context, scope, values)
}

/// Returns the PHP boolean result for `ctype_xdigit(...)` from one evaluated value.
pub(in crate::interpreter) fn eval_ctype_xdigit_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::ctype_alnum::eval_ctype_named_result("ctype_xdigit", value, values)
}
