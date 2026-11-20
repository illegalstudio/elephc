//! Purpose:
//! Creates, copies and retires source producer contracts on native value bindings.
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
    let key = (ctx.loop_storage_scope.clone(), span, name.into());
    let Some(copy) = ctx.local_store_contracts.copies.get(&key).cloned() else {
        prepare_producer(ctx, name, span);
        return;
    };
    let source_name = &copy.source;
    if !ctx.local_can_own_reference_cell(source_name)
        || crate::names::is_generated_local_name(source_name) {
        prepare_producer(ctx, name, span);
        return;
    }
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

/// Publishes a first producer only on the executed path that creates its binding.
fn prepare_producer(ctx: &mut LoweringContext<'_, '_>, name: &str, span: Span) {
    let Some(contract) = ctx.local_store_contracts.origins.get(
        &(ctx.loop_storage_scope.clone(), span, name.into())).cloned() else { return; };
    start_producer(ctx, name, contract, span);
}

/// Seeds a by-value parameter's declared origin before its owned shadow receives the input.
pub(super) fn prepare_entry(ctx: &mut LoweringContext<'_, '_>, name: &str) {
    if ctx.local_store_contracts.entry_has_captured_contract(&ctx.loop_storage_scope, name) {
        super::captured_binding_origins::prepare_entry(ctx, name);
        return;
    }
    let Some(contract) = ctx.local_store_contracts.entry_origin(&ctx.loop_storage_scope, name).cloned()
        else { return; };
    start_producer(ctx, name, contract, Span::dummy());
}

/// Publishes a fresh producer without replacing the contract of an already live binding.
fn start_producer(ctx: &mut LoweringContext<'_, '_>, name: &str, contract: PhpType, span: Span) {
    let Some(owner) = ctx.ref_cell_owner_slot(name) else { return; };
    let destination = ctx.local_slots[name];
    super::local_binding_state::when_unbound(ctx, destination, span, |ctx| {
        if let Some(contract) = crate::semantics::conversions::inferred_binding_contract(&contract) {
            ctx.builder.emit_with_effects(Op::BindLocalRefContract, Vec::new(),
                Some(Immediate::LocalSlot(owner)), IrType::Void, contract,
                Ownership::NonHeap, Op::BindLocalRefContract.default_effects(), Some(span));
        }
        ctx.declare_eval_scope_local();
        let name = ctx.intern_global_name(name);
        ctx.emit_void(Op::EvalScopeStartBinding, Vec::new(),
            Some(Immediate::EvalScopeStore(Box::new(crate::ir::EvalScopeStoreImmediate {
                name, contract: crate::semantics::conversions::inferred_binding_contract(&contract),
                role: crate::ir::EvalBindingRole::Contract,
            }))), Op::EvalScopeStartBinding.default_effects(), Some(span));
    });
}

/// Starts a reached reference declaration without importing the name's displaced producer.
pub(super) fn start_reference_declaration(ctx: &mut LoweringContext<'_, '_>, name: &str,
    contract: PhpType, span: Span,
) {
    if !ctx.local_store_contracts.has_runtime_origin(&ctx.loop_storage_scope, name) { return; }
    let slot = ctx.local_slots[name];
    super::local_binding_state::emit(ctx, slot, false);
    start_producer(ctx, name, contract, span);
    super::local_binding_state::emit(ctx, slot, true);
}

/// Ends the logical origin without abandoning a frame slot shared by other source paths.
pub(super) fn end(ctx: &mut LoweringContext<'_, '_>, name: &str, span: Option<Span>) {
    if !ctx.local_store_contracts.has_runtime_origin(&ctx.loop_storage_scope, name) { return; }
    if let Some(slot) = ctx.local_slots.get(name).copied() {
        super::local_binding_state::emit(ctx, slot, false);
    }
    ctx.declare_eval_scope_local();
    let origin = ctx.intern_global_name(name);
    ctx.emit_void(Op::EvalScopeEndBinding, Vec::new(), Some(Immediate::GlobalName(origin)),
        Op::EvalScopeEndBinding.default_effects(), span);
}
