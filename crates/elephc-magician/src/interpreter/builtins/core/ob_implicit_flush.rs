//! Purpose:
//! Eval registry entry and implementation for `ob_implicit_flush`.
//!
//! Called from:
//! - `crate::interpreter::builtins::core` direct and by-value dispatch.
//!
//! Key details:
//! - The stored flag is semantically inert (elephc terminal writes are unbuffered
//! -   syscalls); returns true like PHP 8.

use super::super::super::*;

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "ob_implicit_flush",
    area: Core,
    params: [enable = EvalBuiltinDefaultValue::Bool(true)],
    direct: Core,
    values: Core,
}

/// Evaluates PHP `ob_implicit_flush($enable = true)`.
pub(in crate::interpreter) fn eval_builtin_ob_implicit_flush(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [] => eval_ob_implicit_flush_result(&[], context, values),
        [enable] => {
            let enable = eval_expr(enable, context, scope, values)?;
            eval_ob_implicit_flush_result(&[enable], context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Stores the (inert) implicit-flush flag and returns true like PHP 8.
pub(in crate::interpreter) fn eval_ob_implicit_flush_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let enable = match evaluated_args {
        [] => true,
        [enable] => values.truthy(*enable)?,
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    values.ob_implicit_flush(enable)?;
    values.bool_value(true)
}
