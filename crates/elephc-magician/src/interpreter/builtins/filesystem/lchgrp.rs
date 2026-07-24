//! Purpose:
//! Declarative eval registry entry for `lchgrp`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the ownership/group helper.

#[cfg(not(windows))]
eval_builtin! {
    name: "lchgrp",
    area: Filesystem,
    params: [filename, group],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `lchgrp` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_lchgrp_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::chown::eval_builtin_chown_like("lchgrp", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `lchgrp` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_lchgrp_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [filename, principal] => super::chown::eval_chown_like_result("lchgrp", *filename, *principal, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
