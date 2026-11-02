//! Purpose:
//! Eval registry entry and implementation for `defined`.
//!
//! Called from:
//! - `crate::interpreter::builtins::core`.
//!
//! Key details:
//! - Dynamic names use `define`'s constant-name normalizer so the two builtins
//!   stay in lockstep.

use super::define::eval_constant_name;
use super::super::super::*;

eval_builtin! {
    contract: "defined",
    area: Core,
    direct: Core,
    values: Core,
}

/// Evaluates `defined(name)` against eval dynamic constant names.
pub(in crate::interpreter) fn eval_builtin_defined(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [name] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let name = eval_expr(name, context, scope, values)?;
    let exists = eval_defined_name(name, context, values)?;
    values.bool_value(exists)
}

/// Evaluates `defined(...)` from already materialized call arguments.
pub(in crate::interpreter) fn eval_defined_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [name] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let exists = eval_defined_name(*name, context, values)?;
    values.bool_value(exists)
}

/// Normalizes and probes one eval dynamic constant name.
fn eval_defined_name(
    name: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let name = eval_constant_name(name, values)?;
    if let Some((class_name, constant)) = name.split_once("::") {
        return eval_defined_class_constant(class_name, constant, context, values);
    }
    Ok(context.has_native_global_constant(&name)
        || context.has_native_user_constant(&name)
        || eval_predefined_constant_value(&name).is_some()
        || context.has_constant(&name))
}

/// Answers `defined('Class::CONST')`.
///
/// PHP accepts a class constant here and autoloads the class to answer it; a class or constant
/// that does not exist is `false`, never an error. Symfony's
/// `DebugHandlersListener::getSubscribedEvents()` subscribes to `console.command` only when
/// `defined('Symfony\Component\Console\ConsoleEvents::COMMAND')`, and answering `false` dropped
/// that listener from every container `cache:clear` rebuilt.
fn eval_defined_class_constant(
    class_name: &str,
    constant: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let class_name = context
        .resolve_class_like_name(class_name)
        .unwrap_or_else(|| class_name.trim_start_matches('\\').to_string());
    let known = |context: &ElephcEvalContext, values: &mut _| -> Result<bool, EvalStatus> {
        eval_class_like_is_known(&class_name, context, values)
    };
    if !known(context, values)? {
        let _ = crate::interpreter::eval_spl_autoload_class(&class_name, context, values)?;
        #[cfg(not(test))]
        context.sync_global_eval_classes();
        if !known(context, values)? {
            return Ok(false);
        }
    }
    if context.enum_case(&class_name, constant).is_some()
        || context.class_constant(&class_name, constant).is_some()
    {
        return Ok(true);
    }
    match values.class_constant_get(&class_name, constant)? {
        Some(value) => {
            values.release(value)?;
            Ok(true)
        }
        None => Ok(false),
    }
}
