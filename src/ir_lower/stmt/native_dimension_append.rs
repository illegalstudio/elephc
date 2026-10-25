//! Purpose:
//! Preserves the native XML distinction between append and an explicit null offset.
//!
//! Called from:
//! - Local, static, property and nested dimension append lowering.
//!
//! Key details:
//! - Ordinary receivers retain their caller-provided append operation.
//! - Native calls borrow independently rooted receiver and value storage on every target.

use super::*;

/// Emits native append when possible, otherwise leaves or invokes the ordinary caller path.
pub(super) fn lower(
    ctx: &mut LoweringContext<'_, '_>, array: LoweredValue, value: LoweredValue, span: Span,
    fallback: impl FnOnce(&mut LoweringContext<'_, '_>, LoweredValue, LoweredValue),
) -> bool {
    if !ctx.classes.contains_key("SimpleXMLElement") { return false; }
    let ty = ctx.builder.value_php_type(array.value);
    if ty.is_php_array() { return false; }
    let known = crate::ir_lower::expr::singular_object_class(&ty).is_some_and(|(class, _)|
        crate::types::builtin_classes::has_native_dimensions(class, ctx.classes));
    let opaque = matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
        || matches!(ty.codegen_repr(), PhpType::Object(class)
            if class.is_empty() || ctx.interfaces.contains_key(&class));
    if !known && !opaque { return false; }
    let pins = crate::ir_lower::expr::pin_in_flight_owners(ctx, &[array.value, value.value], span);
    if known {
        emit_native(ctx, array, value, span);
    } else {
        let class = ctx.intern_class_name("SimpleXMLElement");
        let native = ctx.emit_value(Op::InstanceOf, vec![array.value], Some(Immediate::Data(class)),
            PhpType::Bool, Op::InstanceOf.default_effects(), Some(span));
        let yes = ctx.builder.create_named_block("dimension.append.native", vec![]);
        let no = ctx.builder.create_named_block("dimension.append.ordinary", vec![]);
        let done = ctx.builder.create_named_block("dimension.append.done", vec![]);
        let initial = ctx.initialized_slots_snapshot();
        ctx.builder.terminate(Terminator::CondBr { cond: native.value,
            then_target: yes, then_args: vec![], else_target: no, else_args: vec![] });
        ctx.builder.position_at_end(yes);
        emit_native(ctx, array, value, span);
        ctx.builder.terminate(Terminator::Br { target: done, args: vec![] });
        let native_slots = ctx.initialized_slots_snapshot();
        ctx.builder.position_at_end(no);
        ctx.restore_initialized_slots(initial);
        let array = borrow(ctx, array, span);
        let value = borrow(ctx, value, span);
        fallback(ctx, array, value);
        ctx.builder.terminate(Terminator::Br { target: done, args: vec![] });
        let ordinary_slots = ctx.initialized_slots_snapshot();
        ctx.builder.position_at_end(done);
        ctx.restore_initialized_slots(native_slots.intersection(&ordinary_slots).copied().collect());
    }
    crate::ir_lower::expr::unpin_in_flight_owners(ctx, pins, span);
    true
}

/// Roots an independent native receiver and any argument box across PHP exceptions.
fn emit_native(ctx: &mut LoweringContext<'_, '_>, array: LoweredValue, value: LoweredValue, span: Span) {
    let array = borrow(ctx, array, span);
    let ty = ctx.builder.value_php_type(array.value);
    let owner = if matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        ctx.emit_owned_value(Op::MixedClone, vec![array.value], None, PhpType::Mixed,
            Op::MixedClone.default_effects(), Some(span))
    } else { crate::ir_lower::ownership::acquire_lifetime_pin_if_refcounted(ctx, array, Some(span)) };
    let lease = property_write_receiver::PropertyWriteReceiver::new(ctx, owner, span);
    let receiver = crate::ir_lower::expr::borrow_native_xml_property_receiver(ctx, lease.value, span);
    let source = borrow(ctx, value, span);
    let boxed = ctx.box_value_as_mixed(source, PhpType::Mixed, Some(span));
    let signature = ctx.classes.get("SimpleXMLElement")
        .and_then(|class| class.methods.get(&php_symbol_key("__elephc_append"))).cloned();
    let mut operands = vec![receiver.value, boxed.value];
    let expression = Expr::new(ExprKind::Null, span);
    let extra = crate::ir_lower::expr::append_hidden_arguments(ctx, signature.as_ref(), &mut operands, 1, &expression);
    let pins = crate::ir_lower::expr::pin_in_flight_owners(ctx, &operands, span);
    let method = ctx.intern_string("__elephc_append");
    ctx.emit_void(Op::MethodCall, operands, Some(Immediate::Data(method)),
        Op::MethodCall.default_effects(), Some(span));
    crate::ir_lower::expr::unpin_in_flight_owners(ctx, pins, span);
    for value in extra { crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span)); }
    crate::ir_lower::ownership::release_if_owned(ctx, boxed, Some(span));
    lease.finish(ctx, span);
}

/// Lends original operands without transferring their caller-owned cleanup responsibility.
fn borrow(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, span: Span) -> LoweredValue {
    let ty = ctx.builder.value_php_type(value.value);
    let borrowed = ctx.emit_value(Op::Borrow, vec![value.value], None, ty,
        Op::Borrow.default_effects(), Some(span));
    ctx.builder.set_value_ownership(borrowed.value, Ownership::Borrowed);
    borrowed
}
