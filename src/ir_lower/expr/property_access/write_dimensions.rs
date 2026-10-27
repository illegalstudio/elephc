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
    let ty = ctx.builder.value_php_type(object.value);
    let may_be_native = matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
        || is_opaque_property_object(ctx, &ty)
        || native_dimension_method_for_type(ctx, &ty,
            elephc_builtin_contract::ObjectDimension::WriteSelect).is_some();
    if !ctx.classes.contains_key("SimpleXMLElement") || !may_be_native {
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
            "__elephc_dimension_select", &[key.clone()], Op::MethodCall, expr),
        |ctx, receiver| lower_array_access_from_lowered_receiver(ctx, receiver, &key, expr),
    );
    stage_call_result(ctx, staging.as_ref(), result, expr.span);
    if let Some(slot) = key_owner { unregister_owned_call_operand(ctx, slot, expr.span); }
    release_evaluated_once(ctx, key_temp, expr.span);
    if let Some(slot) = owner { retire_owned_call_operand(ctx, slot, expr.span); }
    take_prepublished_call_result(ctx, staging, result, expr.span)
}
