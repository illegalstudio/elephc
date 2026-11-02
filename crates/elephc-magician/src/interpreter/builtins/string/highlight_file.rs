//! Purpose:
//! Declarative eval registry entry for `highlight_file`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Direct and values dispatch share one implementation in `super::highlight`.

eval_builtin! {
    contract: "highlight_file",
    area: String,
    direct: Highlight,
    values: Highlight,
}

use super::super::super::*;

/// Evaluates PHP's `highlight_file(...)` over source expressions.
pub(in crate::interpreter) fn eval_builtin_highlight_file(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::highlight::eval_highlight_named("highlight_file", args, context, scope, values)
}
