//! Purpose:
//! Defers false-to-array handler exceptions until PHP's eligible write opcode completes.
//!
//! Called from:
//! - Local and ordinary property plain stores after publishing their empty parent.
//!
//! Key details:
//! - Original int, bool, and string keys and appends complete before rethrowing.
//! - Array/object key errors chain the callback exception; other key kinds stop the write.
//! - Compound updates keep immediate callback failure semantics at their caller.

use super::*;
use crate::runtime_hooks::owner_frames::{RuntimeOwnerSlot, with_runtime_owners};

/// Runs the already captured plain write while retaining any pending conversion exception.
pub(super) fn with_completion<V: RuntimeValueOps>(
    was_false: bool, index: Option<RuntimeCellHandle>, context: &mut ElephcEvalContext,
    values: &mut V,
    write: impl FnOnce(&mut ElephcEvalContext, &mut V) -> Result<(), EvalStatus>,
) -> Result<(), EvalStatus> {
    match eval_false_array_parent_diagnostic(was_false, context, values) {
        Ok(()) => return write(context, values),
        Err(EvalStatus::UncaughtThrowable) => {}
        Err(status) => return Err(status),
    }
    let complete = match index.map(|key| values.type_tag(key)).transpose()? {
        None | Some(EVAL_TAG_INT | EVAL_TAG_BOOL | EVAL_TAG_STRING
            | EVAL_TAG_ARRAY | EVAL_TAG_ASSOC | EVAL_TAG_OBJECT) => true,
        _ => false,
    };
    if !complete { return Err(EvalStatus::UncaughtThrowable); }
    let previous = RuntimeOwnerSlot::new(context.take_pending_throw().ok_or(EvalStatus::RuntimeFatal)?);
    with_runtime_owners(&previous, || {
        match write(context, values) {
            Ok(()) => {
                context.set_pending_throw(previous.take());
                Err(EvalStatus::UncaughtThrowable)
            }
            Err(EvalStatus::UncaughtThrowable) => chain_pending(previous.take(), context, values),
            Err(status) => finish_synthesized_method_arguments(Err(status), [previous.take()], context, values).map(|_| ()),
        }
    })
}

/// Consumes a parked predecessor without replacing the new Throwable or its existing links.
fn chain_pending(
    previous: RuntimeCellHandle, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let previous = RuntimeOwnerSlot::new(previous);
    let current = context.take_pending_throw().map(RuntimeOwnerSlot::new);
    with_runtime_owners(&(&previous, &current), || {
        let result = match &current {
            Some(current) => {
                let chained = values.chain_throwables(current.borrowed(), previous.borrowed());
                let chained = native_results::finish(chained, context, values);
                if chained.is_ok() { context.set_pending_throw(current.take()); }
                chained.and(Err::<RuntimeCellHandle, _>(EvalStatus::UncaughtThrowable))
            }
            None => Err(EvalStatus::RuntimeFatal),
        };
        finish_synthesized_method_arguments(result,
            [previous.take(), current.as_ref().map(RuntimeOwnerSlot::take)
                .unwrap_or_else(|| RuntimeCellHandle::from_raw(std::ptr::null_mut()).borrowed())],
            context, values).map(|_| ())
    })
}
