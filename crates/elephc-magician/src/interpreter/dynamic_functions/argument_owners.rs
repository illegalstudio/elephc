//! Purpose:
//! Owns temporary property receivers while eval call arguments borrow their reference targets.
//!
//! Called from:
//! - The shared eval function, method and constructor call-argument boundary.
//!
//! Key details:
//! - Reference metadata remains borrowed and survives until argument writeback completes.
//! - Failed argument evaluation retires both completed values and receiver owners.

use super::*;

/// Acquires each argument before later argument side effects can replace its source storage.
pub(in crate::interpreter) fn eval_owned_call_arg_values(
    args: &[EvalCallArg],
    modes: &CallArgumentModes,
    context: &mut ElephcEvalContext,
    caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(Vec<EvaluatedCallArg>, Vec<RuntimeCellHandle>), EvalStatus> {
    let mut evaluated_args = Vec::with_capacity(args.len());
    let mut receiver_owners = Vec::new();
    let mut saw_named = false;

    let evaluated = (|| {
        for arg in args {
            if arg.is_spread() {
                if saw_named {
                    return Err(EvalStatus::RuntimeFatal);
                }
                let spread = eval_expr(arg.value(), context, caller_scope, values)?;
                let unpacked = (|| {
                    if !values.is_array_like(spread)? {
                        return Err(EvalStatus::RuntimeFatal);
                    }
                    let first_unpacked = evaluated_args.len();
                    append_unpacked_call_arg_values(
                        spread, &mut evaluated_args, &mut saw_named, context, values,
                    )?;
                    for argument in &mut evaluated_args[first_unpacked..] {
                        if argument.value.is_borrowed() {
                            argument.value = values.retain(argument.value)?;
                        }
                    }
                    Ok(())
                })();
                let released = release_expr_result(spread, context, values);
                unpacked.and(released)?;
                continue;
            }

            if let Some(name) = arg.name() {
                saw_named = true;
                let (value, ref_target) = eval_call_arg_value_with_mode(arg.value(),
                    modes.by_reference(Some(name), evaluated_args.len()), context, caller_scope, values)?;
                let (value, mut ref_target) = own_temporary_reference(value, ref_target, context, values)?;
                capture_argument_receiver_owners(ref_target.as_mut(), &mut receiver_owners);
                let value = if value.is_borrowed() {
                    values.retain(value)?
                } else {
                    value
                };
                evaluated_args.push(EvaluatedCallArg {
                    name: Some(name.to_string()),
                    value,
                    ref_target,
                });
                continue;
            }

            if saw_named {
                return Err(EvalStatus::RuntimeFatal);
            }
            let (value, ref_target) = eval_call_arg_value_with_mode(arg.value(),
                modes.by_reference(None, evaluated_args.len()), context, caller_scope, values)?;
            let (value, mut ref_target) = own_temporary_reference(value, ref_target, context, values)?;
            capture_argument_receiver_owners(ref_target.as_mut(), &mut receiver_owners);
            let value = if value.is_borrowed() {
                values.retain(value)?
            } else {
                value
            };
            evaluated_args.push(EvaluatedCallArg {
                name: None,
                value,
                ref_target,
            });
        }
        Ok(())
    })();
    if let Err(status) = evaluated {
        for argument in evaluated_args {
            let _ = release_expr_result(argument.value, context, values);
        }
        for owner in receiver_owners.into_iter().rev() {
            let _ = release_expr_result(owner, context, values);
        }
        return Err(status);
    }
    Ok((evaluated_args, receiver_owners))
}

/// Resolves a reference receiver once and retires metadata belonging only to an intermediate hop.
pub(super) fn eval_call_argument_receiver(
    expr: &EvalExpr, by_reference: bool, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !by_reference { return eval_expr(expr, context, scope, values); }
    let (value, mut target) = eval_call_arg_value_with_mode(expr, true, context, scope, values)?;
    let value = promote_borrowed_result(value, values);
    let mut owners = Vec::new();
    capture_abandoned_reference_owners(target.as_mut(), &mut owners);
    finish_argument_reference_owners(value.map(|value| (value, None)), owners, context, values)
        .map(|(value, _)| value)
}

/// Moves independently owned property receivers out of borrowed call reference metadata.
pub(super) fn capture_argument_receiver_owners(
    target: Option<&mut EvalReferenceTarget>, owners: &mut Vec<RuntimeCellHandle>,
) {
    match target {
        Some(EvalReferenceTarget::ObjectProperty { object, .. }) if !object.is_borrowed() => {
            owners.push(*object);
            *object = object.borrowed();
        }
        Some(EvalReferenceTarget::NestedArrayElement { array_target, .. }) => {
            capture_argument_receiver_owners(Some(array_target.as_mut()), owners);
        }
        _ => {}
    }
}

/// Retires keys as well as receivers when reference metadata will never reach a consumer.
pub(super) fn capture_abandoned_reference_owners(
    target: Option<&mut EvalReferenceTarget>, owners: &mut Vec<RuntimeCellHandle>,
) {
    match target {
        Some(EvalReferenceTarget::ArrayElement { index, .. }) => {
            if !index.is_borrowed() { owners.push(*index); *index = index.borrowed(); }
        }
        Some(EvalReferenceTarget::NestedArrayElement { array_target, index }) => {
            capture_abandoned_reference_owners(Some(array_target.as_mut()), owners);
            if !index.is_borrowed() { owners.push(*index); *index = index.borrowed(); }
        }
        target => capture_argument_receiver_owners(target, owners),
    }
}

/// Retires intermediate lvalue operands without losing an owned result or abandoned target on failure.
pub(super) fn finish_argument_reference_owners(
    result: Result<(RuntimeCellHandle, Option<EvalReferenceTarget>), EvalStatus>,
    owners: Vec<RuntimeCellHandle>, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, Option<EvalReferenceTarget>), EvalStatus> {
    let mut cleanup = Ok(());
    for owner in owners.into_iter().rev() {
        let released = release_expr_result(owner, context, values);
        if cleanup.is_ok() { cleanup = released; }
    }
    match (result, cleanup) {
        (Err(status), _) => Err(status),
        (Ok((value, mut target)), Err(status)) => {
            let _ = release_expr_result(value, context, values);
            let mut abandoned = Vec::new();
            capture_abandoned_reference_owners(target.as_mut(), &mut abandoned);
            for owner in abandoned.into_iter().rev() {
                let _ = release_expr_result(owner, context, values);
            }
            Err(status)
        }
        (Ok(result), Ok(())) => Ok(result),
    }
}

/// Promotes a temporary XML selection to a reference that may escape without parent writeback.
fn own_temporary_reference(
    value: RuntimeCellHandle, target: Option<EvalReferenceTarget>,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, Option<EvalReferenceTarget>), EvalStatus> {
    if matches!(&target, Some(EvalReferenceTarget::Cell { cell }) if *cell == value)
        && values.supports_persistent_references()
    {
        match values.is_reference(value) {
            Ok(true) => return Ok((value, target)),
            Ok(false) => {}
            Err(status) => { let _ = release_expr_result(value, context, values); return Err(status); }
        }
        let result = values.reference_new(value);
        let reference = finish_synthesized_method_arguments(result, vec![value], context, values)?;
        return Ok((reference, Some(EvalReferenceTarget::Cell { cell: reference.borrowed() })));
    }
    Ok((value, target))
}
