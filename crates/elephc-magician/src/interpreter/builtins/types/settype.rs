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
    contract: "settype",
    area: Types,
    direct: none,
    values: Settype,
}

/// Keeps direct settype operands and reference receivers owned through conversion and writeback.
pub(in crate::interpreter) fn eval_builtin_settype_call(
    args: &[EvalCallArg], context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    with_eval_call_arguments(args, context, scope, values, |args, context, _, values| {
        let (value, target, type_name) = bind_settype_arguments(args)?;
        let Some(converted) = eval_settype_cast_value(value, type_name, values)? else {
            return values.bool_value(false);
        };
        if matches!(&target, EvalReferenceTarget::Variable { .. }) {
            // Variable assignment consumes the converted owner on success and rejection.
            eval_write_direct_ref_target(&target, converted, context, values, Some(ScopeCellOwnership::Owned))?;
            values.bool_value(true)
        } else {
            let result = eval_write_direct_ref_target(&target, converted, context, values, None)
                .and_then(|()| values.bool_value(true));
            finish_synthesized_method_arguments(result, vec![converted], context, values)
        }
    })
}

/// Binds already evaluated named or positional operands without repeating PHP side effects.
fn bind_settype_arguments(
    args: Vec<EvaluatedCallArg>,
) -> Result<(RuntimeCellHandle, EvalReferenceTarget, RuntimeCellHandle), EvalStatus> {
    let mut variable = None;
    let mut type_name = None;
    let mut positional = 0;
    for arg in args {
        let parameter = if let Some(name) = arg.name.as_deref() { name } else {
            let name = match positional { 0 => "var", 1 => "type", _ => return Err(EvalStatus::RuntimeFatal) };
            positional += 1;
            name
        };
        match parameter {
            "var" if variable.is_none() => {
                variable = Some((arg.value, arg.ref_target.ok_or(EvalStatus::RuntimeFatal)?));
            }
            "type" if type_name.is_none() => type_name = Some(arg.value),
            _ => return Err(EvalStatus::RuntimeFatal),
        }
    }
    let (value, target) = variable.ok_or(EvalStatus::RuntimeFatal)?;
    Ok((value, target, type_name.ok_or(EvalStatus::RuntimeFatal)?))
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
