//! Purpose:
//! Eval registry entry and implementation for `class_exists`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Lookup checks eval declarations before generated/AOT runtime metadata.
//! - `Closure` is treated as a built-in class-like symbol.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "class_exists",
    area: Symbols,
    params: [r#class, autoload = EvalBuiltinDefaultValue::Bool(true)],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

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
    let name = match args {
        [name] => eval_expr(name, context, scope, values)?,
        [name, autoload] => {
            let name = eval_expr(name, context, scope, values)?;
            let _ = eval_expr(autoload, context, scope, values)?;
            name
        }
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let exists = eval_class_exists_name(name, context, values)?;
    values.bool_value(exists)
}

/// Evaluates `class_exists(...)` from already materialized call arguments.
pub(in crate::interpreter) fn eval_class_exists_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let exists = match evaluated_args {
        [name] => eval_class_exists_name(*name, context, values)?,
        [name, _autoload] => eval_class_exists_name(*name, context, values)?,
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    values.bool_value(exists)
}

/// Normalizes a PHP class-name cell and probes dynamic names before generated classes.
pub(in crate::interpreter) fn eval_class_exists_name(
    name: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let name = values.string_bytes(name)?;
    let name = String::from_utf8(name).map_err(|_| EvalStatus::RuntimeFatal)?;
    let name = name.trim_start_matches('\\');
    if name.eq_ignore_ascii_case("Closure") {
        return Ok(true);
    }
    if context.has_class(name) {
        return Ok(true);
    }
    values.class_exists(name)
}
