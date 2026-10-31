//! Purpose:
//! Invokes boxed AOT callable descriptors that cross into the eval interpreter.
//!
//! Called from:
//! - Normalized callable dispatch for direct calls, `call_user_func*`, and eval Closure objects.
//!
//! Key details:
//! - The argument container preserves only caller-supplied values and names.
//! - Descriptor metadata selects by-reference staging without exposing compiler-hidden slots.
//! - The generated invoker remains authoritative for defaults, named-argument errors, and arity.

use super::*;

/// Invokes one boxed descriptor through its protected uniform adapter.
pub(super) fn eval_native_descriptor_with_call_args(
    callback: RuntimeCellHandle,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let signature = values
        .native_callable_signature(callback)?
        .ok_or(EvalStatus::UnsupportedConstruct)?;
    let mut staged = stage_native_descriptor_args(&signature, evaluated_args, context, values)?;
    let _native_context = crate::context::push_native_frame_context(context);
    let arg_array = match build_native_function_arg_array(&staged, values) {
        Ok(arguments) => arguments,
        Err(status) => {
            let _ = cleanup_native_function_ref_args(&staged, values);
            return finish_eval_argument_values(Err(status), staged.values, context, values);
        }
    };

    let result = values.invoke_native_callable(callback, arg_array, -1);
    let writeback = write_back_native_function_ref_args(&staged, context, values);
    let result = match (result, writeback) {
        (Err(status), _) => Err(status),
        (Ok(value), Err(status)) => {
            let _ = eval_release_value(context, values, value);
            Err(status)
        }
        (Ok(value), Ok(())) => Ok(value),
    };
    let result = finish_eval_argument_values(result, [arg_array], context, values);
    finish_eval_argument_values(result, staged.values.drain(..), context, values)
}

/// Transfers by-value inputs and replaces by-reference inputs with invoker marker cells.
fn stage_native_descriptor_args(
    signature: &NativeCallableSignatureMetadata,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<BoundNativeFunctionArgs, EvalStatus> {
    let mut staged = BoundNativeFunctionArgs {
        values: Vec::with_capacity(evaluated_args.len()),
        ref_slots: Vec::new(),
        named_keys: Vec::with_capacity(evaluated_args.len()),
    };
    let mut binding = DescriptorBindingState::new(signature.regular_param_count);
    let mut pending = std::collections::VecDeque::from(evaluated_args);
    while let Some(argument) = pending.front().cloned() {
        let parameter_index = binding.parameter_index(signature, argument.name.as_deref());
        let parameter = parameter_index.and_then(|index| signature.parameters.get(index));
        if !parameter.is_some_and(|parameter| parameter.by_ref) {
            let argument = pending.pop_front().expect("the staged argument is still pending");
            staged.named_keys.push(argument.name);
            staged.values.push(argument.value);
            continue;
        }

        let parameter = parameter.expect("a by-reference descriptor parameter has metadata");
        let target = match argument.ref_target.clone() {
            Some(target) => Some(target),
            None => {
                if let Err(status) = values.warning(&format!(
                    "{}(): Argument #{} (${}) must be passed by reference, value given",
                    signature.display_name,
                    parameter_index.unwrap_or(0) + 1,
                    if parameter.name.is_empty() { "arg" } else { &parameter.name },
                )) {
                    return abort_native_descriptor_staging(status, staged, pending, context, values);
                }
                None
            }
        };
        let parameter_type = descriptor_parameter_type(parameter.type_tag);
        let coerced = match parameter_type.as_ref() {
            Some(parameter_type) => match eval_method_parameter_value(
                parameter_type,
                argument.value,
                context,
                values,
            ) {
                Ok(value) => value,
                Err(status) => {
                    return abort_native_descriptor_staging(
                        status, staged, pending, context, values,
                    );
                }
            },
            None => argument.value,
        };
        let coerced_is_temporary = coerced != argument.value;
        let (marker, mut slot) = match stage_native_function_ref_arg(
            parameter_type.as_ref(),
            false,
            coerced,
            target,
            values,
        ) {
            Ok(staged_ref) => staged_ref,
            Err(status) => {
                if coerced_is_temporary {
                    let _ = release_expr_result(coerced, context, values);
                }
                return abort_native_descriptor_staging(status, staged, pending, context, values);
            }
        };
        mark_descriptor_coercion_writeback(&mut slot, coerced_is_temporary);
        if coerced_is_temporary {
            if let Err(status) = release_expr_result(coerced, context, values) {
                let partial = BoundNativeFunctionArgs {
                    values: vec![marker],
                    ref_slots: vec![slot],
                    named_keys: vec![None],
                };
                let _ = cleanup_native_function_ref_args(&partial, values);
                let _ = release_expr_result(marker, context, values);
                return abort_native_descriptor_staging(status, staged, pending, context, values);
            }
        }
        let argument = pending.pop_front().expect("the staged argument is still pending");
        staged.named_keys.push(argument.name);
        staged.values.push(marker);
        staged.ref_slots.push(slot);
        if let Err(status) = release_expr_result(argument.value, context, values) {
            return abort_native_descriptor_staging(status, staged, pending, context, values);
        }
    }
    Ok(staged)
}

/// Releases partial marker state and every caller argument whose ownership was not transferred.
fn abort_native_descriptor_staging(
    status: EvalStatus,
    staged: BoundNativeFunctionArgs,
    pending: std::collections::VecDeque<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<BoundNativeFunctionArgs, EvalStatus> {
    let _ = cleanup_native_function_ref_args(&staged, values);
    for value in staged
        .values
        .into_iter()
        .chain(pending.into_iter().map(|argument| argument.value))
    {
        let _ = release_expr_result(value, context, values);
    }
    Err(status)
}

/// Maps supplied argument order and names onto caller-visible descriptor parameters.
struct DescriptorBindingState {
    next_positional: usize,
    saw_named: bool,
    assigned_regular: Vec<bool>,
}

impl DescriptorBindingState {
    /// Starts argument matching with no regular parameter assigned.
    fn new(regular_param_count: usize) -> Self {
        Self {
            next_positional: 0,
            saw_named: false,
            assigned_regular: vec![false; regular_param_count],
        }
    }

    /// Returns the parameter whose reference representation this supplied argument must use.
    fn parameter_index(
        &mut self,
        signature: &NativeCallableSignatureMetadata,
        name: Option<&str>,
    ) -> Option<usize> {
        if let Some(name) = name {
            self.saw_named = true;
            if let Some(index) = signature
                .parameters
                .iter()
                .take(signature.regular_param_count)
                .position(|parameter| parameter.name == name)
            {
                if self.assigned_regular.get(index).copied().unwrap_or(true) {
                    return None;
                }
                self.assigned_regular[index] = true;
                return Some(index);
            }
            return signature.variadic_index;
        }
        if self.saw_named {
            return None;
        }
        if self.next_positional < signature.regular_param_count {
            let index = self.next_positional;
            self.next_positional += 1;
            self.assigned_regular[index] = true;
            return Some(index);
        }
        signature.variadic_index
    }
}

/// Reconstructs the broad PHP type needed for safe reference coercion and staging.
fn descriptor_parameter_type(type_tag: u64) -> Option<EvalParameterType> {
    let variant = match type_tag {
        0 => EvalParameterTypeVariant::Int,
        1 => EvalParameterTypeVariant::String,
        2 => EvalParameterTypeVariant::Float,
        3 => EvalParameterTypeVariant::Bool,
        4 | 5 => EvalParameterTypeVariant::Array,
        6 => EvalParameterTypeVariant::Object,
        7 => EvalParameterTypeVariant::Mixed,
        10 => EvalParameterTypeVariant::Callable,
        12 => EvalParameterTypeVariant::Iterable,
        _ => return None,
    };
    Some(EvalParameterType::new(vec![variant], false))
}

/// Forces publication when entering a typed reference required a scalar conversion.
fn mark_descriptor_coercion_writeback(
    slot: &mut BoundNativeFunctionRefSlot,
    force_writeback: bool,
) {
    if !force_writeback {
        return;
    }
    match slot {
        BoundNativeFunctionRefSlot::RawWord {
            force_writeback: force,
            ..
        }
        | BoundNativeFunctionRefSlot::RawString {
            force_writeback: force,
            ..
        } => *force = true,
        BoundNativeFunctionRefSlot::Mixed { .. }
        | BoundNativeFunctionRefSlot::TaggedScalar { .. }
        | BoundNativeFunctionRefSlot::OwnedRawWord { .. } => {}
    }
}
