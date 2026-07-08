//! Purpose:
//! Declarative eval registry entry for `html_entity_decode`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the HTML entity hook.

eval_builtin! {
    name: "html_entity_decode",
    area: String,
    params: [string],
    direct: HtmlEntity,
    values: HtmlEntity,
}

use super::super::super::*;

/// Evaluates PHP `html_entity_decode(...)` over one eval string expression.
pub(in crate::interpreter) fn eval_builtin_html_entity_decode(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::htmlspecialchars::eval_builtin_html_entity_named("html_entity_decode", args, context, scope, values)
}

/// Applies PHP `html_entity_decode(...)` to one evaluated string value.
pub(in crate::interpreter) fn eval_html_entity_decode_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::htmlspecialchars::eval_html_entity_decode_value_result(value, values)
}
