//! Purpose:
//! Owns foreach sources and prepared operands through publication and retirement.
//!
//! Called from:
//! - Array, Iterator and IteratorAggregate foreach execution.
//!
//! Key details:
//! - Both operands are prepared before publishing the value followed by the key.
//! - Root slots detach transferred owners before callbacks can reenter PHP.

use super::*;
use crate::runtime_hooks::owner_frames::{RuntimeOwnerSlot, with_runtime_owners};

/// Retires all transferred owners even when an earlier destructor throws.
pub(super) fn release(owners: impl IntoIterator<Item = RuntimeCellHandle>,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let owners = owners.into_iter().filter(|owner| !owner.is_null() && !owner.is_borrowed())
        .map(RuntimeOwnerSlot::new).collect::<Vec<_>>();
    with_runtime_owners(&owners, || {
        let mut result = Ok(());
        for owner in &owners {
            let released = eval_release_value(context, values, owner.take());
            if result.is_ok() { result = released; }
        }
        result
    })
}

/// Publishes an owned candidate before retiring every displaced binding lease.
fn publish(name: &str, owner: &RuntimeOwnerSlot, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let replaced = set_owned_scope_cell(context, scope, name.to_string(), owner.take(), values)?;
    release(replaced, context, values)
}

/// Acquires both operands once and consumes their leases in PHP publication order.
pub(super) fn bind<V: RuntimeValueOps>(value_name: &str, key_name: Option<&str>,
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut V,
    value: impl FnOnce(&mut ElephcEvalContext, &mut V) -> Result<RuntimeCellHandle, EvalStatus>,
    key: impl FnOnce(&mut ElephcEvalContext, &mut V) -> Result<RuntimeCellHandle, EvalStatus>,
) -> Result<(), EvalStatus> {
    let value = value(context, values).and_then(|value| promote_borrowed_result(value, values))?;
    let value = RuntimeOwnerSlot::new(value);
    let key = with_runtime_owners(&value, || {
        key_name.map(|_| key(context, values).and_then(|key| promote_borrowed_result(key, values)))
            .transpose().map(|key| key.map(RuntimeOwnerSlot::new))
    });
    let key = match key {
        Ok(key) => key,
        Err(status) => { let _ = release([value.take()], context, values); return Err(status); }
    };
    with_runtime_owners(&(&value, &key), || {
        let result = publish(value_name, &value, context, scope, values).and_then(|()| {
            if let Some((name, key)) = key_name.zip(key.as_ref()) {
                publish(name, key, context, scope, values)
            } else { Ok(()) }
        });
        let released = release(std::iter::once(value.take()).chain(key.as_ref().map(RuntimeOwnerSlot::take)), context, values);
        result.and(released)
    })
}

/// Checks a prepared Iterator predicate while keeping it rooted through a possible TypeError.
pub(super) fn condition(value: RuntimeCellHandle, context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let owner = RuntimeOwnerSlot::new(value);
    let result = with_runtime_owners(&owner, || eval_boolean_condition(owner.borrowed(), context, values));
    let released = release([owner.take()], context, values);
    result.and_then(|condition| released.map(|()| condition))
}

/// Preserves an owned return or throw value while source retirement executes PHP callbacks.
fn finish_owned_control(value: RuntimeCellHandle, wrap: fn(RuntimeCellHandle) -> EvalControl,
    source: &RuntimeOwnerSlot, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    let value = RuntimeOwnerSlot::new(value);
    let released = with_runtime_owners(&value, || release([source.take()], context, values));
    match released {
        Ok(()) => Ok(wrap(value.take())),
        Err(status) => { let _ = release([value.take()], context, values); Err(status) }
    }
}

/// Roots one owned iterable through its body and consumes it on every control-flow exit.
pub(super) fn with_source<V: RuntimeValueOps>(source: RuntimeCellHandle,
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut V,
    body: impl FnOnce(RuntimeCellHandle, &mut ElephcEvalContext, &mut ElephcEvalScope, &mut V)
        -> Result<EvalControl, EvalStatus>,
) -> Result<EvalControl, EvalStatus> {
    let source = RuntimeOwnerSlot::new(promote_borrowed_result(source, values)?);
    let result = with_runtime_owners(&source, || body(source.borrowed(), context, scope, values));
    match result {
        Ok(EvalControl::Return(value)) => finish_owned_control(value, EvalControl::Return, &source, context, values),
        Ok(EvalControl::Throw(value)) => finish_owned_control(value, EvalControl::Throw, &source, context, values),
        result => {
            let released = release([source.take()], context, values);
            result.and_then(|control| released.map(|()| control))
        }
    }
}
