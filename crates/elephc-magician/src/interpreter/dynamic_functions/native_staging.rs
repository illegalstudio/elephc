//! Purpose:
//! Stages native function reference slots and records their marker-cell owners.
//!
//! Called from:
//! - Native function argument binding after defaults and scalar type coercion.
//!
//! Key details:
//! - Slots use stable Rust allocations through native invocation and reference writeback.
//! - The binder retires partial staging on failure and markers after writeback.

use super::*;

/// Converts bound values into descriptor-invoker arguments, staging by-reference slots.
pub(super) fn stage_native_function_invoker_args(
    function: &NativeFunction,
    variadic_index: Option<usize>,
    bound_args: Vec<BoundMethodArg>,
    by_ref_mode: EvalByRefBindingMode<'_>,
    values: &mut impl RuntimeValueOps,
    staged: &mut BoundNativeFunctionArgs,
) -> Result<(), EvalStatus> {
    let invoker_values = &mut staged.values;
    let ref_slots = &mut staged.ref_slots;
    for (position, bound_arg) in bound_args.into_iter().enumerate() {
        let param_index = if variadic_index.is_some_and(|index| position >= index) {
            variadic_index.ok_or(EvalStatus::RuntimeFatal)?
        } else {
            position
        };
        if !function.param_by_ref(param_index) {
            invoker_values.push(bound_arg.value);
            continue;
        }
        let target = match (bound_arg.ref_target, by_ref_mode) {
            (Some(target), _) => Some(target),
            (None, EvalByRefBindingMode::WarnByValue { .. }) => None,
            (None, EvalByRefBindingMode::RequireTarget) => return Err(EvalStatus::RuntimeFatal),
        };
        if let Some(raw_ref_kind) = native_function_raw_ref_kind(function.param_type(param_index)) {
            match raw_ref_kind {
                NativeFunctionRawRefKind::Scalar { tag } => {
                    let original = values.raw_value_word(bound_arg.value)?;
                    let mut slot = Box::new(original);
                    let marker =
                        values.invoker_raw_ref_cell(slot.as_mut() as *mut u64 as *mut c_void, tag)?;
                    staged.owners.push(marker);
                    invoker_values.push(marker);
                    ref_slots.push(BoundNativeFunctionRefSlot::RawWord {
                        tag,
                        original,
                        slot,
                        target,
                    });
                }
                NativeFunctionRawRefKind::String => {
                    let original_ptr = values.raw_value_word(bound_arg.value)?;
                    let original_len = values.raw_value_high_word(bound_arg.value)?;
                    let retained = values.retain_raw_string_words(original_ptr, original_len)?;
                    let mut slot = Box::new([retained.0, retained.1]);
                    let marker = match values.invoker_raw_ref_cell(
                        slot.as_mut() as *mut [u64; 2] as *mut c_void,
                        EVAL_TAG_STRING,
                    ) {
                        Ok(marker) => marker,
                        Err(status) => {
                            let _ = values.release_raw_string_words(retained.0, retained.1);
                            return Err(status);
                        }
                    };
                    staged.owners.push(marker);
                    invoker_values.push(marker);
                    ref_slots.push(BoundNativeFunctionRefSlot::RawString {
                        original: [retained.0, retained.1],
                        slot,
                        target,
                    });
                }
                NativeFunctionRawRefKind::OwnedHeap => {
                    let source_tag = values.type_tag(bound_arg.value)?;
                    let original = values.raw_value_word(bound_arg.value)?;
                    let retained = values.retain_raw_heap_word(original)?;
                    let mut slot = Box::new(retained);
                    let marker = match values.invoker_raw_ref_cell(
                        slot.as_mut() as *mut u64 as *mut c_void,
                        source_tag,
                    ) {
                        Ok(marker) => marker,
                        Err(status) => {
                            let _ = values.release_raw_heap_word(retained);
                            return Err(status);
                        }
                    };
                    staged.owners.push(marker);
                    invoker_values.push(marker);
                    ref_slots.push(BoundNativeFunctionRefSlot::OwnedRawWord {
                        original,
                        slot,
                        target,
                    });
                }
            }
            continue;
        }
        let original = bound_arg.value;
        let retained = values.retain(original)?;
        let mut slot = Box::new(retained);
        let marker = match values.invoker_ref_cell(slot.as_mut() as *mut RuntimeCellHandle) {
            Ok(marker) => marker,
            Err(status) => {
                values.release(retained)?;
                return Err(status);
            }
        };
        staged.owners.push(marker);
        invoker_values.push(marker);
        ref_slots.push(BoundNativeFunctionRefSlot::Mixed {
            original,
            slot,
            target,
        });
    }
    Ok(())
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
    if param_type.allows_null()
        || param_type.is_intersection()
        || param_type.variants().len() != 1
    {
        return None;
    }
    match param_type.variants().first()? {
        EvalParameterTypeVariant::Array
        | EvalParameterTypeVariant::Class(_)
        | EvalParameterTypeVariant::Iterable
        | EvalParameterTypeVariant::Object => Some(NativeFunctionRawRefKind::OwnedHeap),
        EvalParameterTypeVariant::Bool => Some(NativeFunctionRawRefKind::Scalar { tag: EVAL_TAG_BOOL }),
        EvalParameterTypeVariant::Float => {
            Some(NativeFunctionRawRefKind::Scalar { tag: EVAL_TAG_FLOAT })
        }
        EvalParameterTypeVariant::Int => Some(NativeFunctionRawRefKind::Scalar { tag: EVAL_TAG_INT }),
        EvalParameterTypeVariant::String => Some(NativeFunctionRawRefKind::String),
        _ => None,
    }
}
