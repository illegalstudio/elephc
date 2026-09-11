//! Purpose:
//! Binds native callable arguments and prepares by-reference writeback metadata.
//!
//! Called from:
//! - Native instance, static, constructor, and call_user_func dispatch.
//!
//! Key details:
//! - Named, variadic, typed, and degraded by-value reference modes share one binder.
//! - A generated method or constructor bridge is called DIRECTLY, with one argument per physical
//!   parameter, so this binder both binds the PHP-visible signature and materializes the
//!   compiler-internal slots that bridge takes: the hidden actual-argument count, and the hidden
//!   surplus collector (whose first element is that same count when a visible regular carries a
//!   default). Free functions do not come through here: they reach an eval-registered descriptor
//!   INVOKER, which synthesizes those slots itself from a container of only the supplied
//!   arguments, so their registration drops the hidden slots entirely.
//! - Which physical slots are hidden comes from the EXPLICIT `NativeCallableShape` the generated
//!   bridge registers, not from a spelling convention: the shape states the PHP-visible regular
//!   count, the true required count, and whether the variadic slot is source-declared. Hidden
//!   slots additionally register an empty name, which keeps them unreachable by a named argument
//!   and invisible to Reflection even if a caller reached this code with no shape at all.

use super::*;

/// Binds native AOT callable args using the selected by-reference degradation mode.
pub(super) fn bind_native_callable_bound_args_with_mode(
    signature: Option<NativeCallableSignature>,
    mut args: Vec<EvaluatedCallArg>,
    by_ref_mode: EvalByRefBindingMode<'_>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<BoundMethodArg>, EvalStatus> {
    // The caller owns evaluated operands. Only defaults, coercions, and variadic
    // containers allocated by this binder belong to the native activation.
    for arg in &mut args { arg.value = arg.value.borrowed(); }
    let Some(signature) = signature else {
        return positional_evaluated_bound_args(None, args, by_ref_mode, context, values);
    };
    if !signature.bridge_supported() {
        return Err(EvalStatus::RuntimeFatal);
    }
    if signature.param_names().len() == signature.param_count() {
        bind_native_signature_args(&signature, args, by_ref_mode, context, values)
    } else {
        positional_evaluated_bound_args(Some(&signature), args, by_ref_mode, context, values)
    }
}

/// Binds positional-only native AOT args and validates registered by-reference slots.
pub(super) fn positional_evaluated_bound_args(
    signature: Option<&NativeCallableSignature>,
    args: Vec<EvaluatedCallArg>,
    by_ref_mode: EvalByRefBindingMode<'_>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<BoundMethodArg>, EvalStatus> {
    if args.iter().any(|arg| arg.name.is_some()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let bound_args = args
        .into_iter()
        .enumerate()
        .map(|(index, arg)| {
            let ref_target = match signature {
                Some(signature) => native_parameter_ref_target(
                    signature,
                    Some(index),
                    arg.ref_target,
                    by_ref_mode,
                    values,
                )?,
                None => None,
            };
            Ok(BoundMethodArg {
                value: arg.value,
                ref_target,
                variadic_ref_targets: Vec::new(),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if let Some(signature) = signature {
        return finish_native_argument_binding(signature, bound_args, by_ref_mode, context, values);
    }
    Ok(bound_args)
}

/// Returns only runtime cell values from bound native AOT call arguments.
pub(in crate::interpreter) fn native_bound_arg_values(
    args: &[BoundMethodArg],
) -> Vec<RuntimeCellHandle> {
    args.iter().map(|arg| arg.value).collect()
}

/// Releases only binder-owned defaults/coercions/variadics after reference writeback.
pub(in crate::interpreter) fn release_native_bound_args(
    args: &[BoundMethodArg],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let mut result = Ok(());
    for arg in args {
        for (key, _) in &arg.variadic_ref_targets {
            let released = release_expr_result(*key, context, values);
            if result.is_ok() { result = released; }
        }
        let released = release_expr_result(arg.value, context, values);
        if result.is_ok() { result = released; }
    }
    result
}

/// Preserves a native result while releasing the activation's internal argument owners.
pub(super) fn finish_native_bound_call(
    result: Result<RuntimeCellHandle, EvalStatus>,
    args: &[BoundMethodArg],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let released = release_native_bound_args(args, context, values);
    match (result, released) {
        (Err(status), _) => Err(status),
        (Ok(value), Err(status)) => {
            let _ = release_expr_result(value, context, values);
            Err(status)
        }
        (Ok(value), Ok(())) => Ok(value),
    }
}

/// Writes native AOT by-reference argument cells back to their eval caller targets.
pub(in crate::interpreter) fn write_back_native_callable_ref_args(
    bound_args: &[BoundMethodArg],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for bound_arg in bound_args {
        if let Some(target) = bound_arg.ref_target.as_ref() {
            write_back_native_ref_target(target, bound_arg.value, context, values)?;
        }
        for (key, target) in &bound_arg.variadic_ref_targets {
            let value = values.array_get(bound_arg.value, *key)?;
            let written = write_back_native_ref_target(target, value, context, values);
            let released = release_expr_result(value, context, values);
            written.and(released)?;
        }
    }
    Ok(())
}

/// Acquires a caller-scope owner before a native activation releases a coerced reference argument.
fn write_back_native_ref_target(
    target: &EvalReferenceTarget,
    value: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let EvalReferenceTarget::Variable { scope, name } = target else {
        return write_back_method_ref_target(target, value, context, values);
    };
    let Some(scope) = (unsafe { scope.as_mut() }) else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let retained = values.retain(value)?;
    let replaced = match set_owned_scope_cell(context, scope, name.clone(), retained) {
        Ok(replaced) => replaced,
        Err(status) => {
            let _ = eval_release_value(context, values, retained);
            return Err(status);
        }
    };
    for cell in replaced { eval_release_value(context, values, cell)?; }
    Ok(())
}

/// Returns the argument count PHP reports for this call, which is what `func_num_args()` sees.
///
/// PHP fills the gap before the greatest supplied regular parameter with defaults and counts that
/// slot's index plus one, then adds the surplus positional arguments. An unknown NAMED argument,
/// which only a source-declared variadic can absorb, is not counted: it is keyed by its name.
fn native_actual_argument_count(
    signature: &NativeCallableSignature,
    variadic_index: Option<usize>,
    args: &[EvaluatedCallArg],
) -> usize {
    let regular_count = signature.visible_regular_param_count();
    let mut next_positional = 0usize;
    let mut highest_regular = 0usize;
    let mut positional_surplus = 0usize;
    for arg in args {
        if let Some(name) = arg.name.as_deref() {
            if let Some(position) = native_regular_param_index(signature, variadic_index, name) {
                highest_regular = highest_regular.max(position + 1);
            }
            continue;
        }
        if next_positional < regular_count {
            highest_regular = highest_regular.max(next_positional + 1);
            next_positional += 1;
        } else {
            positional_surplus += 1;
        }
    }
    highest_regular + positional_surplus
}

/// Writes the actual argument count into the hidden collector's first element.
///
/// `values.int` hands back an owner this binder holds; the collector retains its own on insert,
/// so the local owner is released immediately and only the collector's survives into the call.
fn stage_hidden_collector_count(
    bound_args: &mut [Option<BoundMethodArg>],
    variadic_index: Option<usize>,
    actual_count: usize,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let count = values.int(actual_count as i64)?;
    let key = match values.int(0) {
        Ok(key) => key,
        Err(status) => {
            let _ = values.release(count);
            return Err(status);
        }
    };
    // `bind_native_variadic_arg` releases the key itself when there is no writeback target.
    let inserted = bind_native_variadic_arg(bound_args, variadic_index, key, count, None, values);
    let released = values.release(count);
    inserted.and(released)
}

/// Binds native AOT callable args and fills omitted defaults from metadata.
pub(super) fn bind_native_signature_args(
    signature: &NativeCallableSignature,
    args: Vec<EvaluatedCallArg>,
    by_ref_mode: EvalByRefBindingMode<'_>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<BoundMethodArg>, EvalStatus> {
    let mut bound_args = vec![None; signature.param_count()];
    let variadic_index = native_callable_variadic_index(signature);
    let regular_count = signature.visible_regular_param_count();
    let collector_needs_count = signature.collector_needs_count();
    let hidden_argc_index = signature.hidden_argc_index();
    let actual_count = native_actual_argument_count(signature, variadic_index, &args);
    let mut next_positional = 0;
    let mut next_variadic_index = 0_i64;

    let filled = (|| {
        if let Some(index) = variadic_index {
            let array = values.array_new(args.len())?;
            bound_args[index] = Some(BoundMethodArg {
                value: array,
                ref_target: None,
                variadic_ref_targets: Vec::new(),
            });
            if collector_needs_count {
                // The hidden collector's element 0 is the count; the surplus starts at 1, which
                // is exactly what the rewritten `func_get_args()` body slices off.
                stage_hidden_collector_count(&mut bound_args, variadic_index, actual_count, values)?;
                next_variadic_index = 1;
            }
        }
        if let Some(index) = hidden_argc_index {
            bound_args[index] = Some(BoundMethodArg {
                value: values.int(actual_count as i64)?,
                ref_target: None,
                variadic_ref_targets: Vec::new(),
            });
        }

        for arg in args {
            if let Some(name) = arg.name {
                bind_native_named_signature_arg(
                    signature,
                    variadic_index,
                    &mut bound_args,
                    &name,
                    arg.value,
                    arg.ref_target,
                    by_ref_mode,
                    values,
                )?;
            } else {
                bind_native_positional_signature_arg(
                    signature,
                    &mut bound_args,
                    variadic_index,
                    regular_count,
                    &mut next_positional,
                    &mut next_variadic_index,
                    arg.value,
                    arg.ref_target,
                    by_ref_mode,
                    values,
                )?;
            }
        }

        for (position, value) in bound_args.iter_mut().enumerate() {
            if Some(position) == variadic_index {
                continue;
            }
            if value.is_some() {
                continue;
            }
            if position >= regular_count {
                // Every hidden slot was materialized above. Reaching one here would mean the
                // registered frame shape disagrees with the bridge's physical parameter list,
                // and calling it with a hole would corrupt the callee's activation.
                return Err(EvalStatus::RuntimeFatal);
            }
            if position < signature.required_param_count() {
                return Err(EvalStatus::RuntimeFatal);
            }
            let Some(default) = signature.param_default(position) else {
                return Err(EvalStatus::RuntimeFatal);
            };
            *value = Some(BoundMethodArg {
                value: materialize_native_callable_default(default, context, values)?,
                ref_target: None,
                variadic_ref_targets: Vec::new(),
            });
        }

        Ok(())
    })();
    let bound_args = bound_args.into_iter().flatten().collect::<Vec<_>>();
    if let Err(status) = filled {
        let _ = release_native_bound_args(&bound_args, context, values);
        return Err(status);
    }
    finish_native_argument_binding(signature, bound_args, by_ref_mode, context, values)
}

/// Applies type coercion and reference degradation, reclaiming partial binding on errors.
fn finish_native_argument_binding(
    signature: &NativeCallableSignature,
    mut args: Vec<BoundMethodArg>,
    by_ref_mode: EvalByRefBindingMode<'_>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<BoundMethodArg>, EvalStatus> {
    let bound = apply_native_callable_bound_arg_types(signature, &mut args, context, values)
        .and_then(|()| copy_native_call_user_func_by_value_ref_args(signature, &mut args, by_ref_mode, values));
    if let Err(status) = bound {
        let _ = release_native_bound_args(&args, context, values);
        return Err(status);
    }
    Ok(args)
}

/// Applies registered native AOT parameter types after argument binding and default filling.
pub(super) fn apply_native_callable_bound_arg_types(
    signature: &NativeCallableSignature,
    bound_args: &mut [BoundMethodArg],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for (position, bound_arg) in bound_args.iter_mut().enumerate() {
        let Some(param_type) = signature.param_type(position) else {
            continue;
        };
        if signature.param_variadic(position) {
            apply_native_callable_variadic_arg_type(param_type, bound_arg, context, values)?;
        } else {
            let original = bound_arg.value;
            bound_arg.value = eval_method_parameter_value(param_type, original, context, values)?;
            if bound_arg.value != original { release_expr_result(original, context, values)?; }
        }
    }
    Ok(())
}

/// Applies one registered native variadic parameter type to each collected argument.
pub(super) fn apply_native_callable_variadic_arg_type(
    param_type: &EvalParameterType,
    bound_arg: &mut BoundMethodArg,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let len = values.array_len(bound_arg.value)?;
    for position in 0..len {
        let key = values.array_iter_key(bound_arg.value, position)?;
        let updated = (|| {
            let original = values.array_get(bound_arg.value, key)?;
            let coerced = eval_method_parameter_value(param_type, original, context, values);
            let inserted = coerced.and_then(|value| {
                let inserted = values.array_set(bound_arg.value, key, value);
                if value != original { release_expr_result(value, context, values)?; }
                inserted
            });
            let released = release_expr_result(original, context, values);
            inserted.and_then(|array| released.map(|()| array))
        })();
        let released = release_expr_result(key, context, values);
        bound_arg.value = updated?;
        released?;
    }
    Ok(())
}

/// Copies by-value degraded by-ref native method args before the generated bridge mutates them.
pub(super) fn copy_native_call_user_func_by_value_ref_args(
    signature: &NativeCallableSignature,
    bound_args: &mut [BoundMethodArg],
    by_ref_mode: EvalByRefBindingMode<'_>,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if !matches!(by_ref_mode, EvalByRefBindingMode::WarnByValue { .. }) {
        return Ok(());
    }
    let variadic_index = native_callable_variadic_index(signature);
    for (position, bound_arg) in bound_args.iter_mut().enumerate() {
        let param_index = if variadic_index.is_some_and(|index| position >= index) {
            variadic_index.ok_or(EvalStatus::RuntimeFatal)?
        } else {
            position
        };
        if !signature.param_by_ref(param_index) || bound_arg.ref_target.is_some() {
            continue;
        }
        let original = bound_arg.value;
        bound_arg.value = copy_native_call_user_func_by_value_ref_arg(original, values)?;
        if !original.is_borrowed() { values.release(original)?; }
    }
    Ok(())
}

/// Allocates a temporary runtime cell for one by-value degraded by-ref native method arg.
pub(super) fn copy_native_call_user_func_by_value_ref_arg(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let tag = values.type_tag(value)?;
    match tag {
        EVAL_TAG_INT | EVAL_TAG_FLOAT | EVAL_TAG_BOOL | EVAL_TAG_RESOURCE => {
            let word = values.raw_value_word(value)?;
            values.raw_word_value(tag, word)
        }
        EVAL_TAG_STRING => {
            let bytes = values.string_bytes(value)?;
            values.string_bytes_value(&bytes)
        }
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => values.array_clone_shallow(value),
        EVAL_TAG_OBJECT => {
            let word = values.raw_value_word(value)?;
            let retained = values.retain_raw_heap_word(word)?;
            values.raw_heap_word_value(retained)
        }
        EVAL_TAG_NULL => values.null(),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Returns the native callable variadic slot, if metadata registered one.
pub(super) fn native_callable_variadic_index(signature: &NativeCallableSignature) -> Option<usize> {
    (0..signature.param_count()).find(|index| signature.param_variadic(*index))
}

/// Binds one positional native AOT argument to a fixed slot or variadic array.
///
/// `regular_count` is the number of PHP-visible non-variadic parameters, which is NOT the
/// variadic slot's physical index whenever a hidden count parameter sits between them. Using the
/// physical index here is what let a caller's second argument land in the hidden count slot.
#[allow(clippy::too_many_arguments)]
pub(super) fn bind_native_positional_signature_arg(
    signature: &NativeCallableSignature,
    bound_args: &mut [Option<BoundMethodArg>],
    variadic_index: Option<usize>,
    regular_count: usize,
    next_positional: &mut usize,
    next_variadic_index: &mut i64,
    value: RuntimeCellHandle,
    ref_target: Option<EvalReferenceTarget>,
    by_ref_mode: EvalByRefBindingMode<'_>,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if variadic_index.is_some() && *next_positional >= regular_count {
        let key = values.int(*next_variadic_index)?;
        *next_variadic_index = next_variadic_index
            .checked_add(1)
            .ok_or(EvalStatus::RuntimeFatal)?;
        let ref_target =
            native_parameter_ref_target(signature, variadic_index, ref_target, by_ref_mode, values)?;
        return bind_native_variadic_arg(bound_args, variadic_index, key, value, ref_target, values);
    }
    let param_index = *next_positional;
    if param_index >= regular_count
        || param_index >= bound_args.len()
        || bound_args[param_index].is_some()
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    let ref_target =
        native_parameter_ref_target(signature, Some(param_index), ref_target, by_ref_mode, values)?;
    bound_args[param_index] = Some(BoundMethodArg {
        value,
        ref_target,
        variadic_ref_targets: Vec::new(),
    });
    *next_positional += 1;
    Ok(())
}

/// Binds one named native AOT argument to a fixed slot, or to a source variadic under its name.
///
/// A hidden slot can never be selected here: `native_regular_param_index` searches only the
/// registered PHP-visible regular prefix, and a hidden slot also registers an EMPTY name that no
/// PHP parameter name can equal.
pub(super) fn bind_native_named_signature_arg(
    signature: &NativeCallableSignature,
    variadic_index: Option<usize>,
    bound_args: &mut [Option<BoundMethodArg>],
    name: &str,
    value: RuntimeCellHandle,
    ref_target: Option<EvalReferenceTarget>,
    by_ref_mode: EvalByRefBindingMode<'_>,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if let Some(param_index) = native_regular_param_index(signature, variadic_index, name) {
        if bound_args[param_index].is_some() {
            return Err(EvalStatus::RuntimeFatal);
        }
        let ref_target = native_parameter_ref_target(
            signature,
            Some(param_index),
            ref_target,
            by_ref_mode,
            values,
        )?;
        bound_args[param_index] = Some(BoundMethodArg {
            value,
            ref_target,
            variadic_ref_targets: Vec::new(),
        });
        return Ok(());
    }
    // PHP absorbs an unknown named argument into a SOURCE-declared variadic, keeping its string
    // key in the collected array. A frame that only carries the hidden collector declares no
    // variadic as far as the program is concerned, so it still refuses the name.
    if signature.source_variadic_index().is_some() {
        let key = values.string(name)?;
        let ref_target =
            native_parameter_ref_target(signature, variadic_index, ref_target, by_ref_mode, values)?;
        return bind_native_variadic_arg(bound_args, variadic_index, key, value, ref_target, values);
    }
    Err(EvalStatus::RuntimeFatal)
}

/// Returns the caller writeback target required by a native by-reference parameter.
pub(super) fn native_parameter_ref_target(
    signature: &NativeCallableSignature,
    param_index: Option<usize>,
    ref_target: Option<EvalReferenceTarget>,
    by_ref_mode: EvalByRefBindingMode<'_>,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalReferenceTarget>, EvalStatus> {
    let Some(param_index) = param_index else {
        return Ok(None);
    };
    if !signature.param_by_ref(param_index) {
        return Ok(None);
    }
    if let Some(ref_target) = ref_target {
        return Ok(Some(ref_target));
    }
    match by_ref_mode {
        EvalByRefBindingMode::RequireTarget => Err(EvalStatus::RuntimeFatal),
        EvalByRefBindingMode::WarnByValue { callable_name } => {
            let param_name = native_callable_param_warning_name(signature, param_index);
            values.warning(&format!(
                "{callable_name}(): Argument #{} (${param_name}) must be passed by reference, value given",
                param_index + 1
            ))?;
            Ok(None)
        }
    }
}

/// Returns the PHP parameter name used in native method by-reference warnings.
pub(super) fn native_callable_param_warning_name(
    signature: &NativeCallableSignature,
    param_index: usize,
) -> String {
    signature
        .param_names()
        .get(param_index)
        .filter(|name| !name.is_empty())
        .cloned()
        .unwrap_or_else(|| format!("arg{}", param_index + 1))
}

/// Returns the matching non-variadic native parameter index for one named arg.
///
/// The search is bounded by the registered PHP-visible regular count, so a compiler-internal slot
/// can never be selected by name even if it somehow carried one. The empty name a hidden slot
/// registers is a second, independent guard rather than the mechanism.
pub(super) fn native_regular_param_index(
    signature: &NativeCallableSignature,
    variadic_index: Option<usize>,
    name: &str,
) -> Option<usize> {
    signature
        .param_names()
        .iter()
        .take(signature.visible_regular_param_count())
        .enumerate()
        .position(|(index, param)| Some(index) != variadic_index && param == name)
}

/// Appends one value into the native AOT variadic argument array.
pub(super) fn bind_native_variadic_arg(
    bound_args: &mut [Option<BoundMethodArg>],
    variadic_index: Option<usize>,
    key: RuntimeCellHandle,
    value: RuntimeCellHandle,
    ref_target: Option<EvalReferenceTarget>,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let index = variadic_index.ok_or(EvalStatus::RuntimeFatal)?;
    let bound = bound_args[index].as_mut().ok_or(EvalStatus::RuntimeFatal)?;
    let inserted = values.array_set(bound.value, key, value);
    if ref_target.is_none() || inserted.is_err() { values.release(key)?; }
    let array = inserted?;
    bound.value = array;
    if let Some(ref_target) = ref_target {
        bound.variadic_ref_targets.push((key, ref_target));
    }
    Ok(())
}
