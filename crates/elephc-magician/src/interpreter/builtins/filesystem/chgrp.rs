//! Purpose:
//! Declarative eval registry entry for `chgrp`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the ownership/group helper.

eval_builtin! {
    name: "chgrp",
    area: Filesystem,
    params: [filename, group],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `chgrp` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_chgrp_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::direct_dispatch::eval_builtin_filesystem_call_impl("chgrp", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `chgrp` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_chgrp_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::values_dispatch::eval_filesystem_values_result_impl("chgrp", evaluated_args, context, values)
}
