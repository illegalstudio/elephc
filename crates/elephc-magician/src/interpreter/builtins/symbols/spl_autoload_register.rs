//! Purpose:
//! Eval registry entry and implementation for `spl_autoload_register`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Registration is a conservative successful stub, mirroring the main backend.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "spl_autoload_register",
    area: Symbols,
    params: [
        callback = EvalBuiltinDefaultValue::Null,
        throw = EvalBuiltinDefaultValue::Bool(true),
        prepend = EvalBuiltinDefaultValue::Bool(false),
    ],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

/// Evaluates direct `spl_autoload_register(...)` calls as a successful stub.
pub(in crate::interpreter) fn eval_spl_autoload_register_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_spl_autoload_bool("spl_autoload_register", args, context, scope, values)
}

/// Evaluates materialized `spl_autoload_register(...)` arguments as a successful stub.
pub(in crate::interpreter) fn eval_spl_autoload_register_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_spl_autoload_bool_result("spl_autoload_register", evaluated_args, values)
}

/// Evaluates boolean SPL autoload registration stubs.
pub(in crate::interpreter) fn eval_builtin_spl_autoload_bool(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "spl_autoload_register" if args.len() <= 3 => {}
        "spl_autoload_unregister" if args.len() == 1 => {}
        _ => return Err(EvalStatus::RuntimeFatal),
    }
    for arg in args {
        let _ = eval_expr(arg, context, scope, values)?;
    }
    values.bool_value(true)
}

/// Evaluates materialized boolean SPL autoload registration stubs.
pub(in crate::interpreter) fn eval_spl_autoload_bool_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "spl_autoload_register" if evaluated_args.len() <= 3 => values.bool_value(true),
        "spl_autoload_unregister" if evaluated_args.len() == 1 => values.bool_value(true),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
