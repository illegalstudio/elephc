//! Purpose:
//! Declarative eval registry entry for `umask`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the process umask helper.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "umask",
    area: Filesystem,
    params: [mask = EvalBuiltinDefaultValue::Null],
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;

/// Dispatches direct eval calls for the `umask` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_umask_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_umask(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `umask` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_umask_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [] => eval_umask_result(None, values),
        [mask] => eval_umask_result(Some(*mask), values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `umask($mask = null)` over an optional eval expression.
pub(in crate::interpreter) fn eval_builtin_umask(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [] => eval_umask_result(None, values),
        [mask] => {
            let mask = eval_expr(mask, context, scope, values)?;
            eval_umask_result(Some(mask), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Applies PHP `umask()` semantics and returns the previous mask.
pub(in crate::interpreter) fn eval_umask_result(
    mask: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let previous = match mask {
        Some(mask) => {
            let mask = eval_int_value(mask, values)? as u32;
            unsafe { umask(mask) }
        }
        None => unsafe {
            let current = umask(0);
            umask(current);
            current
        },
    };
    values.int(i64::from(previous))
}
