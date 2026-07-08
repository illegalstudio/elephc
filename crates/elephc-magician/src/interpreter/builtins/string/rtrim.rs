//! Purpose:
//! Declarative eval registry entry for `rtrim`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the trim-family hook.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "rtrim",
    area: String,
    params: [string, characters = EvalBuiltinDefaultValue::Bytes(b" \n\r\t\x0b\x0c\0")],
    direct: TrimLike,
    values: TrimLike,
}

use super::super::super::*;

/// Evaluates PHP `rtrim(...)` over one eval expression and optional mask.
pub(in crate::interpreter) fn eval_builtin_rtrim(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::trim::eval_builtin_trim_like_named("rtrim", args, context, scope, values)
}

/// Applies PHP `rtrim(...)` to one evaluated string and optional mask.
pub(in crate::interpreter) fn eval_rtrim_result(
    value: RuntimeCellHandle,
    mask: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::trim::eval_trim_like_named_result("rtrim", value, mask, values)
}
