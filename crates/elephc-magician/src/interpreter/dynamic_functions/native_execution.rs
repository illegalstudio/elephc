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
use crate::context::push_native_frame_context;
use crate::interpreter::builtins::collection_builder::EvalArrayBuilder;
use crate::runtime_hooks::owner_frames::{RuntimeOwnerSlot, with_runtime_owners};
use elephc_memoir::runtime_owners::{CleanupErrorPolicy, release_in_order};

/// Evaluates a registered AOT function through its descriptor-compatible invoker.
pub(in crate::interpreter) fn eval_native_function(
    function: NativeFunction,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let modes = CallArgumentModes::for_native_function(&function);
    with_eval_call_arguments_with_modes(args, &modes, context, caller_scope, values, |arguments, context, _, values| {
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
    let _native_context = push_native_frame_context(context);
    let result = with_runtime_owners(&(&bound_args.values, &bound_args.ref_slots), ||
        invoke_native_function_with_staged_args(&function, &bound_args, context, values));
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
    let arg_array = RuntimeOwnerSlot::new(arg_array);
    with_runtime_owners(&arg_array, || {
        if let Err(status) = pin_native_call_evaluation_owners(bound_args, context, values) {
            let _ = values.release(arg_array.take());
            let _ = cleanup_native_function_ref_args(bound_args, values);
            return Err(status);
        }
        let result = unsafe { function.call(arg_array.borrowed()) };
        // Transfer a native exception before any cleanup can run another destructor.
        let result = values.native_call_result(result);
        let result = finish_native_invocation(result, bound_args, &arg_array, context, values)?;
        eval_declared_native_return_value(
            Some(function.name()), function.return_type(), None, None, result, context, values,
        )
    })
}

/// Keeps a native return visible during reference writeback and argument retirement.
pub(in crate::interpreter) fn finish_native_invocation(
    result: Result<RuntimeCellHandle, EvalStatus>,
    bound_args: &BoundNativeFunctionArgs,
    arg_array: &RuntimeOwnerSlot,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let result = result.and_then(|value| promote_borrowed_result(value, values))
        .map(RuntimeOwnerSlot::new);
    with_runtime_owners(&result.as_ref().ok(), || {
        let writeback = write_back_native_function_ref_args(bound_args, context, values);
        let result = result.as_ref().map_err(|status| *status).and_then(|slot| {
            slot.finish(writeback, |cell| eval_release_value(context, values, cell))
        });
        finish_eval_argument_values(result, [arg_array.take()], context, values)
    })
}

/// Builds the runtime container passed to descriptor-compatible native invokers.
///
/// Every staged value is inserted under its own key: its container position when the binder
/// recorded none, or the argument's PHP name when it is an unknown named argument a
/// source-declared variadic absorbs. The key cell is a temporary this function owns; `array_set`
/// only borrows it, and the container retains the value, so the key is released either way and
/// the staged value keeps the single owner staging gave it.
pub(in crate::interpreter) fn build_native_function_arg_array(
    bound_args: &BoundNativeFunctionArgs,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let named = bound_args.named_keys.iter().any(Option::is_some);
    let capacity = bound_args.values.len();
    let mut arg_array = if named {
        EvalArrayBuilder::assoc(values, capacity)?
    } else {
        EvalArrayBuilder::indexed(values, capacity)?
    };
    for (index, value) in bound_args.values.iter().copied().enumerate() {
        let inserted = arg_array.entry_key_first(
            |values| match bound_args.named_keys.get(index).and_then(Option::as_deref) {
                Some(name) => values.string(name),
                None => values.int(index as i64),
            },
            |_| Ok(value.borrowed()),
        );
        if let Err(status) = inserted {
            arg_array.discard()?;
            return Err(status);
        }
    }
    Ok(arg_array.finish())
}

/// Releases current staging owners; native replacement has already consumed each old slot value.
pub(in crate::interpreter) fn cleanup_native_function_ref_args(
    bound_args: &BoundNativeFunctionArgs,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    release_in_order(&bound_args.ref_slots, CleanupErrorPolicy::First, |slot| {
        match slot {
            BoundNativeFunctionRefSlot::ManagedMixed { cell, pin, .. } => {
                pin.retire(|record| values.release_native_mixed_reference_cell(*cell, record))
            }
            BoundNativeFunctionRefSlot::RawString { slot, .. } => {
                let words = slot.as_ref().take();
                values.release_raw_string_words(words.word, words.high)
            }
            BoundNativeFunctionRefSlot::OwnedRawWord { slot, .. } => {
                let word = slot.as_ref().take().word;
                values.release_raw_heap_word(word)
            }
            BoundNativeFunctionRefSlot::Mixed { slot, .. } => {
                slot.take().map_or(Ok(()), |cell| values.release(cell))
            }
            BoundNativeFunctionRefSlot::RawWord { .. } | BoundNativeFunctionRefSlot::TaggedScalar { .. } => Ok(()),
        }
    })
}

/// Writes changed staged native-function by-reference slots back to eval caller targets.
pub(in crate::interpreter) fn write_back_native_function_ref_args(
    bound_args: &BoundNativeFunctionArgs,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    release_in_order(&bound_args.ref_slots, CleanupErrorPolicy::First,
        |slot| write_back_native_function_ref_slot(slot, context, values))
}

/// Consumes one staging owner, publishing changed values before releasing displaced storage.
fn write_back_native_function_ref_slot(
    ref_slot: &BoundNativeFunctionRefSlot,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    match ref_slot {
        // Native stores already published through the caller's live PHPRef. Only
        // retire staging's cell lease; constructors may have retained another one.
        BoundNativeFunctionRefSlot::ManagedMixed { cell, pin, .. } => {
            pin.retire(|record| values.release_native_mixed_reference_cell(*cell, record))
        }
        BoundNativeFunctionRefSlot::Mixed {
            original,
            slot,
            target,
        } => {
            let Some(value) = slot.take() else { return Ok(()); };
            if value == *original {
                return eval_release_value(context, values, value);
            }
            let Some(target) = target else {
                return eval_release_value(context, values, value);
            };
            write_back_owned_ref_target(target, value, context, values)
        }
        BoundNativeFunctionRefSlot::RawWord {
            tag,
            original,
            slot,
            target,
            force_writeback,
        } => {
            let word = slot.as_ref().snapshot().word;
            if word == *original && !force_writeback {
                return Ok(());
            }
            let Some(target) = target else {
                return Ok(());
            };
            let value = values.raw_word_value(*tag, word)?;
            write_back_owned_ref_target(target, value, context, values)
        }
        BoundNativeFunctionRefSlot::TaggedScalar { original, slot, target } => {
            let words = slot.as_ref().snapshot();
            if words == *original { return Ok(()); }
            let Some(target) = target else { return Ok(()); };
            let value = match words.high {
                EVAL_TAG_INT => values.raw_word_value(EVAL_TAG_INT, words.word)?,
                EVAL_TAG_NULL => values.null()?,
                _ => return Err(EvalStatus::RuntimeFatal),
            };
            write_back_owned_ref_target(target, value, context, values)
        }
        BoundNativeFunctionRefSlot::RawString {
            original,
            slot,
            target,
            force_writeback,
        } => {
            let words = slot.as_ref().take();
            let Some(target) = target else {
                return values.release_raw_string_words(words.word, words.high);
            };
            if words == *original && !force_writeback {
                return values.release_raw_string_words(words.word, words.high);
            }
            let value = values.raw_string_value(words.word, words.high);
            publish_native_function_ref_conversion(target, value, context, values, |values| {
                values.release_raw_string_words(words.word, words.high)
            })
        }
        BoundNativeFunctionRefSlot::OwnedRawWord {
            original,
            slot,
            target,
        } => {
            let word = slot.as_ref().take().word;
            let Some(target) = target else {
                return values.release_raw_heap_word(word);
            };
            if word == *original {
                return values.release_raw_heap_word(word);
            }
            let value = values.raw_heap_word_value(word);
            publish_native_function_ref_conversion(target, value, context, values, |values| {
                values.release_raw_heap_word(word)
            })
        }
    }
}

/// Discards a fresh box if retiring its raw staging owner failed before publication.
fn publish_native_function_ref_conversion<V: RuntimeValueOps>(
    target: &EvalReferenceTarget,
    value: Result<RuntimeCellHandle, EvalStatus>,
    context: &mut ElephcEvalContext,
    values: &mut V,
    release: impl FnOnce(&mut V) -> Result<(), EvalStatus>,
) -> Result<(), EvalStatus> {
    let value = value.map(RuntimeOwnerSlot::new);
    with_runtime_owners(&value.as_ref().ok(), || {
        let released = release(values);
        let value = value.as_ref().map_err(|status| *status)?
            .finish(released, |cell| eval_release_value(context, values, cell))?;
        write_back_owned_ref_target(target, value, context, values)
    })
}
