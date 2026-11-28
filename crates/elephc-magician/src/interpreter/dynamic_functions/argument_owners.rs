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
use crate::runtime_hooks::owner_frames::{
    RuntimeOwnerSlot, RuntimeOwners, with_mutable_runtime_owners, with_runtime_owners,
};
use crate::value::RuntimeCell;
use elephc_memoir::runtime_owners::{
    CleanupErrorPolicy, ReleaseOrder, RuntimeOwnerBatch,
};
use std::cell::RefCell;

/// Acquires each argument before later argument side effects can replace its source storage.
pub(in crate::interpreter) fn eval_owned_call_arg_values(
    args: &[EvalCallArg],
    modes: &CallArgumentModes,
    context: &mut ElephcEvalContext,
    caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(Vec<EvaluatedCallArg>, Vec<RuntimeCellHandle>), EvalStatus> {
    let mut inventory = (
        Vec::with_capacity(args.len()),
        RuntimeOwnerBatch::empty(),
    );
    let mut saw_named = false;

    let evaluated = with_mutable_runtime_owners(&mut inventory, |inventory| (|| {
        let (evaluated_args, receiver_owners) = inventory;
        for arg in args {
            if arg.is_spread() {
                if saw_named {
                    return Err(EvalStatus::RuntimeFatal);
                }
                let spread = RuntimeOwnerSlot::new(eval_expr(
                    arg.value(),
                    context,
                    caller_scope,
                    values,
                )?);
                let unpacked = with_runtime_owners(&spread, || {
                    let result = (|| {
                        if !values.is_array_like(spread.borrowed())? {
                            return Err(EvalStatus::RuntimeFatal);
                        }
                        let first_unpacked = evaluated_args.len();
                        append_unpacked_call_arg_values(
                            spread.borrowed(),
                            evaluated_args,
                            &mut saw_named,
                            context,
                            values,
                        )?;
                        for argument in &mut evaluated_args[first_unpacked..] {
                            if argument.value.is_borrowed() {
                                argument.value = values.retain(argument.value)?;
                            }
                        }
                        Ok(())
                    })();
                    let released = release_expr_result(spread.take(), context, values);
                    result.and(released)
                });
                unpacked?;
                continue;
            }

            if let Some(name) = arg.name() {
                saw_named = true;
                let (value, ref_target) = eval_source_call_argument(
                    arg, evaluated_args.len(), modes, context, caller_scope, values)?;
                let (value, mut ref_target) = own_temporary_reference(value, ref_target, context, values)?;
                capture_argument_receiver_owners(ref_target.as_mut(), receiver_owners);
                let value = if value.is_borrowed() {
                    values.retain(value)?
                } else {
                    value
                };
                evaluated_args.push(EvaluatedCallArg {
                    evaluation_owner: None,
                    name: Some(name.to_string()),
                    value,
                    ref_target,
                });
                continue;
            }

            if saw_named {
                return Err(EvalStatus::RuntimeFatal);
            }
            let (value, ref_target) = eval_source_call_argument(
                arg, evaluated_args.len(), modes, context, caller_scope, values)?;
            let (value, mut ref_target) = own_temporary_reference(value, ref_target, context, values)?;
            capture_argument_receiver_owners(ref_target.as_mut(), receiver_owners);
            let value = if value.is_borrowed() {
                values.retain(value)?
            } else {
                value
            };
            evaluated_args.push(EvaluatedCallArg {
                evaluation_owner: None,
                name: None,
                value,
                ref_target,
            });
        }
        Ok(())
    })());
    let (evaluated_args, receiver_owners) = inventory;
    if let Err(status) = evaluated {
        let mut abandoned = RuntimeOwnerBatch::new(
            evaluated_args.into_iter().map(|argument| argument.value),
        );
        abandoned.append(RuntimeOwnerBatch::new(receiver_owners.transfer().into_iter().rev()));
        let _ = retire_eval_owner_batch(
            abandoned,
            ReleaseOrder::Forward,
            CleanupErrorPolicy::Ignore,
            context,
            values,
        );
        return Err(status);
    }
    Ok((evaluated_args, receiver_owners.transfer()))
}

/// Captures a retaining native boxed argument without changing legacy or intermediate receivers.
fn eval_source_call_argument(
    argument: &EvalCallArg, position: usize, modes: &CallArgumentModes,
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, Option<EvalReferenceTarget>), EvalStatus> {
    if modes.persistent_native_reference(argument.name(), position) {
        if let EvalExpr::LoadVar(name) = argument.value() {
            if let Some(reference) = eval_persistent_variable_reference(name, context, scope, values)? {
                return Ok((reference.borrowed(), Some(EvalReferenceTarget::Cell { cell: reference.borrowed() })));
            }
        }
    }
    eval_call_arg_value_with_mode(argument.value(), modes.by_reference(argument.name(), position), context, scope, values)
}

/// Resolves a reference receiver once and retires metadata belonging only to an intermediate hop.
pub(super) fn eval_call_argument_receiver(
    expr: &EvalExpr, by_reference: bool, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !by_reference { return eval_expr(expr, context, scope, values); }
    let (value, mut target) = eval_call_arg_value_with_selection(expr,
        Some(elephc_builtin_contract::ObjectProperty::WriteSelect), context, scope, values)?;
    let value = promote_borrowed_result(value, values);
    let mut owners = RuntimeOwnerBatch::empty();
    capture_abandoned_reference_owners(target.as_mut(), &mut owners);
    finish_argument_reference_owners(value.map(|value| (value, None)), owners, context, values)
        .map(|(value, _)| value)
}

/// Moves independently owned property receivers out of borrowed call reference metadata.
pub(super) fn capture_argument_receiver_owners(
    target: Option<&mut EvalReferenceTarget>, owners: &mut RuntimeOwnerBatch,
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

/// Extracts owned keys and receivers while leaving their reference target usable as a borrow.
pub(in crate::interpreter) fn capture_abandoned_reference_owners(
    target: Option<&mut EvalReferenceTarget>, owners: &mut RuntimeOwnerBatch,
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

/// Describes the owned keys and receivers of a reference result during argument cleanup.
struct ArgumentReferenceOwners(RefCell<Option<EvalReferenceTarget>>);

impl RuntimeOwners for ArgumentReferenceOwners {
    /// Visits only the operands that abandoning a reference would retire.
    fn visit_owned_slots(&self, visit: &mut dyn FnMut(*const *mut RuntimeCell)) {
        if let Some(target) = self.0.borrow().as_ref() {
            visit_argument_reference_owners(target, visit);
        }
    }
}

/// Reports the current primary owner fields for one reference target.
fn visit_argument_reference_owners(target: &EvalReferenceTarget, visit: &mut dyn FnMut(*const *mut RuntimeCell)) {
    match target {
        EvalReferenceTarget::ArrayElement { index, .. } => index.visit_owned_slots(visit),
        EvalReferenceTarget::NestedArrayElement { array_target, index } => {
            visit_argument_reference_owners(array_target, visit);
            index.visit_owned_slots(visit);
        }
        EvalReferenceTarget::ObjectProperty { object, .. } => object.visit_owned_slots(visit),
        _ => {}
    }
}

/// Retires intermediate lvalue operands without losing an owned result or abandoned target on failure.
pub(super) fn finish_argument_reference_owners(
    result: Result<(RuntimeCellHandle, Option<EvalReferenceTarget>), EvalStatus>,
    owners: RuntimeOwnerBatch, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, Option<EvalReferenceTarget>), EvalStatus> {
    if owners.is_empty() {
        return result;
    }
    let value = RuntimeOwnerSlot::empty();
    let target = ArgumentReferenceOwners(RefCell::new(None));
    let result = result.map(|(cell, metadata)| {
        value.replace(Some(cell));
        target.0.replace(metadata);
    });
    with_runtime_owners(&(&owners, (&value, &target)), || {
        let cleanup = owners.release(
            ReleaseOrder::Reverse,
            CleanupErrorPolicy::First,
            |owner| release_expr_result(owner, context, values),
        );
        match (result, cleanup) {
            (Err(status), _) => Err(status),
            (Ok(()), Err(status)) => {
                let _ = release_expr_result(value.take(), context, values);
                let mut abandoned = RuntimeOwnerBatch::empty();
                capture_abandoned_reference_owners(target.0.borrow_mut().as_mut(), &mut abandoned);
                let _ = retire_eval_owner_batch(
                    abandoned,
                    ReleaseOrder::Reverse,
                    CleanupErrorPolicy::Ignore,
                    context,
                    values,
                );
                Err(status)
            }
            (Ok(()), Ok(())) => Ok((value.take(), target.0.take())),
        }
    })
}

/// Promotes a temporary XML selection to a reference that may escape without parent writeback.
fn own_temporary_reference(
    value: RuntimeCellHandle, target: Option<EvalReferenceTarget>,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, Option<EvalReferenceTarget>), EvalStatus> {
    let owner = RuntimeOwnerSlot::new(value);
    with_runtime_owners(&owner, || {
        if matches!(&target, Some(EvalReferenceTarget::Cell { cell }) if *cell == value)
            && values.supports_persistent_references()
        {
            match values.is_reference(owner.borrowed()) {
                Ok(true) => return Ok((owner.take(), target)),
                Ok(false) => {}
                Err(status) => {
                    let _ = release_expr_result(owner.take(), context, values);
                    return Err(status);
                }
            }
            let result = values.reference_new(owner.borrowed());
            let reference = finish_synthesized_method_arguments(
                result,
                vec![owner.take()],
                context,
                values,
            )?;
            return Ok((
                reference,
                Some(EvalReferenceTarget::Cell {
                    cell: reference.borrowed(),
                }),
            ));
        }
        Ok((owner.take(), target))
    })
}
