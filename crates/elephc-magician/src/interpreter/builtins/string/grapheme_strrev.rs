//! Purpose:
//! Declarative eval registry entry for `grapheme_strrev`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the grapheme string reverse hook.

eval_builtin! {
    name: "grapheme_strrev",
    area: String,
    params: [string],
    direct: GraphemeStrrev,
    values: GraphemeStrrev,
}

use super::super::super::*;
use unicode_segmentation::UnicodeSegmentation;

/// Evaluates PHP `grapheme_strrev(...)` over one eval expression.
pub(in crate::interpreter) fn eval_builtin_grapheme_strrev(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_grapheme_strrev_result(value, values)
}

/// Reverses a materialized PHP string by grapheme cluster or returns false for invalid UTF-8.
pub(in crate::interpreter) fn eval_grapheme_strrev_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let Ok(source) = std::str::from_utf8(&bytes) else {
        return values.bool_value(false);
    };
    let reversed = source.graphemes(true).rev().collect::<String>();
    values.string(&reversed)
}
