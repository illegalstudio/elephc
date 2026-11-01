//! Purpose:
//! Declarative eval registry entry for `clearstatcache`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through eval's ordered no-op stat-cache helper.

eval_builtin! {
    contract: "clearstatcache",
    area: Filesystem,
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `clearstatcache` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_clearstatcache_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_clearstatcache(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `clearstatcache` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_clearstatcache_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if evaluated_args.len() > 2 {
        return Err(EvalStatus::RuntimeFatal);
    }
    // `clearstatcache(true)` also empties the realpath cache; any argument is taken as that
    // request, since forgetting too much only costs a lookup.
    if !evaluated_args.is_empty() {
        values.filesystem_paths_changed();
    }
    values.null()
}

/// Evaluates `clearstatcache(...)` as an ordered no-op in eval.
pub(in crate::interpreter) fn eval_builtin_clearstatcache(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() > 2 {
        return Err(EvalStatus::RuntimeFatal);
    }
    for arg in args {
        eval_expr(arg, context, scope, values)?;
    }
    if !args.is_empty() {
        values.filesystem_paths_changed();
    }
    values.null()
}
