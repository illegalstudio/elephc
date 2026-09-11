//! Purpose:
//! Executes registered native functions and balances temporary argument arrays.
//!
//! Called from:
//! - Dynamic function dispatch after native signature binding.
//!
//! Key details:
//! - By-reference writeback and temporary runtime-cell ownership are handled together.
//! - The container holds exactly what the caller supplied: the binder truncates the trailing
//!   optionals the caller omitted, and appends surplus positional arguments past the declared
//!   parameters. The descriptor invoker applies the omitted defaults itself, so the arity gate
//!   only checks the mandatory prefix.
//! - The container is INDEXED unless the binder recorded a string key, which only an unknown
//!   named argument absorbed by a source-declared variadic can produce. Then it is associative,
//!   with every positional entry keeping its own integer key ahead of every string key, which is
//!   the order the invoker's container validation requires.

use super::*;

/// Evaluates a registered AOT function through its descriptor-compatible invoker.
pub(in crate::interpreter) fn eval_native_function(
    function: NativeFunction,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    with_eval_call_arguments(args, context, caller_scope, values, |arguments, context, _, values| {
        let bound = bind_evaluated_native_function_args(&function, arguments, context, values)?;
        eval_native_function_with_values(function, bound, context, values)
    })
}

/// Invokes a registered AOT function after its arguments have been bound and staged.
pub(in crate::interpreter) fn eval_native_function_with_values(
    function: NativeFunction,
    bound_args: BoundNativeFunctionArgs,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let result = invoke_native_function_with_staged_args(&function, &bound_args, context, values);
    finish_eval_argument_values(result, bound_args.values, context, values)
}

/// Invokes the native descriptor while keeping binder-owned cells alive through writeback.
fn invoke_native_function_with_staged_args(
    function: &NativeFunction,
    bound_args: &BoundNativeFunctionArgs,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !function.bridge_supported() {
        cleanup_native_function_ref_args(bound_args, values)?;
        return Err(EvalStatus::RuntimeFatal);
    }
    // `required_param_count()` already stops at the variadic slot for a variadic signature, so
    // one gate covers both shapes.
    if bound_args.values.len() < function.required_param_count() {
        cleanup_native_function_ref_args(bound_args, values)?;
        return Err(EvalStatus::RuntimeFatal);
    }
    let arg_array = match build_native_function_arg_array(bound_args, values) {
        Ok(arg_array) => arg_array,
        Err(status) => {
            cleanup_native_function_ref_args(bound_args, values)?;
            return Err(status);
        }
    };
    let result = unsafe { function.call(arg_array) };
    // Transfer a native exception before any cleanup can run another destructor.
    let result = values.native_call_result(result);
    let writeback = write_back_native_function_ref_args(bound_args, context, values);
    let result = match (result, writeback) {
        (Err(status), _) => Err(status),
        (Ok(value), Err(status)) => {
            let _ = eval_release_value(context, values, value);
            Err(status)
        }
        (Ok(value), Ok(())) => Ok(value),
    };
    let result = finish_eval_argument_values(result, [arg_array], context, values)?;
    eval_declared_native_return_value(function.return_type(), None, None, result, context, values)
}

/// Builds the runtime container passed to descriptor-compatible native invokers.
///
/// Every staged value is inserted under its own key: its container position when the binder
/// recorded none, or the argument's PHP name when it is an unknown named argument a
/// source-declared variadic absorbs. The key cell is a temporary this function owns; `array_set`
/// only borrows it, and the container retains the value, so the key is released either way and
/// the staged value keeps the single owner staging gave it.
fn build_native_function_arg_array(
    bound_args: &BoundNativeFunctionArgs,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let named = bound_args.named_keys.iter().any(Option::is_some);
    let arg_array = if named {
        values.assoc_new(bound_args.values.len())?
    } else {
        values.array_new(bound_args.values.len())?
    };
    for (index, value) in bound_args.values.iter().copied().enumerate() {
        let key = match bound_args.named_keys.get(index).and_then(Option::as_deref) {
            Some(name) => values.string(name),
            None => values.int(index as i64),
        };
        let key = match key {
            Ok(key) => key,
            Err(status) => {
                values.release(arg_array)?;
                return Err(status);
            }
        };
        let inserted = values.array_set(arg_array, key, value);
        let released = values.release(key);
        if let Err(status) = inserted.and(released) {
            values.release(arg_array)?;
            return Err(status);
        }
    }
    Ok(arg_array)
}

/// Releases current staging owners; native replacement has already consumed each old slot value.
pub(in crate::interpreter) fn cleanup_native_function_ref_args(
    bound_args: &BoundNativeFunctionArgs,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let mut result = Ok(());
    for ref_slot in &bound_args.ref_slots {
        let released = match ref_slot {
            BoundNativeFunctionRefSlot::RawString { slot, .. } => {
                let words = **slot;
                values.release_raw_string_words(words[0], words[1])
            }
            BoundNativeFunctionRefSlot::OwnedRawWord { slot, .. } => {
                let word = **slot;
                values.release_raw_heap_word(word)
            }
            BoundNativeFunctionRefSlot::Mixed { slot, .. } => {
                values.release(RuntimeCellHandle::from_raw(**slot))
            }
            BoundNativeFunctionRefSlot::RawWord { .. } => Ok(()),
        };
        if result.is_ok() { result = released; }
    }
    result
}

/// Writes changed staged native-function by-reference slots back to eval caller targets.
pub(in crate::interpreter) fn write_back_native_function_ref_args(
    bound_args: &BoundNativeFunctionArgs,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let mut result = Ok(());
    for ref_slot in &bound_args.ref_slots {
        let written = write_back_native_function_ref_slot(ref_slot, context, values);
        if result.is_ok() { result = written; }
    }
    result
}

/// Consumes one staging owner, publishing changed values before releasing displaced storage.
fn write_back_native_function_ref_slot(
    ref_slot: &BoundNativeFunctionRefSlot,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    match ref_slot {
        BoundNativeFunctionRefSlot::Mixed {
            original,
            slot,
            target,
        } => {
            let value = RuntimeCellHandle::from_raw(**slot);
            if value == *original {
                return eval_release_value(context, values, value);
            }
            let Some(target) = target else {
                return eval_release_value(context, values, value);
            };
            publish_native_function_ref_value(target, value, context, values)
        }
        BoundNativeFunctionRefSlot::RawWord {
            tag,
            original,
            slot,
            target,
        } => {
            let word = **slot;
            if word == *original {
                return Ok(());
            }
            let Some(target) = target else {
                return Ok(());
            };
            let value = values.raw_word_value(*tag, word)?;
            publish_native_function_ref_value(target, value, context, values)
        }
        BoundNativeFunctionRefSlot::RawString {
            original,
            slot,
            target,
        } => {
            let words = **slot;
            let Some(target) = target else {
                return values.release_raw_string_words(words[0], words[1]);
            };
            if words == *original {
                return values.release_raw_string_words(words[0], words[1]);
            }
            let value = values.raw_string_value(words[0], words[1]);
            let released = values.release_raw_string_words(words[0], words[1]);
            publish_native_function_ref_conversion(target, value, released, context, values)
        }
        BoundNativeFunctionRefSlot::OwnedRawWord {
            original,
            slot,
            target,
        } => {
            let word = **slot;
            let Some(target) = target else {
                return values.release_raw_heap_word(word);
            };
            if word == *original {
                return values.release_raw_heap_word(word);
            }
            let value = values.raw_heap_word_value(word);
            let released = values.release_raw_heap_word(word);
            publish_native_function_ref_conversion(target, value, released, context, values)
        }
    }
}

/// Discards a fresh box if retiring its raw staging owner failed before publication.
fn publish_native_function_ref_conversion(
    target: &EvalReferenceTarget,
    value: Result<RuntimeCellHandle, EvalStatus>,
    released: Result<(), EvalStatus>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    match (value, released) {
        (Err(status), _) => Err(status),
        (Ok(value), Err(status)) => {
            let _ = eval_release_value(context, values, value);
            Err(status)
        }
        (Ok(value), Ok(())) => publish_native_function_ref_value(target, value, context, values),
    }
}

/// Transfers a staged box to variable storage, or lends it to a retaining container setter.
fn publish_native_function_ref_value(
    target: &EvalReferenceTarget,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if let EvalReferenceTarget::Variable { scope, name } = target {
        let replaced = unsafe { scope.as_mut() }
            .ok_or(EvalStatus::RuntimeFatal)
            .and_then(|scope| set_owned_scope_cell(context, scope, name.clone(), value));
        let replaced = match replaced {
            Ok(replaced) => replaced,
            Err(status) => {
                let _ = eval_release_value(context, values, value);
                return Err(status);
            }
        };
        let mut result = Ok(());
        for replaced in replaced {
            let released = eval_release_value(context, values, replaced);
            if result.is_ok() { result = released; }
        }
        return result;
    }
    let written = write_back_method_ref_target(target, value.borrowed(), context, values);
    let released = eval_release_value(context, values, value);
    written.and(released)
}
