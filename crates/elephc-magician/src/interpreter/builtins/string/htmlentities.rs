//! Purpose:
//! Declarative eval registry entry for `htmlentities`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the HTML entity hook, which
//!   honours `$flags` the way `htmlspecialchars()` does.

eval_builtin! {
    contract: "htmlentities",
    area: String,
    direct: HtmlEntity,
    values: HtmlEntity,
}

use super::super::super::*;
/// Evaluates PHP `htmlentities(...)` over one eval string expression.
pub(in crate::interpreter) fn eval_builtin_htmlentities(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::htmlspecialchars::eval_builtin_html_entity_named("htmlentities", args, context, scope, values)
}

/// Applies PHP `htmlentities(...)` to one evaluated string value under `$flags`.
///
/// Shares the `htmlspecialchars()` encoder, as the compiled lowering does.
pub(in crate::interpreter) fn eval_htmlentities_result(
    value: RuntimeCellHandle,
    flags: i64,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::htmlspecialchars::eval_htmlspecialchars_result(value, flags, values)
}
