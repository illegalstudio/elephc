//! Purpose:
//! Eval registry entry and implementation for `settype`.
//!
//! Called from:
//! - `crate::interpreter::builtins::hooks`.
//!
//! Key details:
//! - Direct calls preserve the writable first argument target and write the
//!   converted value back after source-order argument evaluation.

use super::super::super::*;

eval_builtin! {
    name: "settype",
    area: Types,
    params: [var: by_ref, r#type],
    by_ref: [var],
    direct: none,
    values: Settype,
}

/// Evaluates direct by-reference `settype()` calls and writes the converted cell back.
pub(in crate::interpreter) fn eval_builtin_settype_call(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (value, target, type_name) = eval_settype_direct_args(args, context, scope, values)?;
    let Some(converted) = eval_settype_cast_value(value, type_name, values)? else {
        return values.bool_value(false);
    };
    eval_write_direct_ref_target(
        &target,
        converted,
        context,
        values,
        Some(ScopeCellOwnership::Owned),
    )?;
    values.bool_value(true)
}

/// Evaluates and binds direct `settype()` arguments while preserving source order.
pub(in crate::interpreter) fn eval_settype_direct_args(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, EvalReferenceTarget, RuntimeCellHandle), EvalStatus> {
    let mut var_target = None;
    let mut type_name = None;
    let mut positional_index = 0;
    let mut saw_named = false;

    for arg in args {
        if arg.is_spread() {
            return Err(EvalStatus::RuntimeFatal);
        }
        let parameter = if let Some(name) = arg.name() {
            saw_named = true;
            name
        } else {
            if saw_named {
                return Err(EvalStatus::RuntimeFatal);
            }
            let parameter = match positional_index {
                0 => "var",
                1 => "type",
                _ => return Err(EvalStatus::RuntimeFatal),
            };
            positional_index += 1;
            parameter
        };

        match parameter {
            "var" => {
                if var_target.is_some() {
                    return Err(EvalStatus::RuntimeFatal);
                }
                let (value, target) = eval_call_arg_value(arg.value(), context, scope, values)?;
                let target = target.ok_or(EvalStatus::RuntimeFatal)?;
                var_target = Some((value, target));
            }
            "type" => {
                if type_name.is_some() {
                    return Err(EvalStatus::RuntimeFatal);
                }
                type_name = Some(eval_expr(arg.value(), context, scope, values)?);
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        }
    }

    let (value, target) = var_target.ok_or(EvalStatus::RuntimeFatal)?;
    let type_name = type_name.ok_or(EvalStatus::RuntimeFatal)?;
    Ok((value, target, type_name))
}

/// Dispatches by-value `settype()` callable calls after argument binding.
pub(in crate::interpreter) fn eval_settype_values_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value, type_name] = evaluated_args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    eval_settype_value_result(*value, *type_name, values)
}

/// Applies the eval-supported `settype()` scalar target conversion.
pub(in crate::interpreter) fn eval_settype_cast_value(
    value: RuntimeCellHandle,
    type_name: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let type_name = values.string_bytes(type_name)?;
    let type_name = String::from_utf8_lossy(&type_name).to_ascii_lowercase();
    let converted = match type_name.as_str() {
        "bool" | "boolean" => Some(values.cast_bool(value)?),
        "float" | "double" => Some(values.cast_float(value)?),
        "int" | "integer" => Some(values.cast_int(value)?),
        "string" => Some(values.cast_string(value)?),
        _ => None,
    };
    Ok(converted)
}

/// Evaluates by-value `settype()` callable dispatch without mutating the source argument.
pub(in crate::interpreter) fn eval_settype_value_result(
    value: RuntimeCellHandle,
    type_name: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.warning("settype(): Argument #1 ($var) must be passed by reference, value given")?;
    if let Some(converted) = eval_settype_cast_value(value, type_name, values)? {
        values.release(converted)?;
        return values.bool_value(true);
    }
    values.bool_value(false)
}
