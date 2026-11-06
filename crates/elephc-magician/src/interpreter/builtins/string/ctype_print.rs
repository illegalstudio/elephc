//! Purpose:
//! Declarative eval registry entry for `ctype_print`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the existing ASCII ctype hook.

eval_builtin! {
    contract: "ctype_print",
    area: String,
    direct: Ctype,
    values: Ctype,
}

use super::super::super::*;

/// Evaluates PHP `ctype_print(...)` over one eval string expression.
pub(in crate::interpreter) fn eval_builtin_ctype_print(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::ctype_alnum::eval_builtin_ctype_named("ctype_print", args, context, scope, values)
}

/// Returns the PHP boolean result for `ctype_print(...)` from one evaluated value.
pub(in crate::interpreter) fn eval_ctype_print_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::ctype_alnum::eval_ctype_named_result("ctype_print", value, values)
}
