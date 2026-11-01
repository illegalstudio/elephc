//! Purpose:
//! Eval registry entry and implementation for `interface_exists`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Lookup checks eval interface declarations before generated/AOT runtime metadata.
//! - A miss runs the registered SPL autoloaders unless `$autoload` is false, as PHP does.

eval_builtin! {
    contract: "interface_exists",
    area: Symbols,
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;
use super::eval_spl_autoload_class;

/// Evaluates direct `interface_exists(...)` calls against eval and generated metadata.
pub(in crate::interpreter) fn eval_interface_exists_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_interface_exists(args, context, scope, values)
}

/// Evaluates materialized `interface_exists(...)` arguments.
pub(in crate::interpreter) fn eval_interface_exists_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_interface_exists_result(evaluated_args, context, values)
}

/// Evaluates `interface_exists(...)` against generated interface-name metadata.
pub(in crate::interpreter) fn eval_builtin_interface_exists(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (name, autoload) = match args {
        [name] => (eval_expr(name, context, scope, values)?, true),
        [name, autoload] => {
            let name = eval_expr(name, context, scope, values)?;
            let autoload = eval_expr(autoload, context, scope, values)?;
            (name, values.truthy(autoload)?)
        }
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let exists = eval_interface_exists_name(name, autoload, context, values)?;
    values.bool_value(exists)
}

/// Evaluates `interface_exists(...)` from already materialized call arguments.
pub(in crate::interpreter) fn eval_interface_exists_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let exists = match evaluated_args {
        [name] => eval_interface_exists_name(*name, true, context, values)?,
        [name, autoload] => {
            let autoload = values.truthy(*autoload)?;
            eval_interface_exists_name(*name, autoload, context, values)?
        }
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    values.bool_value(exists)
}

/// Normalizes a PHP interface-name cell and probes eval and generated interface metadata.
///
/// PHP's second parameter defaults to true and means "ask the registered autoloaders when the name
/// is not already defined" — the same contract `class_exists()` has, served by the SAME callback
/// registry, because PHP keeps one registry for classes, interfaces, traits and enums. Symfony's DI
/// passes call `interface_exists()` constantly to decide whether an optional component is present,
/// and answering false without asking the loader makes every one of those answers wrong. A loader
/// that throws propagates from here, as it does from `class_exists()`.
pub(in crate::interpreter) fn eval_interface_exists_name(
    name: RuntimeCellHandle,
    autoload: bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let name = values.string_bytes(name)?;
    let name = String::from_utf8(name).map_err(|_| EvalStatus::RuntimeFatal)?;
    let name = name.trim_start_matches('\\');
    if eval_interface_is_declared(name, context, values)? {
        return Ok(true);
    }
    if !autoload {
        return Ok(false);
    }
    // The chain reports whether ANY class-like symbol became visible, so the interface question is
    // asked again rather than answered from that boolean.
    let _ = eval_spl_autoload_class(name, context, values)?;
    eval_interface_is_declared(name, context, values)
}

/// Returns whether an interface name is already declared, without consulting any autoloader.
fn eval_interface_is_declared(
    name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    Ok(context.has_interface(name) || eval_runtime_interface_exists(name, values)?)
}
