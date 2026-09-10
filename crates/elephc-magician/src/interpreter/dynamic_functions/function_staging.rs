//! Purpose:
//! Prepares descriptor-invoker cells and reference slots for native function calls.
//!
//! Called from:
//! - Native function binding after default materialization and parameter coercion.
//!
//! Key details:
//! - Each staged reference owns its current payload independently of the caller.
//! - Partial staging releases markers, payload leases, and transferred value arguments.
//! - PHP array declarations use boxed slots because native callees can change the array layout.

use super::*;

/// Transfers value arguments or prepares reference markers, rolling back partial staging on errors.
pub(super) fn stage_native_function_invoker_args(
    function: &NativeFunction,
    variadic_index: Option<usize>,
    bound_args: &mut [BoundMethodArg],
    by_ref_mode: EvalByRefBindingMode<'_>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<BoundNativeFunctionArgs, EvalStatus> {
    let mut staged = BoundNativeFunctionArgs {
        values: Vec::with_capacity(bound_args.len()),
        ref_slots: Vec::new(),
    };
    let prepared = (|| {
        for (position, bound) in bound_args.iter_mut().enumerate() {
            let param_index = variadic_index.map_or(position, |index| position.min(index));
            let original = bound.value;
            if !function.param_by_ref(param_index) {
                staged.values.push(original);
                bound.value = original.borrowed();
                continue;
            }
            let target = match (&bound.ref_target, by_ref_mode) {
                (Some(target), _) => Some(target.clone()),
                (None, EvalByRefBindingMode::WarnByValue { .. }) => None,
                (None, EvalByRefBindingMode::RequireTarget) => return Err(EvalStatus::RuntimeFatal),
            };
            let (marker, slot) = stage_native_function_ref_arg(
                function.param_type(param_index), original, target, values,
            )?;
            staged.values.push(marker);
            staged.ref_slots.push(slot);
            bound.value = original.borrowed();
            release_expr_result(original, context, values)?;
        }
        Ok(())
    })();
    if let Err(status) = prepared {
        let _ = cleanup_native_function_ref_args(&staged, values);
        for value in staged.values {
            let _ = release_expr_result(value, context, values);
        }
        return Err(status);
    }
    Ok(staged)
}

/// Creates one reference marker and retires an acquired payload if marker allocation fails.
fn stage_native_function_ref_arg(
    param_type: Option<&EvalParameterType>,
    original: RuntimeCellHandle,
    target: Option<EvalReferenceTarget>,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, BoundNativeFunctionRefSlot), EvalStatus> {
    match native_function_raw_ref_kind(param_type) {
        Some(NativeFunctionRawRefKind::Scalar { tag }) => {
            let original = values.raw_value_word(original)?;
            let mut slot = Box::new(original);
            let marker = values.invoker_raw_ref_cell((slot.as_mut() as *mut u64).cast(), tag)?;
            Ok((marker, BoundNativeFunctionRefSlot::RawWord { tag, original, slot, target }))
        }
        Some(NativeFunctionRawRefKind::String) => {
            let ptr = values.raw_value_word(original)?;
            let len = values.raw_value_high_word(original)?;
            let retained = values.retain_raw_string_words(ptr, len)?;
            let mut slot = Box::new([retained.0, retained.1]);
            let marker = values.invoker_raw_ref_cell(
                (slot.as_mut() as *mut [u64; 2]).cast(), EVAL_TAG_STRING,
            );
            let marker = match marker {
                Ok(marker) => marker,
                Err(status) => {
                    let _ = values.release_raw_string_words(retained.0, retained.1);
                    return Err(status);
                }
            };
            Ok((marker, BoundNativeFunctionRefSlot::RawString {
                original: [retained.0, retained.1], slot, target,
            }))
        }
        Some(NativeFunctionRawRefKind::OwnedHeap) => {
            let tag = values.type_tag(original)?;
            let original = values.raw_value_word(original)?;
            let retained = values.retain_raw_heap_word(original)?;
            let mut slot = Box::new(retained);
            let marker = values.invoker_raw_ref_cell((slot.as_mut() as *mut u64).cast(), tag);
            let marker = match marker {
                Ok(marker) => marker,
                Err(status) => {
                    let _ = values.release_raw_heap_word(retained);
                    return Err(status);
                }
            };
            Ok((marker, BoundNativeFunctionRefSlot::OwnedRawWord { original, slot, target }))
        }
        None => {
            let retained = values.retain(original)?;
            let mut slot = Box::new(retained.as_ptr());
            let marker = match values.invoker_ref_cell(slot.as_mut()) {
                Ok(marker) => marker,
                Err(status) => {
                    let _ = values.release(retained);
                    return Err(status);
                }
            };
            Ok((marker, BoundNativeFunctionRefSlot::Mixed { original, slot, target }))
        }
    }
}

/// Describes native function by-reference parameters that can use typed raw slots.
enum NativeFunctionRawRefKind {
    Scalar { tag: u64 },
    String,
    OwnedHeap,
}

/// Returns the raw-slot strategy for one supported by-reference parameter.
fn native_function_raw_ref_kind(param_type: Option<&EvalParameterType>) -> Option<NativeFunctionRawRefKind> {
    let param_type = param_type?;
    if param_type.allows_null() || param_type.is_intersection() || param_type.variants().len() != 1 {
        return None;
    }
    match param_type.variants().first()? {
        EvalParameterTypeVariant::Class(_)
        | EvalParameterTypeVariant::Iterable
        | EvalParameterTypeVariant::Object => Some(NativeFunctionRawRefKind::OwnedHeap),
        EvalParameterTypeVariant::Array => None,
        EvalParameterTypeVariant::Bool => Some(NativeFunctionRawRefKind::Scalar { tag: EVAL_TAG_BOOL }),
        EvalParameterTypeVariant::Float => Some(NativeFunctionRawRefKind::Scalar { tag: EVAL_TAG_FLOAT }),
        EvalParameterTypeVariant::Int => Some(NativeFunctionRawRefKind::Scalar { tag: EVAL_TAG_INT }),
        EvalParameterTypeVariant::String => Some(NativeFunctionRawRefKind::String),
        _ => None,
    }
}
