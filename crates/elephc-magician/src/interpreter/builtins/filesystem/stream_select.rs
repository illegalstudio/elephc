//! Purpose:
//! Declarative eval registry entry and implementation for `stream_select`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//! - `crate::interpreter::expressions::eval_call()` for by-reference arrays.
//!
//! Key details:
//! - `stream_select()` rewrites read/write/except arrays through by-reference
//!   targets after validating resource handles.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "stream_select",
    area: Filesystem,
    params: [
        read: by_ref,
        write: by_ref,
        except: by_ref,
        seconds,
        microseconds = EvalBuiltinDefaultValue::Int(0)
    ],
    by_ref: [read, write, except],
    direct: none,
    values: Filesystem,
}

use super::super::super::*;
use super::*;

/// Evaluates a positional `stream_select()` call without writable array outputs.
pub(in crate::interpreter) fn eval_stream_select_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(4..=5).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut evaluated_args = Vec::with_capacity(args.len());
    for arg in args {
        evaluated_args.push(eval_expr(arg, context, scope, values)?);
    }
    eval_stream_select_by_value_ref_warnings(evaluated_args.len(), values)?;
    eval_stream_select_result(&evaluated_args, context, values)
}

/// Evaluates `stream_select()` from already evaluated by-value arguments.
pub(in crate::interpreter) fn eval_stream_select_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_stream_select_by_value_ref_warnings(evaluated_args.len(), values)?;
    eval_stream_select_result(evaluated_args, context, values)
}

/// Evaluates `stream_select()` over full eval call metadata.
pub(in crate::interpreter) fn eval_builtin_stream_select_call(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let evaluated_args = eval_call_arg_values(args, context, scope, values)?;
    let (bound, _) = bind_evaluated_ref_builtin_args(
        &["read", "write", "except", "seconds", "microseconds"],
        &evaluated_args,
        false,
    )?;
    let read = required_evaluated_ref_arg(&bound, 0)?;
    let write = required_evaluated_ref_arg(&bound, 1)?;
    let except = required_evaluated_ref_arg(&bound, 2)?;
    let seconds = required_evaluated_ref_arg(&bound, 3)?;
    let targets = vec![
        read.ref_target.clone().ok_or(EvalStatus::RuntimeFatal)?,
        write.ref_target.clone().ok_or(EvalStatus::RuntimeFatal)?,
        except.ref_target.clone().ok_or(EvalStatus::RuntimeFatal)?,
    ];
    let mut selected_args = vec![read.value, write.value, except.value, seconds.value];
    if let Some(microseconds) = optional_evaluated_ref_arg(&bound, 4) {
        selected_args.push(microseconds.value);
    }
    let result = eval_stream_select_result(&selected_args, context, values)?;
    eval_write_stream_select_empty_arrays(&targets, context, values)?;
    Ok(result)
}

/// Evaluates materialized `stream_select(...)` arguments.
pub(in crate::interpreter) fn eval_stream_select_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(4..=5).contains(&evaluated_args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    for array in evaluated_args.iter().take(3) {
        eval_stream_select_cast_array(*array, context, values)?;
    }
    values.int(0)
}

/// Emits PHP by-reference warnings for by-value `stream_select()` array outputs.
fn eval_stream_select_by_value_ref_warnings(
    supplied_count: usize,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for (index, param_name) in ["read", "write", "except"].iter().enumerate() {
        if supplied_count <= index {
            continue;
        }
        values.warning(&format!(
            "stream_select(): Argument #{} (${param_name}) must be passed by reference, value given",
            index + 1
        ))?;
    }
    Ok(())
}

/// Writes conservative empty readiness arrays back to `stream_select()` lvalues.
fn eval_write_stream_select_empty_arrays(
    targets: &[EvalReferenceTarget],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for target in targets {
        let value = values.array_new(0)?;
        eval_write_direct_ref_target(
            target,
            value,
            context,
            values,
            Some(ScopeCellOwnership::Owned),
        )?;
    }
    Ok(())
}

/// Invokes `stream_cast(STREAM_CAST_FOR_SELECT)` for wrapper resources in an array.
fn eval_stream_select_cast_array(
    array: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if !values.is_array_like(array)? {
        return Ok(());
    }
    let len = values.array_len(array)?;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, key)?;
        eval_stream_select_cast_value(value, context, values)?;
    }
    Ok(())
}

/// Invokes `stream_cast()` for one userspace-wrapper stream resource value.
fn eval_stream_select_cast_value(
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if values.type_tag(value)? != EVAL_TAG_RESOURCE {
        return Ok(());
    }
    let display_id = eval_int_value(value, values)?;
    let Some(id) = display_id.checked_sub(1) else {
        return Ok(());
    };
    let Some(result) =
        eval_user_wrapper_stream_cast_result(id, EVAL_STREAM_CAST_FOR_SELECT, context, values)?
    else {
        return Ok(());
    };
    values.release(result)
}
