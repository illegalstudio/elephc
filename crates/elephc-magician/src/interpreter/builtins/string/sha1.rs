//! Purpose:
//! Declarative eval registry entry for `sha1`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the one-shot hash hook.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "sha1",
    area: String,
    params: [string, binary = EvalBuiltinDefaultValue::Bool(false)],
    direct: HashOneShot,
    values: HashOneShot,
}

use super::super::super::*;

/// Evaluates PHP `sha1(...)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_sha1(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::hash::eval_builtin_hash_one_shot_named("sha1", args, context, scope, values)
}

/// Applies PHP `sha1(...)` to already evaluated arguments.
pub(in crate::interpreter) fn eval_sha1_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::hash::eval_hash_one_shot_named_result("sha1", evaluated_args, values)
}
