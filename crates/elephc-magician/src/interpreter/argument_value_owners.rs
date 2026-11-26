//! Purpose:
//! Tracks exact eval argument leases through native binding and their eventual cleanup.
//!
//! Called from:
//! - Eval argument boundaries and native function or descriptor entry.
//!
//! Key details:
//! - Shared Rust metadata identifies an existing owner without acquiring another PHP alias.
//! - A pending owner is consumed only when its exact native cell acquires a parameter binding.
//! - The originating boundary unlinks consumed records or retires still-pending owners.

use super::*;
use elephc_memoir::runtime_argument_owner::RuntimeArgumentOwner;
use crate::runtime_hooks::owner_frames::{RuntimeOwners, RuntimeOwnerSlot};
use crate::value::RuntimeCell;

/// Shared provenance for one independently acquired source argument owner.
#[derive(Clone)]
pub(super) struct ArgumentValueOwner(RuntimeArgumentOwner);

/// Keeps ordinary arguments allocation-free while reference candidates carry shared provenance.
pub(super) enum ArgumentCleanupOwner {
    Value(RuntimeOwnerSlot),
    Reference(ArgumentValueOwner),
}

impl RuntimeOwners for BoundMethodArg {
    /// Defaults, conversions and variadic keys own leases independently of source descriptors.
    fn visit_owned_slots(&self, visit: &mut dyn FnMut(*const *mut RuntimeCell)) {
        self.value.visit_owned_slots(visit);
        for (key, _) in &self.variadic_ref_targets { key.visit_owned_slots(visit); }
    }
}

impl ArgumentCleanupOwner {
    /// Consumes the original owner or its replacement pin without borrowing shared state through PHP.
    pub(super) fn release(&self, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps) -> Result<(), EvalStatus> {
        match self {
            Self::Value(value) => release_expr_result(value.take(), context, values),
            Self::Reference(owner) => owner.release(context, values),
        }
    }
}

impl RuntimeOwners for ArgumentCleanupOwner {
    /// Reports the current real lease, following its transfer into a still-pending native pin.
    fn visit_owned_slots(&self, visit: &mut dyn FnMut(*const *mut RuntimeCell)) {
        match self {
            Self::Value(value) => value.visit_owned_slots(visit),
            Self::Reference(owner) => owner.0.visit_owned_slots(visit),
        }
    }
}

impl RuntimeOwners for ArgumentValueOwner {
    /// Follows the shared lease as it moves from its source owner into a native pin.
    fn visit_owned_slots(&self, visit: &mut dyn FnMut(*const *mut RuntimeCell)) {
        self.0.visit_owned_slots(visit);
    }
}

impl ArgumentValueOwner {
    /// Adopts one owner already acquired by argument evaluation without retaining it again.
    pub(super) fn new(value: RuntimeCellHandle) -> Self {
        Self(RuntimeArgumentOwner::new(value))
    }

    /// Reads only the lease identity; no Rust borrow survives a runtime operation.
    fn value(&self) -> Option<RuntimeCellHandle> {
        self.0.value()
    }

    /// Replaces a lease with a pending root whose visibility changes at native parameter entry.
    fn pin(&self, cell: u64, staging: *mut std::ffi::c_void, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps) -> Result<(), EvalStatus> {
        let previous = self.0.try_pin(cell, staging, |value, pin, cell, staging| {
            values.pin_native_reference_argument_owner(value, pin, cell, staging)
        })?;
        match previous {
            Some(value) => release_expr_result(value, context, values),
            None => Ok(()),
        }
    }

    /// Clears shared metadata before releasing its unique foreign owner, including on failure.
    pub(super) fn release(&self, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps) -> Result<(), EvalStatus> {
        match self.0.take(|pin| values.take_native_reference_argument_owner(pin))? {
            Some(value) => release_expr_result(value, context, values),
            None => Ok(()),
        }
    }
}

/// Gives borrowed argument descriptors exact provenance while their original boundary owns cleanup.
pub(super) fn own_evaluated_call_arguments(
    arguments: Vec<EvaluatedCallArg>,
    reference_candidate: impl Fn(&EvaluatedCallArg, usize) -> bool,
) -> (Vec<EvaluatedCallArg>, Vec<ArgumentCleanupOwner>) {
    let mut owners = Vec::with_capacity(arguments.len());
    let arguments = arguments.into_iter().enumerate().map(|(index, mut argument)| {
        if !argument.value.is_borrowed() {
            if reference_candidate(&argument, index) {
                let owner = ArgumentValueOwner::new(argument.value);
                argument.evaluation_owner = Some(owner.clone());
                owners.push(ArgumentCleanupOwner::Reference(owner));
            } else {
                owners.push(ArgumentCleanupOwner::Value(RuntimeOwnerSlot::new(argument.value)));
            }
        }
        argument.value = argument.value.borrowed();
        argument
    }).collect();
    (arguments, owners)
}

/// Associates extracted array argument leases with their descriptors in linear time.
pub(super) fn own_unpacked_call_argument_owners(
    arguments: &mut [EvaluatedCallArg], values: Vec<RuntimeCellHandle>,
) -> Vec<ArgumentCleanupOwner> {
    let candidates = arguments.iter().filter(|arg| arg.ref_target.is_some())
        .map(|arg| arg.value.as_ptr() as usize).collect::<std::collections::HashSet<_>>();
    let mut references = std::collections::HashMap::<usize, std::collections::VecDeque<ArgumentValueOwner>>::new();
    let owners = values.into_iter().map(|value| {
        if !value.is_borrowed() && candidates.contains(&(value.as_ptr() as usize)) {
            let owner = ArgumentValueOwner::new(value);
            references.entry(value.as_ptr() as usize).or_default().push_back(owner.clone());
            ArgumentCleanupOwner::Reference(owner)
        } else { ArgumentCleanupOwner::Value(RuntimeOwnerSlot::new(value)) }
    }).collect();
    for argument in arguments {
        if argument.ref_target.is_none() { continue; }
        argument.evaluation_owner = references.get_mut(&(argument.value.as_ptr() as usize))
            .and_then(std::collections::VecDeque::pop_front);
    }
    owners
}

/// Prepares only outer source leases matching this invocation's managed native references.
pub(super) fn pin_native_call_evaluation_owners(
    arguments: &BoundNativeFunctionArgs, context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if arguments.evaluation_owners.is_empty() { return Ok(()); }
    let managed = arguments.ref_slots.iter().filter_map(|slot| match slot {
        BoundNativeFunctionRefSlot::ManagedMixed { original, cell, pin } => Some((original.as_ptr() as usize, (*cell, pin.as_ptr()))),
        _ => None,
    }).collect::<std::collections::HashMap<_, _>>();
    for owner in &arguments.evaluation_owners {
        let Some(value) = owner.value() else { continue; };
        if let Some((cell, staging)) = managed.get(&(value.as_ptr() as usize)) {
            owner.pin(*cell, *staging, context, values)?;
        }
    }
    Ok(())
}

/// Retains a borrowed result before consuming every argument owner in its original order.
pub(super) fn finish_eval_argument_owners(
    result: Result<RuntimeCellHandle, EvalStatus>, owners: impl AsRef<[ArgumentCleanupOwner]>,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let result = result.and_then(|value| promote_borrowed_result(value, values))
        .map(RuntimeOwnerSlot::new);
    finish_eval_argument_slots(result, owners.as_ref(), context, values, |owner, context, values| {
        owner.release(context, values)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Binder values and variadic keys contribute only their own distinct retained fields.
    #[test]
    fn bound_argument_owners_exclude_borrowed_reference_metadata() {
        let value = RuntimeCellHandle::from_raw(8usize as *mut RuntimeCell);
        let key = RuntimeCellHandle::from_raw(16usize as *mut RuntimeCell);
        let arguments = vec![
            BoundMethodArg { value, ref_target: None, variadic_ref_targets: vec![
                (key, EvalReferenceTarget::Cell { cell: value.borrowed() }),
            ] },
            BoundMethodArg { value, ref_target: None, variadic_ref_targets: Vec::new() },
            BoundMethodArg { value: value.borrowed(), ref_target: None, variadic_ref_targets: Vec::new() },
        ];
        let mut slots = Vec::new();
        arguments.visit_owned_slots(&mut |slot| slots.push(slot as usize));
        assert_eq!(slots.len(), 3, "two independent value owners and one variadic key");
        assert_eq!(slots.iter().copied().collect::<std::collections::HashSet<_>>().len(), 3);
        assert!(slots.contains(&(arguments[0].value.owner_slot() as usize)));
        assert!(slots.contains(&(arguments[0].variadic_ref_targets[0].0.owner_slot() as usize)));
        assert!(slots.contains(&(arguments[1].value.owner_slot() as usize)));
        assert!(!slots.contains(&(arguments[2].value.owner_slot() as usize)));
    }

}
