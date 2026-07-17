//! Purpose:
//! Declarative eval registry entry for `md5`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the one-shot hash hook.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "md5",
    area: String,
    params: [string, binary = EvalBuiltinDefaultValue::Bool(false)],
    direct: HashOneShot,
    values: HashOneShot,
}

use super::super::super::*;

/// Evaluates PHP `md5(...)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_md5(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::hash::eval_builtin_hash_one_shot_named("md5", args, context, scope, values)
}

/// Applies PHP `md5(...)` to already evaluated arguments.
pub(in crate::interpreter) fn eval_md5_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::hash::eval_hash_one_shot_named_result("md5", evaluated_args, values)
}
