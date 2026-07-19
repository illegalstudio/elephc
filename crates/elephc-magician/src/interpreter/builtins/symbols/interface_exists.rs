//! Purpose:
//! Eval registry entry and implementation for `interface_exists`.
//!
//! Called from:
//! - `crate::interpreter::builtins::symbols`.
//!
//! Key details:
//! - Lookup checks eval interface declarations before generated/AOT runtime metadata.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "interface_exists",
    area: Symbols,
    params: [interface, autoload = EvalBuiltinDefaultValue::Bool(true)],
    direct: Symbols,
    values: Symbols,
}

use super::super::super::*;

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
    let name = match args {
        [name] => eval_expr(name, context, scope, values)?,
        [name, autoload] => {
            let name = eval_expr(name, context, scope, values)?;
            let _ = eval_expr(autoload, context, scope, values)?;
            name
        }
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let exists = eval_interface_exists_name(name, context, values)?;
    values.bool_value(exists)
}

/// Evaluates `interface_exists(...)` from already materialized call arguments.
pub(in crate::interpreter) fn eval_interface_exists_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let exists = match evaluated_args {
        [name] => eval_interface_exists_name(*name, context, values)?,
        [name, _autoload] => eval_interface_exists_name(*name, context, values)?,
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    values.bool_value(exists)
}

/// Normalizes a PHP interface-name cell and probes eval and generated interface metadata.
pub(in crate::interpreter) fn eval_interface_exists_name(
    name: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let name = values.string_bytes(name)?;
    let name = String::from_utf8(name).map_err(|_| EvalStatus::RuntimeFatal)?;
    let name = name.trim_start_matches('\\');
    Ok(context.has_interface(name) || eval_runtime_interface_exists(name, values)?)
}
