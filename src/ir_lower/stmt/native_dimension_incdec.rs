//! Purpose:
//! Selects native XML statement incdec without storing its scalar result back.
//!
//! Called from:
//! - Local, static and retained nested dimension update lowering.
//!
//! Key details:
//! - Ordinary receivers keep their caller's existing update path.
//! - Native receiver leases survive computed keys and same-frame exception cleanup.

use super::*;

/// Recognizes a native XML lineage through its writable dimension contract.
fn known_xml(ctx: &LoweringContext<'_, '_>, ty: &PhpType) -> bool {
    crate::ir_lower::expr::singular_object_class(ty).is_some_and(|(class, _)|
        crate::types::builtin_classes::native_dimension_method(class,
            elephc_builtin_contract::ObjectDimension::WriteSelect, ctx.classes).is_some())
}

/// Limits runtime native dispatch to opaque receivers and declared native XML dimensions.
pub(super) fn may_contain_xml(ctx: &LoweringContext<'_, '_>, ty: &PhpType) -> bool {
    ctx.classes.contains_key("SimpleXMLElement") && (known_xml(ctx, ty)
        || matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
        || matches!(ty.codegen_repr(), PhpType::Object(class)
            if class.is_empty() || ctx.interfaces.contains_key(&class)))
}

/// Applies native RW selection or invokes the unchanged ordinary update with a borrowed receiver.
pub(super) fn lower(
    ctx: &mut LoweringContext<'_, '_>, array: LoweredValue, index: &Expr,
    increment: bool, span: Span,
    fallback: impl FnOnce(&mut LoweringContext<'_, '_>, LoweredValue),
) -> bool {
    let ty = ctx.builder.value_php_type(array.value);
    if !may_contain_xml(ctx, &ty) { return false; }
    let pins = crate::ir_lower::expr::pin_in_flight_owners(ctx, &[array.value], span);
    if known_xml(ctx, &ty) {
        emit_native(ctx, array, index, increment, span);
    } else {
        let class = ctx.intern_class_name("SimpleXMLElement");
        let condition = ctx.emit_value(Op::InstanceOf, vec![array.value], Some(Immediate::Data(class)),
            PhpType::Bool, Op::InstanceOf.default_effects(), Some(span));
        let native = ctx.builder.create_named_block("dimension.incdec.native", vec![]);
        let ordinary = ctx.builder.create_named_block("dimension.incdec.ordinary", vec![]);
        let merge = ctx.builder.create_named_block("dimension.incdec.merge", vec![]);
        let initial = ctx.initialized_slots_snapshot();
        ctx.builder.terminate(Terminator::CondBr { cond: condition.value,
            then_target: native, then_args: vec![], else_target: ordinary, else_args: vec![] });
        ctx.builder.position_at_end(native);
        emit_native(ctx, array, index, increment, span);
        jump(ctx, merge);
        let native_slots = ctx.initialized_slots_snapshot();
        ctx.builder.position_at_end(ordinary);
        ctx.restore_initialized_slots(initial);
        let array = borrow(ctx, array, span);
        fallback(ctx, array);
        jump(ctx, merge);
        let ordinary_slots = ctx.initialized_slots_snapshot();
        ctx.builder.position_at_end(merge);
        ctx.restore_initialized_slots(native_slots.intersection(&ordinary_slots).copied().collect());
    }
    crate::ir_lower::expr::unpin_in_flight_owners(ctx, pins, span);
    if ctx.value_is_owning_temporary(array) {
        crate::ir_lower::ownership::release_if_owned(ctx, array, Some(span));
    }
    true
}

/// Owns the native receiver independently before a key can replace its original storage.
fn emit_native(
    ctx: &mut LoweringContext<'_, '_>, array: LoweredValue, index: &Expr,
    increment: bool, span: Span,
) {
    let array = borrow(ctx, array, span);
    let ty = ctx.builder.value_php_type(array.value);
    let owner = if matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        ctx.emit_owned_value(Op::MixedClone, vec![array.value], None, PhpType::Mixed,
            Op::MixedClone.default_effects(), Some(span))
    } else { crate::ir_lower::ownership::acquire_lifetime_pin_if_refcounted(ctx, array, Some(span)) };
    let lease = property_write_receiver::PropertyWriteReceiver::new(ctx, owner, span);
    let array = crate::ir_lower::expr::borrow_native_xml_property_receiver(ctx, lease.value, span);
    let key = lower_expr(ctx, index);
    super::property_array_writes::lower_native_incdec(ctx, array, key, increment, span);
    lease.finish(ctx, span);
}

/// Borrows the caller's owner without giving a fallback permission to consume it.
fn borrow(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, span: Span) -> LoweredValue {
    let ty = ctx.builder.value_php_type(value.value);
    let borrowed = ctx.emit_value(Op::Borrow, vec![value.value], None, ty,
        Op::Borrow.default_effects(), Some(span));
    ctx.builder.set_value_ownership(borrowed.value, Ownership::Borrowed);
    borrowed
}

/// Joins a completed update arm after its scoped owners have retired.
fn jump(ctx: &mut LoweringContext<'_, '_>, merge: BlockId) {
    ctx.builder.terminate(Terminator::Br { target: merge, args: vec![] });
}
