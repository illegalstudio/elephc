//! Purpose:
//! Declarative eval registry entry for `ltrim`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the trim-family hook.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "ltrim",
    area: String,
    params: [string, characters = EvalBuiltinDefaultValue::Bytes(b" \n\r\t\x0b\x0c\0")],
    direct: TrimLike,
    values: TrimLike,
}

use super::super::super::*;

/// Evaluates PHP `ltrim(...)` over one eval expression and optional mask.
pub(in crate::interpreter) fn eval_builtin_ltrim(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::trim::eval_builtin_trim_like_named("ltrim", args, context, scope, values)
}

/// Applies PHP `ltrim(...)` to one evaluated string and optional mask.
pub(in crate::interpreter) fn eval_ltrim_result(
    value: RuntimeCellHandle,
    mask: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::trim::eval_trim_like_named_result("ltrim", value, mask, values)
}
