//! Purpose:
//! Select writable XML offsets while preserving ordinary array and ArrayAccess reads.
//!
//! Called from:
//! - Property write receiver chains and deferred static receiver traversal.
//!
//! Key details:
//! - Receiver and index owners remain visible throughout callbacks and diagnostics.
//! - Normal dimension reads keep their nullable getter.

use super::*;

/// Uses the native writable offset adapter only for receivers that can contain SimpleXML.
pub(crate) fn lower_native_dimension_write_select(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, index: &Expr, expr: &Expr,
) -> LoweredValue {
    lower_dimension_select(ctx, object, index, expr, true)
}

/// Admits only opaque receiver storage or classes whose dimensions can use native XML adapters.
pub(crate) fn may_have_native_xml_dimensions(ctx: &LoweringContext<'_, '_>, ty: &PhpType) -> bool {
    ctx.classes.contains_key("SimpleXMLElement") &&
        (matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
            || is_opaque_property_object(ctx, ty)
            || native_dimension_method_for_type(ctx, ty,
                elephc_builtin_contract::ObjectDimension::WriteSelect).is_some())
}

/// Distinguishes a dereferencing write from incdec's non-throwing native scalar conversion.
fn lower_dimension_select(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, index: &Expr, expr: &Expr,
    throw_on_failure: bool,
) -> LoweredValue {
    let ty = ctx.builder.value_php_type(object.value);
    if !may_have_native_xml_dimensions(ctx, &ty) {
        return lower_array_access_from_lowered_receiver(ctx, object, index, expr);
    }
    let staging = prepublish_call_result(ctx, &PhpType::Mixed, expr.span);
    let (object, owner) = root_owned_call_operand(ctx, object, expr.span);
    let borrowed = ctx.emit_value(Op::Borrow, vec![object.value], None, ty,
        Op::Borrow.default_effects(), Some(expr.span));
    ctx.builder.set_value_ownership(borrowed.value, Ownership::Borrowed);
    let (key, key_temp) = evaluate_once(ctx, index, expr);
    let key_owner = key_temp.as_ref().filter(|name|
        Ownership::php_type_needs_lifetime_tracking(&ctx.local_type(name)))
        .map(|name| ctx.local_slots[name]);
    if let Some(slot) = key_owner { register_owned_call_operand(ctx, slot, expr.span); }
    let result = native_xml_reads::dispatch(
        ctx, borrowed, expr, PhpType::Mixed, false,
        |ctx, receiver| lower_method_call_with_receiver(ctx, receiver,
            "__elephc_dimension_select", &[key.clone(), Expr::new(ExprKind::BoolLiteral(throw_on_failure), expr.span)], Op::MethodCall, expr),
        |ctx, receiver| lower_array_access_from_lowered_receiver(ctx, receiver, &key, expr),
    );
    stage_call_result(ctx, staging.as_ref(), result, expr.span);
    if let Some(slot) = key_owner { unregister_owned_call_operand(ctx, slot, expr.span); }
    release_evaluated_once(ctx, key_temp, expr.span);
    if let Some(slot) = owner { retire_owned_call_operand(ctx, slot, expr.span); }
    take_prepublished_call_result(ctx, staging, result, expr.span)
}

/// Selects an incdec offset with a captured key and non-throwing native conversion semantics.
pub(crate) fn lower_native_dimension_incdec_select_from_values(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, index: LoweredValue, expr: &Expr,
) -> LoweredValue {
    let staging = prepublish_call_result(ctx, &PhpType::Mixed, expr.span);
    let ty = ctx.builder.value_php_type(index.value);
    let name = ctx.declare_owned_hidden_temp(ty.clone());
    let slot = ctx.local_slots[&name];
    let rooted = Ownership::php_type_needs_lifetime_tracking(&ty);
    if rooted { register_owned_call_operand(ctx, slot, expr.span); }
    store_value_into_temp(ctx, &name, ty, index, expr.span);
    let key = Expr::new(ExprKind::Variable(name), expr.span);
    let result = lower_dimension_select(ctx, object, &key, expr, false);
    stage_call_result(ctx, staging.as_ref(), result, expr.span);
    if rooted { unregister_owned_call_operand(ctx, slot, expr.span); }
    ctx.emit_void(Op::ReleaseLocalSlot, vec![], Some(Immediate::LocalSlot(slot)),
        Op::ReleaseLocalSlot.default_effects(), Some(expr.span));
    take_prepublished_call_result(ctx, staging, result, expr.span)
}
