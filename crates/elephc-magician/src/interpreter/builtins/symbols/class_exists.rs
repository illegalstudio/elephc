//! Purpose:
//! Eval registry entry and implementation for `class_exists`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Lookup checks eval declarations before generated/AOT runtime metadata.
//! - `Closure` is treated as a built-in class-like symbol.
//! - A miss runs the registered SPL autoloaders unless `$autoload` is false, as PHP does.

eval_builtin! {
    contract: "class_exists",
    area: Symbols,
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;
use super::eval_spl_autoload_class;

/// Evaluates direct `class_exists(...)` calls against dynamic and generated class-name tables.
pub(in crate::interpreter) fn eval_class_exists_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_class_exists(args, context, scope, values)
}

/// Evaluates materialized `class_exists(...)` arguments.
pub(in crate::interpreter) fn eval_class_exists_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_class_exists_result(evaluated_args, context, values)
}

/// Evaluates `class_exists(...)` against dynamic and generated class-name tables.
pub(in crate::interpreter) fn eval_builtin_class_exists(
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
    let exists = eval_class_exists_name(name, autoload, context, values)?;
    values.bool_value(exists)
}

/// Evaluates `class_exists(...)` from already materialized call arguments.
pub(in crate::interpreter) fn eval_class_exists_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let exists = match evaluated_args {
        [name] => eval_class_exists_name(*name, true, context, values)?,
        [name, autoload] => {
            let autoload = values.truthy(*autoload)?;
            eval_class_exists_name(*name, autoload, context, values)?
        }
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    values.bool_value(exists)
}

/// Normalizes a PHP class-name cell and probes dynamic names before generated classes.
///
/// PHP's second parameter defaults to true and means "ask the registered autoloaders when the
/// name is not already defined". The autoloaders run inside `class_exists()`, so a loader that
/// throws makes `class_exists()` THROW — it does not report `false`. Symfony's
/// `ClassExistenceResource` depends on exactly that: it registers a temporary loader whose only
/// job is to raise `ReflectionException` for a class it cannot provide, and it catches that
/// exception at the `class_exists()` call site.
pub(in crate::interpreter) fn eval_class_exists_name(
    name: RuntimeCellHandle,
    autoload: bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let name = values.string_bytes(name)?;
    let name = String::from_utf8(name).map_err(|_| EvalStatus::RuntimeFatal)?;
    let name = name.trim_start_matches('\\');
    if eval_class_is_declared(name, context, values)? {
        return Ok(true);
    }
    if !autoload {
        return Ok(false);
    }
    // The chain reports whether ANY class-like symbol became visible — PHP shares one registry
    // across classes, interfaces, traits and enums — so the class question is asked again rather
    // than answered from that boolean. A Throwable raised by a loader propagates from here.
    let _ = eval_spl_autoload_class(name, context, values)?;
    eval_class_is_declared(name, context, values)
}

/// Returns whether a class name is already declared, without consulting any autoloader.
pub(in crate::interpreter) fn eval_class_is_declared(
    name: &str,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if name.eq_ignore_ascii_case("Closure") {
        return Ok(true);
    }
    if context.has_class(name) {
        return Ok(true);
    }
    values.class_exists(name)
}
