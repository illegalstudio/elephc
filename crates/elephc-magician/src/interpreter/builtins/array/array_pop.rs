//! Purpose:
//! Declarative eval registry entry for `array_pop`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Direct calls stay on the source-sensitive by-reference path.

use super::super::super::*;
use crate::interpreter::builtins::collection_builder::{
    EvalArrayBuilder, with_collection_call, with_collection_cells, with_collection_result,
};

eval_builtin! {
    contract: "array_pop",
    area: Array,
    direct: none,
    values: ArrayMutating,
}
/// Dispatches by-value callable eval calls for the `array_pop` array mutator.
pub(in crate::interpreter) fn eval_array_pop_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    _context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = evaluated_args else { return Err(EvalStatus::RuntimeFatal); };
    eval_warn_array_by_value("array_pop", values)?;
    eval_array_pop_shift_value_result("array_pop", *array, values)
}

/// Emits the standard by-value warning for array mutator callable calls.
pub(in crate::interpreter) fn eval_warn_array_by_value(
    name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    values.warning(&format!(
        "{name}(): Argument #1 ($array) must be passed by reference, value given"
    ))
}

/// Returns the value produced by `array_pop()` / `array_shift()` without mutating the source.
pub(in crate::interpreter) fn eval_array_pop_shift_value_result(
    name: &str,
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !matches!(values.type_tag(array)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let len = values.array_len(array)?;
    if len == 0 {
        return values.null();
    }
    let position = match name {
        "array_pop" => len - 1,
        "array_shift" => 0,
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    with_collection_result(values, |values, result| {
        with_collection_cells(values, |values, owners| {
            let key = owners.stage(values.array_iter_key(array, position)?);
            result.replace(Some(values.array_get(array, key)?));
            Ok(())
        })
    })
}

/// Builds the return value plus replacement array for direct pop/shift write-back.
pub(in crate::interpreter) fn eval_array_pop_shift_replacement(
    name: &str,
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, RuntimeCellHandle), EvalStatus> {
    let len = values.array_len(array)?;
    let tag = values.type_tag(array)?;
    let mut replacement = None;
    let removed = with_collection_result(values, |values, removed| {
        removed.replace(Some(eval_array_pop_shift_value_result(name, array, values)?));
        let removed_position = if name == "array_pop" { len.saturating_sub(1) } else { 0 };
        let array = match tag {
            EVAL_TAG_ARRAY => eval_array_pop_shift_indexed_replacement(array, removed_position, len, values)?,
            EVAL_TAG_ASSOC => eval_array_pop_shift_assoc_replacement(name, array, removed_position, len, values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        };
        replacement = Some(array);
        Ok(())
    })?;
    Ok((removed, replacement.expect("successful replacement")))
}

/// Rebuilds an indexed array after removing one position and reindexing values.
pub(in crate::interpreter) fn eval_array_pop_shift_indexed_replacement(
    array: RuntimeCellHandle,
    removed_position: usize,
    len: usize,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = EvalArrayBuilder::indexed(values, len.saturating_sub(1))?;
    let mut target = 0_i64;
    for position in 0..len {
        if position == removed_position {
            continue;
        }
        result.update(|values, owners, _| {
            let source_key = owners.stage(values.array_iter_key(array, position)?);
            let value = owners.stage(values.array_get(array, source_key)?);
            let target_key = owners.stage(values.int(target)?);
            target = target.checked_add(1).ok_or(EvalStatus::RuntimeFatal)?;
            Ok(Some((target_key, value)))
        })?;
    }
    Ok(result.finish())
}

/// Rebuilds an associative array after pop/shift, preserving PHP key behavior.
pub(in crate::interpreter) fn eval_array_pop_shift_assoc_replacement(
    name: &str,
    array: RuntimeCellHandle,
    removed_position: usize,
    len: usize,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if name == "array_shift"
        && eval_array_remaining_keys_are_int(array, removed_position, len, values)?
    {
        return eval_array_pop_shift_indexed_replacement(array, removed_position, len, values);
    }

    let mut result = EvalArrayBuilder::assoc(values, len.saturating_sub(1))?;
    let mut next_int_key = 0_i64;
    for position in 0..len {
        if position == removed_position {
            continue;
        }
        result.update(|values, owners, _| {
            let source_key = owners.stage(values.array_iter_key(array, position)?);
            let target_key = if name == "array_shift" && values.type_tag(source_key)? == EVAL_TAG_INT {
                let key = owners.stage(values.int(next_int_key)?);
                next_int_key = next_int_key
                    .checked_add(1)
                    .ok_or(EvalStatus::RuntimeFatal)?;
                key
            } else {
                source_key
            };
            let value = owners.stage(values.array_get(array, source_key)?);
            Ok(Some((target_key, value)))
        })?;
    }
    Ok(result.finish())
}

/// Returns true when every remaining key is an integer after removing one element.
pub(in crate::interpreter) fn eval_array_remaining_keys_are_int(
    array: RuntimeCellHandle,
    removed_position: usize,
    len: usize,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    for position in 0..len {
        if position == removed_position {
            continue;
        }
        let integer = with_collection_cells(values, |values, owners| {
            let key = owners.stage(values.array_iter_key(array, position)?);
            Ok(values.type_tag(key)? == EVAL_TAG_INT)
        })?;
        if !integer {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Evaluates direct by-reference `array_pop()` / `array_shift()` calls and writes back the array.
pub(in crate::interpreter) fn eval_array_pop_shift_declared_call(
    name: &str,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [arg] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    with_collection_call(context, values, |context, values, owners, output| {
        let (array, target) = super::mutation::eval_array_mutation_lvalue_arg(
            arg, context, scope, values, owners,
        )?;
        let (removed, replacement) = eval_array_pop_shift_replacement(name, array, values)?;
        output.replace(Some(removed));
        write_back_owned_ref_target(&target, replacement, context, values)
    })
}
