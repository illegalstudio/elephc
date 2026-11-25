//! Purpose:
//! Eval registry entry and implementation for `define`.
//!
//! Called from:
//! - `crate::interpreter::builtins::core`.
//!
//! Key details:
//! - Dynamic constants are stored in the eval context after predefined-constant
//!   checks and keep PHP's case-sensitive dynamic constant names.

use super::super::super::*;

eval_builtin! {
    contract: "define",
    area: Core,
    direct: Core,
    values: Core,
}

/// Evaluates `define(name, value)` for eval dynamic constant-name registration.
pub(in crate::interpreter) fn eval_builtin_define(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [name, value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value_expr = value;
    let name = eval_expr(name, context, scope, values)?;
    let value = eval_expr(value, context, scope, values)?;
    let is_string = values.type_tag(value)? == EVAL_TAG_STRING;
    let constant_name = eval_constant_name(name, values)?;
    let defined = eval_define_name(name, value, context, values)?;
    // A string the argument built at run time (anything but a literal or a constant read) is
    // refcounted in php-src, which decides whether a parameter default reading the constant
    // is cached (`eval_result_is_refcounted`).
    if defined
        && is_string
        && !matches!(value_expr, EvalExpr::Const(_) | EvalExpr::ConstFetch(_) | EvalExpr::NamespacedConstFetch { .. } | EvalExpr::ClassConstantFetch { .. })
    {
        context.record_built_constant(&constant_name);
    }
    values.bool_value(defined)
}

/// Evaluates `define(...)` from already materialized call arguments.
pub(in crate::interpreter) fn eval_define_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [name, value] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let defined = eval_define_name(*name, *value, context, values)?;
    values.bool_value(defined)
}

/// Normalizes and registers one eval dynamic constant name.
fn eval_define_name(
    name: RuntimeCellHandle,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let name = eval_constant_name(name, values)?;
    if name.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    if eval_constant_already_defined(&name, context) {
        values.warning(DEFINE_ALREADY_DEFINED_WARNING)?;
        return Ok(false);
    }
    let value = values.retain(value)?;
    if context.define_constant(&name, value) {
        Ok(true)
    } else {
        values.release(value)?;
        Ok(false)
    }
}

/// Reports whether a global constant name already resolves, natively or in eval.
///
/// `define()` and the `const` statement refuse to redefine any of these names.
pub(in crate::interpreter) fn eval_constant_already_defined(
    name: &str,
    context: &ElephcEvalContext,
) -> bool {
    context.has_native_global_constant(name)
        || context.has_native_user_constant(name)
        || eval_predefined_constant_value(name).is_some()
        || context.has_constant(name)
}

/// Reads a PHP constant name from a runtime cell without changing case.
pub(in crate::interpreter) fn eval_constant_name(
    name: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<String, EvalStatus> {
    let name = values.string_bytes(name)?;
    String::from_utf8(name).map_err(|_| EvalStatus::RuntimeFatal)
}
