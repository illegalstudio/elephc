//! Purpose:
//! Implements eval SPL autoload helper builtins.
//!
//! Called from:
//! - `crate::interpreter::expressions::eval_positional_expr_call()`.
//! - Dynamic callable dispatch under `builtins::registry::dispatch`.
//!
//! Key details:
//! - The main EIR backend models autoload registration as conservative stubs.
//! - Eval mirrors that behavior while keeping `spl_autoload_extensions()` as
//!   eval-local mutable state on the context.

use super::super::*;

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

/// Evaluates void SPL autoload call stubs.
pub(in crate::interpreter) fn eval_builtin_spl_autoload_void(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "spl_autoload_call" if args.len() == 1 => {}
        "spl_autoload" if (1..=2).contains(&args.len()) => {}
        _ => return Err(EvalStatus::RuntimeFatal),
    }
    for arg in args {
        let _ = eval_expr(arg, context, scope, values)?;
    }
    eval_spl_autoload_void_result(name, args, values)
}

/// Evaluates materialized void SPL autoload call stubs.
pub(in crate::interpreter) fn eval_spl_autoload_void_result<T>(
    name: &str,
    evaluated_args: &[T],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "spl_autoload_call" if evaluated_args.len() == 1 => values.null(),
        "spl_autoload" if (1..=2).contains(&evaluated_args.len()) => values.null(),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates `spl_autoload_functions()`.
pub(in crate::interpreter) fn eval_builtin_spl_autoload_functions(
    args: &[EvalExpr],
    _context: &mut ElephcEvalContext,
    _scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_spl_autoload_functions_result(args, values)
}

/// Evaluates materialized `spl_autoload_functions()`.
pub(in crate::interpreter) fn eval_spl_autoload_functions_result<T>(
    evaluated_args: &[T],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !evaluated_args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    values.array_new(0)
}

/// Evaluates `spl_autoload_extensions()`.
pub(in crate::interpreter) fn eval_builtin_spl_autoload_extensions(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let evaluated_args = match args {
        [] => Vec::new(),
        [extensions] => vec![eval_expr(extensions, context, scope, values)?],
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    eval_spl_autoload_extensions_result(&evaluated_args, context, values)
}

/// Evaluates materialized `spl_autoload_extensions()` arguments.
pub(in crate::interpreter) fn eval_spl_autoload_extensions_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [] => {}
        [extensions] if values.type_tag(*extensions)? == EVAL_TAG_NULL => {}
        [extensions] => {
            let extensions = values.string_bytes(*extensions)?;
            let extensions = String::from_utf8(extensions).map_err(|_| EvalStatus::RuntimeFatal)?;
            context.set_spl_autoload_extensions(extensions);
        }
        _ => return Err(EvalStatus::RuntimeFatal),
    }
    values.string(context.spl_autoload_extensions())
}
