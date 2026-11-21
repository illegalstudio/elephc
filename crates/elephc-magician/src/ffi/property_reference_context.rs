//! Purpose:
//! Keeps a reference conversion's execution context alive through reentrant PHP callbacks.
//!
//! Called from:
//! - The external eval property-source provider after immutable declaration planning.
//!
//! Key details:
//! - An active native caller already owns its context; other contexts acquire a bounded ABI owner.
//! - The lease retains metadata rather than property receivers or their reference bindings.
//! - Final context teardown returns its boxed Throwable before the native publisher resumes.

use crate::{context::ElephcEvalContext, errors::EvalStatus,
    value::{RuntimeCell, RuntimeCellHandle}};

/// An active execution borrow or one independently owned heap-context activation.
pub(super) struct ReferenceContextLease {
    context: *mut ElephcEvalContext,
    owned: bool,
}

impl ReferenceContextLease {
    /// Stabilizes a live thread-bound ABI context without retaining a PHP receiver.
    ///
    /// # Safety
    /// Non-active contexts must be live handles allocated by the context-new ABI. An active
    /// native caller keeps its context alive until this conversion returns.
    pub(super) unsafe fn acquire(context: *mut ElephcEvalContext) -> Result<Self, EvalStatus> {
        if context.is_null() { return Err(EvalStatus::RuntimeFatal); }
        let owned = crate::context::native_frame_active_eval_context() != Some(context);
        if owned && unsafe { super::context::__elephc_eval_context_retain(context) } != EvalStatus::Ok.code() {
            return Err(EvalStatus::RuntimeFatal);
        }
        Ok(Self { context, owned })
    }

    /// Transfers a pending conversion or teardown exception after retiring this activation.
    pub(super) fn finish(mut self) -> Option<RuntimeCellHandle> {
        let context = std::mem::replace(&mut self.context, std::ptr::null_mut());
        if self.owned { unsafe { release_owned_context(context) } }
        else { unsafe { context.as_mut() }.and_then(ElephcEvalContext::take_pending_throw) }
    }
}

impl Drop for ReferenceContextLease {
    /// Retires a metadata lease even when a Rust panic interrupts candidate preparation.
    fn drop(&mut self) {
        if !self.owned || self.context.is_null() { return; }
        let pending = unsafe { release_owned_context(self.context) };
        #[cfg(not(test))]
        {
            let mut pending = pending;
            while let Some(error) = pending {
                pending = crate::runtime_hooks::release::release_native_cells([error], None);
            }
        }
        #[cfg(test)]
        let _ = pending;
    }
}

/// Releases a context owner without borrowing metadata after its allocation can disappear.
///
/// # Safety
/// Context is a live heap ABI handle with this lease's owner. No PHP executes between the
/// final-owner/deferred checks and the free entry. Teardown itself contains every PHP throw.
unsafe fn release_owned_context(context: *mut ElephcEvalContext) -> Option<RuntimeCellHandle> {
    let owner = unsafe { context.as_mut() }?;
    let deferred = owner.teardown_active() || !owner.is_last_abi_owner()
        || crate::context::pcntl_runtime::defer_context_free(context);
    let pending = if deferred { owner.take_pending_throw() } else { None };
    let teardown = unsafe { super::context::__elephc_eval_context_free(context) };
    debug_assert!(pending.is_none() || teardown.is_null(), "deferred teardown cannot run PHP");
    pending.or_else(|| (!teardown.is_null()).then(|| RuntimeCellHandle::from_raw(teardown.cast::<RuntimeCell>())))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Releasing the original host owner during a callback leaves metadata live until completion.
    #[test]
    fn reference_context_lease_survives_last_host_release_and_transfers_pending_throw() {
        let context = super::super::context::__elephc_eval_context_new();
        let lease = unsafe { ReferenceContextLease::acquire(context) }.unwrap();
        assert!(unsafe { super::super::context::__elephc_eval_context_free(context) }.is_null());
        assert_eq!(unsafe { (*context).abi_version() }, crate::abi::ABI_VERSION);
        let error = RuntimeCellHandle::from_raw(8usize as *mut RuntimeCell);
        unsafe { (*context).set_pending_throw(error); }
        assert_eq!(lease.finish(), Some(error));
    }

    /// An existing native activation retains its ownership when the conversion finishes.
    #[test]
    fn reference_context_lease_borrows_active_native_execution() {
        let context = super::super::context::__elephc_eval_context_new();
        let frame = crate::context::push_native_frame_context(context);
        let lease = unsafe { ReferenceContextLease::acquire(context) }.unwrap();
        assert!(lease.finish().is_none());
        assert!(unsafe { (*context).is_last_abi_owner() });
        drop(frame);
        assert!(unsafe { super::super::context::__elephc_eval_context_free(context) }.is_null());
    }

    /// Nested teardown keeps its pending conversion failure without taking a second context Box.
    #[test]
    fn reference_context_lease_contains_pending_throw_during_outer_teardown() {
        let context = super::super::context::__elephc_eval_context_new();
        assert!(unsafe { (*context).release_abi_owner() });
        assert!(unsafe { (*context).begin_teardown() });
        let lease = unsafe { ReferenceContextLease::acquire(context) }.unwrap();
        let error = RuntimeCellHandle::from_raw(8usize as *mut RuntimeCell);
        unsafe { (*context).set_pending_throw(error); }
        assert_eq!(lease.finish(), Some(error));
        unsafe {
            (*context).end_teardown();
            super::super::context::drop_eval_context_now(context);
        }
    }
}
