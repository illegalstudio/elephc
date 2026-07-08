//! Purpose:
//! Declarative eval registry entry for `rawurlencode`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the existing URL encode hook.

eval_builtin! {
    name: "rawurlencode",
    area: String,
    params: [string],
    direct: UrlEncode,
    values: UrlEncode,
}

use super::super::super::*;

/// Evaluates PHP `rawurlencode(...)` over one eval string expression.
pub(in crate::interpreter) fn eval_builtin_rawurlencode(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::urlencode::eval_builtin_url_encode_named("rawurlencode", args, context, scope, values)
}

/// Applies PHP `rawurlencode(...)` to one evaluated string value.
pub(in crate::interpreter) fn eval_rawurlencode_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::urlencode::eval_url_encode_named_result("rawurlencode", value, values)
}
