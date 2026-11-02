//! Purpose:
//! Eval registry entry and implementation for `trait_exists`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - The shared trait/enum existence probe lives here and `enum_exists()`
//!   calls it explicitly.
//! - A miss runs the registered SPL autoloaders unless `$autoload` is false, as PHP does.

eval_builtin! {
    contract: "trait_exists",
    area: Symbols,
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;
use super::eval_spl_autoload_class;

/// Evaluates direct `trait_exists(...)` calls.
pub(in crate::interpreter) fn eval_trait_exists_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_class_like_exists("trait_exists", args, context, scope, values)
}

/// Evaluates materialized `trait_exists(...)` arguments.
pub(in crate::interpreter) fn eval_trait_exists_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_class_like_exists_result("trait_exists", evaluated_args, context, values)
}

/// Evaluates `trait_exists(...)` and `enum_exists(...)` against generated metadata.
pub(in crate::interpreter) fn eval_builtin_class_like_exists(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (symbol, autoload) = match args {
        [symbol] => (eval_expr(symbol, context, scope, values)?, true),
        [symbol, autoload] => {
            let symbol = eval_expr(symbol, context, scope, values)?;
            let autoload = eval_expr(autoload, context, scope, values)?;
            (symbol, values.truthy(autoload)?)
        }
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let exists = eval_class_like_exists_name(name, symbol, autoload, context, values)?;
    values.bool_value(exists)
}

/// Evaluates materialized `trait_exists(...)` or `enum_exists(...)` arguments.
pub(in crate::interpreter) fn eval_class_like_exists_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let exists = match evaluated_args {
        [symbol] => eval_class_like_exists_name(name, *symbol, true, context, values)?,
        [symbol, autoload] => {
            let autoload = values.truthy(*autoload)?;
            eval_class_like_exists_name(name, *symbol, autoload, context, values)?
        }
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    values.bool_value(exists)
}

/// Normalizes a PHP class-like name cell and probes generated trait or enum metadata.
///
/// PHP's second parameter defaults to true and means "ask the registered autoloaders when the name
/// is not already defined". One callback registry serves classes, interfaces, traits and enums, so
/// the chain `class_exists()` runs is the chain these run; only the question asked afterwards
/// differs. A loader that throws propagates from here.
pub(in crate::interpreter) fn eval_class_like_exists_name(
    name: &str,
    symbol: RuntimeCellHandle,
    autoload: bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let symbol = values.string_bytes(symbol)?;
    let symbol = String::from_utf8(symbol).map_err(|_| EvalStatus::RuntimeFatal)?;
    let symbol = symbol.trim_start_matches('\\');
    if eval_class_like_is_declared(name, symbol, context, values)? {
        return Ok(true);
    }
    if !autoload {
        return Ok(false);
    }
    let _ = eval_spl_autoload_class(symbol, context, values)?;
    eval_class_like_is_declared(name, symbol, context, values)
}

/// Returns whether a trait or enum name is already declared, without consulting any autoloader.
fn eval_class_like_is_declared(
    name: &str,
    symbol: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    match name {
        "trait_exists" => Ok(context.has_trait(symbol) || values.trait_exists(symbol)?),
        "enum_exists" => Ok(context.has_enum(symbol) || values.enum_exists(symbol)?),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}
