//! Purpose:
//! Owns shared builtin arguments across direct calls, callback arrays, and evaluated callbacks.
//!
//! Called from:
//! - Registry binding and core callback dispatch before invoking shared runtime operations.
//!
//! Key details:
//! - The shared parameter contract selects independent values or persistent reference wrappers.
//! - Direct callback-array syntax releases its source array before invocation; borrowed arrays keep their caller owner.
//! - Argument owners, reference pins, and inserted defaults are cleaned up on every exit.

use super::*;
use super::binding::bind_builtin_arguments;
use crate::interpreter::dynamic_functions::builtin_arguments;
use elephc_builtin_contract::BuiltinContract;

/// Evaluates call_user_func inputs by value before adapting required references into temporary cells.
pub(in crate::interpreter) fn eval_builtin_call_by_value(
    name: &str, args: &[EvalCallArg], context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_owned_builtin_call(name, args, true, context, scope, values)
}

/// Captures direct source arguments according to their passing modes, or by value for a callback wrapper.
pub(in crate::interpreter) fn eval_owned_builtin_call(
    name: &str, args: &[EvalCallArg], callback_by_value: bool, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    with_owned_builtin_arguments(name, callback_by_value, Some(scope), context, values,
        |contract, lexical_scope, context, values, owners, evaluated| {
            let lexical_scope = lexical_scope.ok_or(EvalStatus::RuntimeFatal)?;
            eval_owned_call_arg_values(args, context, lexical_scope, values, owners, evaluated,
                (!callback_by_value).then_some(contract))
        })
}

/// Evaluates a direct callback-array expression and consumes its temporary array before invoking the callee.
pub(in crate::interpreter) fn eval_builtin_call_array_expr(
    name: &str, array: &EvalExpr, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    with_owned_builtin_arguments(name, true, Some(scope), context, values,
        |contract, lexical_scope, context, values, owners, evaluated| {
            let lexical_scope = lexical_scope.ok_or(EvalStatus::RuntimeFatal)?;
            let array = eval_owned_expr(array, context, lexical_scope, values)?;
            owners.push(array);
            capture_array(contract, array, context, values, owners, evaluated)
        })
}

/// Expands an independently owned snapshot without consuming the caller's borrowed argument array.
pub(in crate::interpreter) fn eval_builtin_call_array_value(
    name: &str, array: RuntimeCellHandle, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    with_owned_builtin_arguments(name, true, None, context, values,
        |contract, _, context, values, owners, evaluated| {
            let copy = values.copy_value(array)?;
            context.copy_array_metadata(array, copy);
            owners.push(copy);
            capture_array(contract, copy, context, values, owners, evaluated)
        })
}

/// Copies already evaluated callback inputs, optionally preserving explicit persistent output references.
pub(in crate::interpreter) fn eval_builtin_callback_with_arguments(
    name: &str, arguments: Vec<EvaluatedCallArg>, preserve_references: bool,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    with_owned_builtin_arguments(name, true, None, context, values,
        |contract, _, context, values, owners, evaluated| {
            for (index, argument) in arguments.into_iter().enumerate() {
                let value = values.retain(argument.value)?;
                owners.push(value);
                evaluated.push(EvaluatedCallArg { value, ..argument });
                capture_callback_argument(contract, &mut evaluated[index], index, preserve_references,
                    context, values, owners)?;
            }
            Ok(())
        })
}

/// Captures array elements in key order, then releases the source array while argument owners remain pinned.
fn capture_array(
    contract: &BuiltinContract, array: RuntimeCellHandle, context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps, owners: &mut Vec<RuntimeCellHandle>, evaluated: &mut Vec<EvaluatedCallArg>,
) -> Result<(), EvalStatus> {
    if !values.is_array_like(array)? { return Err(EvalStatus::RuntimeFatal); }
    append_unpacked_call_arg_values_with_owners(array, evaluated, &mut false, context, values, Some(owners))?;
    for (index, argument) in evaluated.iter_mut().enumerate() {
        capture_callback_argument(contract, argument, index, true, context, values, owners)?;
    }
    let index = owners.iter().position(|owner| *owner == array).expect("owned callback array");
    owners.remove(index);
    context.clear_array_metadata(array);
    eval_release_value(context, values, array)
}

/// Retains an explicit output wrapper or detaches an ordinary input from the referenced caller value.
fn capture_callback_argument(
    contract: &BuiltinContract, argument: &mut EvaluatedCallArg, position: usize, preserve_references: bool,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps, owners: &mut Vec<RuntimeCellHandle>,
) -> Result<(), EvalStatus> {
    if preserve_references && builtin_arguments::by_reference(Some(contract), argument.name.as_deref(), position) {
        if argument.ref_target.is_some() && !values.is_reference(argument.value)? {
            return Err(EvalStatus::UnsupportedConstruct);
        }
        return Ok(());
    }
    let old = argument.value;
    let copy = values.copy_value(old)?;
    context.copy_array_metadata(old, copy);
    argument.value = copy;
    argument.ref_target = None;
    owners.push(copy);
    let index = owners.iter().position(|owner| *owner == old).expect("owned callback input");
    owners.remove(index);
    eval_release_value(context, values, old)
}

/// Shares binding, optional callback-reference adaptation, invocation, and failure-safe owner cleanup.
fn with_owned_builtin_arguments<V: RuntimeValueOps>(
    name: &str, callback: bool, mut lexical_scope: Option<&mut ElephcEvalScope>,
    context: &mut ElephcEvalContext, values: &mut V,
    evaluate: impl FnOnce(&BuiltinContract, Option<&mut ElephcEvalScope>, &mut ElephcEvalContext, &mut V,
        &mut Vec<RuntimeCellHandle>, &mut Vec<EvaluatedCallArg>) -> Result<(), EvalStatus>,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut owners = Vec::new();
    let mut evaluated = Vec::new();
    let mut ordered = Vec::new();
    let mut callback_reference_metadata = Vec::new();
    let result = (|| {
        let contract = elephc_builtin_contract::lookup(name).ok_or(EvalStatus::UnsupportedConstruct)?;
        evaluate(
            contract,
            lexical_scope.as_deref_mut(),
            context,
            values,
            &mut owners,
            &mut evaluated,
        )?;
        ordered = bind_builtin_arguments(name, evaluated.clone(), values, Some(&mut owners))?;
        if callback {
            adapt_callback_references(
                contract,
                &mut ordered,
                &mut owners,
                &mut callback_reference_metadata,
                context,
                values,
            )?;
        }
        let borrowed = ordered
            .iter()
            .copied()
            .map(RuntimeCellHandle::borrowed)
            .collect::<Vec<_>>();
        let result = eval_builtin_with_values_from_scope(
            name,
            &borrowed,
            lexical_scope.as_deref(),
            context,
            values,
        )?.ok_or(EvalStatus::UnsupportedConstruct)?;
        promote_borrowed_result(result, values)
    })();
    let preserved = result.as_ref().ok().copied();
    for reference in callback_reference_metadata {
        if preserved.map_or(true, |value| value.as_ptr() != reference.as_ptr()) {
            context.clear_array_metadata(reference);
        }
    }
    if ordered.is_empty() {
        // Partial evaluation still destroys captured PHP arguments in parameter order.
        let names = eval_builtin_param_names(name).unwrap_or(&[]);
        let mut partial = evaluated.iter().enumerate().collect::<Vec<_>>();
        partial.sort_by_key(|(position, argument)| argument.name.as_deref()
            .and_then(|name| names.iter().position(|parameter| *parameter == name)).unwrap_or(*position));
        ordered.extend(partial.into_iter().map(|(_, argument)| argument.value));
    }
    let mut releases = Vec::new();
    for value in ordered {
        if let Some(index) = owners.iter().position(|owner| *owner == value) {
            releases.push(owners.remove(index));
        }
    }
    releases.extend(owners);
    let mut cleanup = Ok(());
    for value in releases {
        if let Err(status) = eval_release_value(context, values, value) { cleanup = Err(status); }
    }
    match (result, cleanup) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(status), _) => Err(status),
        (Ok(value), Err(status)) => {
            let _ = eval_release_value(context, values, value);
            Err(status)
        }
    }
}

/// Warns for callback values sent to reference parameters and transfers their owners into temporary wrappers.
fn adapt_callback_references(
    contract: &BuiltinContract, arguments: &mut [RuntimeCellHandle], owners: &mut Vec<RuntimeCellHandle>,
    callback_reference_metadata: &mut Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for (index, (argument, parameter)) in arguments.iter_mut().zip(contract.params).enumerate() {
        if !parameter.by_ref || values.is_reference(*argument)? { continue; }
        values.warning(&format!("Warning: {}(): Argument #{} (${}) must be passed by reference, value given\n",
            contract.name, index + 1, parameter.name))?;
        let old = *argument;
        let carries_array_metadata = values.is_array_like(old)?;
        let reference = values.reference_new(old)?;
        if carries_array_metadata {
            context.copy_array_metadata(old, reference);
            callback_reference_metadata.push(reference);
        }
        owners.push(reference);
        *argument = reference;
        let position = owners.iter().position(|owner| *owner == old).expect("owned callback argument");
        owners.remove(position);
        // The wrapper becomes the sole call owner so initialization can destroy a temporary old value.
        eval_release_value(context, values, old)?;
    }
    Ok(())
}
