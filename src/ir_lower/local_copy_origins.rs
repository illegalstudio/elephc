//! Purpose:
//! Transfers post-eval producer contracts to independent native value copies.
//!
//! Called from:
//! - Statement and expression local assignments after ordinary value-copy preparation.
//!
//! Key details:
//! - Only a fresh destination receives a source contract.
//! - Native-origin metadata is conditional; eval origins retain their runtime descriptor.

use super::context::LoweringContext;
use crate::{ir::{Immediate, IrType, Op, Ownership}, span::Span, types::PhpType};

/// Preserves an independent producer origin without copying the source's reference identity.
pub(super) fn prepare(ctx: &mut LoweringContext<'_, '_>, name: &str, span: Span) {
    if ctx.source_mode == crate::source::SourceMode::Internal
        || !ctx.supports_eval_native_binding_store(name) { return; }
    let Some(copy) = ctx.local_store_contracts.copies.get(
        &(ctx.loop_storage_scope.clone(), span, name.into())).cloned() else { return; };
    let source_name = &copy.source;
    if !ctx.local_can_own_reference_cell(source_name)
        || crate::names::is_generated_local_name(source_name) { return; }
    let Some(source_slot) = ctx.local_slots.get(source_name).copied() else { return; };
    let Some(owner) = ctx.ref_cell_owner_slot(name) else { return; };
    let destination = ctx.local_slots[name];
    let contract = copy.contract;
    super::local_binding_state::when_unbound(ctx, destination, span, |ctx| {
        let native = ctx.emit_value(Op::GetNativeBindingState, Vec::new(),
            Some(Immediate::LocalSlot(source_slot)), PhpType::Bool,
            Op::GetNativeBindingState.default_effects(), Some(span));
        ctx.declare_eval_scope_local();
        let source = ctx.intern_global_name(source_name);
        ctx.builder.emit_with_effects(Op::EvalScopeCopyBindingType,
            vec![native.value], Some(Immediate::EvalBindingCopy { source, destination, owner }),
            IrType::Void, contract, Ownership::NonHeap,
            Op::EvalScopeCopyBindingType.default_effects(), Some(span));
    });
}
