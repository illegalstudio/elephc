//! Purpose:
//! Declarative eval registry entry for `array_splice`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Direct calls stay on the source-sensitive by-reference path.

use super::super::super::*;

mod builders;

eval_builtin! {
    contract: "array_splice",
    area: Array,
    direct: none,
    values: ArrayMutating,
}
/// Dispatches by-value callable eval calls for the `array_splice` array mutator.
pub(in crate::interpreter) fn eval_array_splice_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let result = match evaluated_args {
        [array, offset] => eval_array_splice_value_result(*array, *offset, None, context, values)?,
        [array, offset, length] => eval_array_splice_value_result(*array, *offset, Some(*length), context, values)?,
        [array, offset, length, _replacement] => eval_array_splice_value_result(*array, *offset, Some(*length), context, values)?,
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    values.warning("array_splice(): Argument #1 ($array) must be passed by reference, value given")?;
    Ok(result)
}

/// Evaluates direct by-reference `array_splice()` calls and writes back the array.
pub(in crate::interpreter) fn eval_builtin_array_splice_call(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (array, target, offset, length, replacement_arg) =
        eval_array_splice_direct_args(args, context, scope, values)?;

    let (removed, replacement) =
        eval_array_splice_removed_and_replacement(array, offset, length, replacement_arg, context, values)?;
    eval_write_direct_ref_target(&target, replacement, context, values, None)?;
    Ok(removed)
}

/// Evaluates and binds direct `array_splice()` arguments while preserving source order.
pub(in crate::interpreter) fn eval_array_splice_direct_args(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalArraySpliceDirectArgs, EvalStatus> {
    let mut array = None;
    let mut offset = None;
    let mut length = None;
    let mut replacement = None;
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
                0 => "array",
                1 => "offset",
                2 => "length",
                3 => "replacement",
                _ => return Err(EvalStatus::RuntimeFatal),
            };
            positional_index += 1;
            parameter
        };

        match parameter {
            "array" => {
                if array.is_some() {
                    return Err(EvalStatus::RuntimeFatal);
                }
                array = Some(super::mutation::eval_array_mutation_lvalue_arg(
                    arg, context, scope, values,
                )?);
            }
            "offset" => {
                if offset.is_some() {
                    return Err(EvalStatus::RuntimeFatal);
                }
                offset = Some(eval_expr(arg.value(), context, scope, values)?);
            }
            "length" => {
                if length.is_some() {
                    return Err(EvalStatus::RuntimeFatal);
                }
                length = Some(eval_expr(arg.value(), context, scope, values)?);
            }
            "replacement" => {
                if replacement.is_some() {
                    return Err(EvalStatus::RuntimeFatal);
                }
                replacement = Some(eval_expr(arg.value(), context, scope, values)?);
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        }
    }

    let (array, target) = array.ok_or(EvalStatus::RuntimeFatal)?;
    let offset = offset.ok_or(EvalStatus::RuntimeFatal)?;
    Ok((array, target, offset, length, replacement))
}

/// Returns the removed elements that `array_splice()` would produce without mutating the source.
pub(in crate::interpreter) fn eval_array_splice_value_result(
    array: RuntimeCellHandle,
    offset: RuntimeCellHandle,
    length: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !matches!(values.type_tag(array)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let (start, end) = eval_array_splice_bounds(array, offset, length, values)?;
    builders::build(array, start..end, None, 0..0, context, values)
}

/// Builds both removed and replacement arrays for direct `array_splice()` write-back.
pub(in crate::interpreter) fn eval_array_splice_removed_and_replacement(
    array: RuntimeCellHandle,
    offset: RuntimeCellHandle,
    length: Option<RuntimeCellHandle>,
    replacement: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, RuntimeCellHandle), EvalStatus> {
    let (start, end) = eval_array_splice_bounds(array, offset, length, values)?;
    let removed = builders::build(array, start..end, None, 0..0, context, values)?;
    let removed = crate::runtime_hooks::owner_frames::RuntimeOwnerSlot::new(removed);
    crate::runtime_hooks::owner_frames::with_runtime_owners(&removed, || {
        let result = values.array_len(array).and_then(|len|
            builders::build(array, 0..start, replacement, end..len, context, values));
        match result {
            Ok(replacement) => Ok((removed.take(), replacement)),
            Err(status) => {
                let _ = release_expr_result(removed.take(), context, values);
                Err(status)
            }
        }
    })
}

/// Converts splice offset and length cells into bounded source positions.
pub(in crate::interpreter) fn eval_array_splice_bounds(
    array: RuntimeCellHandle,
    offset: RuntimeCellHandle,
    length: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<(usize, usize), EvalStatus> {
    let len = values.array_len(array)?;
    let offset = eval_int_value(offset, values)?;
    let start = super::array_slice::eval_slice_start(len, offset)?;
    let end = match length {
        Some(length) if values.type_tag(length)? != EVAL_TAG_NULL => {
            super::array_slice::eval_slice_end(len, start, eval_int_value(length, values)?)?
        }
        _ => len,
    };
    Ok((start, end))
}
