//! Purpose:
//! Preserves attached internal SPL storage behind interface-typed nested write receivers.
//!
//! Called from:
//! - Nested write-parent selection after concrete SPL and native dimension dispatch.
//!
//! Key details:
//! - A metadata-only runtime probe admits unchanged internal SPL getters.
//! - Ordinary user getters retain the established method-call and reference-return path.
//! - Both branches evaluate the key once and protect the selected child before input cleanup.

use super::*;

/// Returns whether an erased interface receiver can hold an internal SPL storage object.
pub(super) fn supports_interface_receiver(
    ctx: &LoweringContext<'_, '_>,
    ty: &PhpType,
) -> bool {
    let PhpType::Object(name) = ty.codegen_repr() else {
        return false;
    };
    ctx.interfaces.contains_key(&name)
        && ctx.classes.keys().any(|class| {
            crate::types::builtin_classes::spl_storage_write_fetch_owner(class, ctx.classes)
                .is_some()
                && ctx.type_is_or_implements_interface(class, &name)
        })
}

/// Selects attached storage only for actual trusted SPL instances behind an interface type.
pub(super) fn fetch_interface_parent(
    ctx: &mut LoweringContext<'_, '_>, receiver: LoweredValue, index: &Expr, expr: &Expr,
    key_already_diagnosed: bool,
) -> Option<LoweredValue> {
    let receiver_ty = ctx.builder.value_php_type(receiver.value);
    if !supports_interface_receiver(ctx, &receiver_ty) {
        return None;
    }
    let span = expr.span;
    let (receiver, receiver_owner) = super::nested_array_writes::root_static_update_receiver(ctx, receiver, span);
    let trusted = ctx.emit_value(Op::RuntimeCall, vec![receiver.value],
        Some(Immediate::RuntimeCall(RuntimeCallTarget::SplStorageHasAttachedWriteFetch)), PhpType::Int,
        crate::ir::Effects::READS_HEAP | crate::ir::Effects::READS_GLOBAL, Some(span));
    let attached = ctx.builder.create_named_block("spl.parent.attached", Vec::new());
    let public = ctx.builder.create_named_block("spl.parent.public", Vec::new());
    let ir_type = IrType::from_php(&PhpType::Mixed);
    let done = ctx.builder.create_named_block("spl.parent.selected", vec![(ir_type, PhpType::Mixed)]);
    let initialized = ctx.initialized_slots_snapshot();
    ctx.builder.terminate(Terminator::CondBr { cond: trusted.value, then_target: attached,
        then_args: Vec::new(), else_target: public, else_args: Vec::new() });
    ctx.builder.position_at_end(attached);
    let boxed = ctx.box_value_as_mixed(receiver, PhpType::Mixed, Some(span));
    let child = super::nested_plain_writes::fetch_parent(ctx, boxed, index, span, key_already_diagnosed);
    ctx.builder.terminate(Terminator::Br { target: done, args: vec![child.value] });
    ctx.builder.position_at_end(public);
    ctx.restore_initialized_slots(initialized.clone());
    let child = lower_array_access_from_lowered_receiver(ctx, receiver, index, expr);
    let child = if ctx.builder.value_php_type(child.value).codegen_repr() == PhpType::Mixed {
        if ctx.value_is_owning_temporary(child) { child }
        else { crate::ir_lower::ownership::acquire_if_refcounted(ctx, child, Some(span)) }
    } else { ctx.box_value_as_mixed(child, PhpType::Mixed, Some(span)) };
    ctx.builder.terminate(Terminator::Br { target: done, args: vec![child.value] });
    ctx.builder.position_at_end(done);
    ctx.restore_initialized_slots(initialized);
    let value = ctx.builder.block_param(done, 0);
    ctx.builder.set_value_ownership(value, Ownership::Owned);
    Some(super::nested_plain_writes::finish_parent_fetch(ctx,
        LoweredValue { value, ir_type }, receiver_owner, None, span))
}
