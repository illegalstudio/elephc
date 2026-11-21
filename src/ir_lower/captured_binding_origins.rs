//! Purpose:
//! Transports immutable executed-producer contracts with closure value snapshots.
//!
//! Called from:
//! - Closure creation and the common incoming-binding initialization path.
//!
//! Key details:
//! - Descriptors borrow program metadata and need no PHP owner or new closure layout.
//! - Each activation installs an independent binding through the ordinary copy operation.

use super::context::{LoweredValue, LoweringContext};
use crate::{ir::{Immediate, IrType, Op, Ownership}, span::Span, types::PhpType};

/// Freezes the selected producer before the enclosing activation can retire or rebind its name.
pub(super) fn freeze(ctx: &mut LoweringContext<'_, '_>, name: &str, span: Span) -> Option<LoweredValue> {
    let scope = crate::types::nested_loop_storage_scope(&ctx.loop_storage_scope, span);
    if !ctx.local_store_contracts.entry_has_captured_contract(&scope, name) { return None; }
    ctx.declare_eval_scope_local();
    let name = ctx.intern_global_name(name);
    Some(ctx.emit_value(Op::EvalScopeBindingType, Vec::new(), Some(Immediate::GlobalName(name)),
        PhpType::Int, Op::EvalScopeBindingType.default_effects(), Some(span)))
}

/// Reuses canonical binding-copy publication before the incoming value reaches its fresh shadow.
pub(super) fn prepare_entry(ctx: &mut LoweringContext<'_, '_>, name: &str) {
    let hidden = crate::names::captured_contract_local_name(name);
    let descriptor = ctx.load_local(&hidden, None);
    let owner = ctx.ref_cell_owner_slot(name).expect("captured origins require an owning reference handle");
    let destination = ctx.local_slots[name];
    let source = ctx.intern_global_name(name);
    ctx.declare_eval_scope_local();
    super::local_binding_state::when_unbound(ctx, destination, Span::dummy(), |ctx| {
        let native = ctx.emit_value(Op::ConstBool, Vec::new(), Some(Immediate::Bool(false)),
            PhpType::Bool, Op::ConstBool.default_effects(), None);
        ctx.builder.emit_with_effects(Op::EvalScopeCopyBindingType, vec![native.value, descriptor.value],
            Some(Immediate::EvalBindingCopy { source, destination, owner }),
            IrType::Void, PhpType::Mixed, Ownership::NonHeap,
            Op::EvalScopeCopyBindingType.default_effects(), None);
    });
}
