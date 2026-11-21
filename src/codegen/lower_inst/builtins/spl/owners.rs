//! Purpose:
//! Publishes iterator_apply's retained callback descriptor to exception cleanup.
//!
//! Called from:
//! - The native SPL iterator_apply lowering around its saved callback state.
//!
//! Key details:
//! - The record sits below the descriptor so loop-relative callback offsets stay unchanged.
//! - Normal retirement detaches before release; exceptional retirement clears the owner slot.

use super::{abi, FunctionContext, IteratorApplyCallback};

/// A reserved record for the independent descriptor owner held by one iterator loop.
#[derive(Clone, Copy)]
pub(super) struct DescriptorGuard(bool);

impl DescriptorGuard {
    /// Reserves cleanup storage before callback state is pushed onto the temporary stack.
    pub(super) fn reserve(ctx: &mut FunctionContext<'_>, callback: &IteratorApplyCallback) -> Self {
        let owned = matches!(callback, IteratorApplyCallback::DescriptorCallable { .. }
            | IteratorApplyCallback::DescriptorCallableArray { release_runtime_descriptor: true, .. });
        if owned { abi::emit_reserve_temporary_stack(ctx.emitter, abi::CALL_OPERAND_OWNER_RECORD_BYTES); }
        Self(owned)
    }

    /// Links the saved descriptor after its retaining or allocating operation succeeds.
    pub(super) fn publish(self, ctx: &mut FunctionContext<'_>) {
        if !self.0 { return; }
        let owner = abi::tertiary_scratch_reg(ctx.emitter);
        abi::emit_temporary_stack_address(ctx.emitter, owner, 0);
        abi::emit_link_call_operand_owner_at_stack(ctx.emitter, owner, true, 16);
    }

    /// Detaches the exact owner before normal callback-state release can run user code.
    pub(super) fn detach(self, ctx: &mut FunctionContext<'_>) {
        if self.0 { abi::emit_unlink_call_operand_owner_at_stack(ctx.emitter, 16); }
    }

    /// Releases the reserved record after the saved descriptor slot has been popped.
    pub(super) fn finish(self, ctx: &mut FunctionContext<'_>) {
        if self.0 { abi::emit_release_temporary_stack(ctx.emitter, abi::CALL_OPERAND_OWNER_RECORD_BYTES); }
    }
}
