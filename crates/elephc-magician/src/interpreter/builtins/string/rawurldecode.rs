//! Purpose:
//! Declarative eval registry entry for `rawurldecode`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the existing URL decode hook.

eval_builtin! {
    name: "rawurldecode",
    area: String,
    params: [string],
    direct: UrlDecode,
    values: UrlDecode,
}

use super::super::super::*;

/// Evaluates PHP `rawurldecode(...)` over one eval string expression.
pub(in crate::interpreter) fn eval_builtin_rawurldecode(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::urldecode::eval_builtin_url_decode_named("rawurldecode", args, context, scope, values)
}

/// Applies PHP `rawurldecode(...)` to one evaluated string value.
pub(in crate::interpreter) fn eval_rawurldecode_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::urldecode::eval_url_decode_named_result("rawurldecode", value, values)
}
