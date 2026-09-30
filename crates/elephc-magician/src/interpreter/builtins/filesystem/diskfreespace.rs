//! Purpose:
//! Declarative eval registry entry for `diskfreespace`, PHP's alias of `disk_free_space`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - The alias needs its OWN inventory entry: the registry panics on a duplicate name,
//!   so it cannot be a second declaration of `disk_free_space`.
//! - Parameter names follow php-src (directory), not the canonical builtin's, because the
//!   parity gate compares them against the compiler's static signature.
//! - Both dispatch arms delegate straight to the canonical implementation.

eval_builtin! {
    contract: "diskfreespace",
    area: Filesystem,
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for `diskfreespace` through the canonical `disk_free_space` implementation.
pub(in crate::interpreter) fn eval_diskfreespace_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::disk_free_space::eval_disk_free_space_declared_call(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for `diskfreespace` through the canonical `disk_free_space` implementation.
pub(in crate::interpreter) fn eval_diskfreespace_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::disk_free_space::eval_disk_free_space_declared_values_result(evaluated_args, context, values)
}
