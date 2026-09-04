//! Purpose:
//! Declarative eval registry entry for `mkdir`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the unary path operation helper.

eval_builtin! {
    contract: "mkdir",
    area: Filesystem,
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `mkdir` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_mkdir_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.is_empty() || args.len() > 4 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut evaluated = Vec::with_capacity(args.len());
    for arg in args {
        evaluated.push(eval_expr(arg, context, scope, values)?);
    }
    eval_mkdir_declared_values_result(&evaluated, context, values)
}

/// Dispatches evaluated-argument calls for the `mkdir` filesystem builtin through the area dispatcher.
///
/// `$permissions` and `$recursive` are honoured; `$context` is accepted and ignored, matching the
/// compiled backend — the checker accepts the documented signature, so the interpreter must too.
pub(in crate::interpreter) fn eval_mkdir_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [path, rest @ ..] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    if rest.len() > 3 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut options = super::chdir::MkdirOptions::DEFAULT;
    if let Some(permissions) = rest.first() {
        options.permissions = eval_int_value(*permissions, values)? as u32;
    }
    if let Some(recursive) = rest.get(1) {
        options.recursive = values.truthy(*recursive)?;
    }
    super::chdir::eval_path_bool_result_with_mkdir_options(
        "mkdir", *path, options, context, values,
    )
}
