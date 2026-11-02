//! Purpose:
//! Declarative eval registry entry for `highlight_string`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Direct and values dispatch share one implementation in `super::highlight`.

eval_builtin! {
    contract: "highlight_string",
    area: String,
    direct: Highlight,
    values: Highlight,
}

use super::super::super::*;

/// Evaluates PHP's `highlight_string(...)` over source expressions.
pub(in crate::interpreter) fn eval_builtin_highlight_string(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::highlight::eval_highlight_named("highlight_string", args, context, scope, values)
}
