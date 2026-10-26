//! Purpose:
//! Selects native SimpleXML or public ArrayAccess dimension probes for object unions.
//!
//! Called from:
//! - Lazy isset and object dimension unset lowering.
//!
//! Key details:
//! - Receiver and key are evaluated once and rooted before nullable dispatch.
//! - Native adapters remain independent of public ArrayAccess method names.

use super::*;
use elephc_builtin_contract::ObjectDimension;

/// Captures dimension operands and retires their owners after the selected protocol completes.
pub(super) fn lower(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, source_type: &PhpType,
    index: &Expr, operation: ObjectDimension, expr: &Expr,
) -> LoweredValue {
    let (object, owner) = root_owned_call_operand(ctx, object, expr.span);
    // Global storage erases object metadata to Mixed; retain the source type for
    // protocol selection while the captured value keeps its real boxed representation.
    let ty = ctx.builder.value_php_type(object.value);
    let borrowed = if Ownership::php_type_needs_lifetime_tracking(&ty) {
        // Object unions use boxed storage and the dynamic method-call ABI.
        let borrow_type = if matches!(ty, PhpType::Union(_)) { PhpType::Mixed } else { ty };
        let borrowed = ctx.emit_value(Op::Borrow, vec![object.value], None, borrow_type,
            Op::Borrow.default_effects(), Some(expr.span));
        ctx.builder.set_value_ownership(borrowed.value, Ownership::Borrowed);
        borrowed
    } else { object };
    let (key, key_temp) = evaluate_once(ctx, index, expr);
    let key_owner = key_temp.as_ref().filter(|name|
        Ownership::php_type_needs_lifetime_tracking(&ctx.local_type(name)))
        .map(|name| ctx.local_slots[name]);
    if let Some(slot) = key_owner { register_owned_call_operand(ctx, slot, expr.span); }
    let native = native_dimension_method_for_type(ctx, source_type, operation);
    let public = match operation {
        ObjectDimension::Isset => "offsetExists",
        ObjectDimension::Unset => "offsetUnset",
        _ => unreachable!("dimension probe operation"),
    };
    let result = if native.is_none() && contains_xml(ctx, source_type) {
        dispatch(ctx, borrowed, &key, operation, public, expr)
    } else {
        invoke(ctx, borrowed, &key, operation, native.unwrap_or(public), expr)
    };
    if let Some(slot) = key_owner { unregister_owned_call_operand(ctx, slot, expr.span); }
    release_evaluated_once(ctx, key_temp, expr.span);
    if let Some(slot) = owner { retire_owned_call_operand(ctx, slot, expr.span); }
    result
}

/// Detects a native XML arm that cannot share the ordinary ArrayAccess method name.
fn contains_xml(ctx: &LoweringContext<'_, '_>, ty: &PhpType) -> bool {
    match ty {
        PhpType::Mixed => ctx.classes.contains_key("SimpleXMLElement"),
        PhpType::Object(class) => crate::types::builtin_classes::native_dimension_method(
            class, ObjectDimension::Get, ctx.classes) == Some("__elephc_dimension_get"),
        PhpType::Union(members) => members.iter().any(|ty| contains_xml(ctx, ty)),
        _ => false,
    }
}

/// Guards heterogeneous object unions without compiling a public offset method on XML.
fn dispatch(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, key: &Expr,
    operation: ObjectDimension, public: &str, expr: &Expr,
) -> LoweredValue {
    let class = ctx.intern_class_name("SimpleXMLElement");
    let native = ctx.emit_value(Op::InstanceOf, vec![object.value], Some(Immediate::Data(class)),
        PhpType::Bool, Op::InstanceOf.default_effects(), Some(expr.span));
    let yes = ctx.builder.create_named_block("dimension.probe.native", vec![]);
    let no = ctx.builder.create_named_block("dimension.probe.public", vec![]);
    let merge = ctx.builder.create_named_block("dimension.probe.merge", vec![]);
    let temp = ctx.declare_hidden_temp(PhpType::Bool);
    let initial = ctx.initialized_slots_snapshot();
    ctx.builder.terminate(Terminator::CondBr { cond: native.value,
        then_target: yes, then_args: vec![], else_target: no, else_args: vec![] });
    ctx.builder.position_at_end(yes);
    let receiver = borrow_native_xml_property_receiver(ctx, object, expr.span);
    let method = match operation {
        ObjectDimension::Isset => "__elephc_dimension_isset",
        ObjectDimension::Unset => "__elephc_dimension_unset",
        _ => unreachable!("dimension probe operation"),
    };
    let value = invoke(ctx, receiver, key, operation, method, expr);
    store_value_into_temp(ctx, &temp, PhpType::Bool, value, expr.span);
    branch_to(ctx, merge);
    let native_slots = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(no);
    ctx.restore_initialized_slots(initial.clone());
    let value = invoke(ctx, object, key, operation, public, expr);
    store_value_into_temp(ctx, &temp, PhpType::Bool, value, expr.span);
    branch_to(ctx, merge);
    let public_slots = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(merge);
    ctx.restore_initialized_slots(merge_initialized_slots_for_expr(&initial,
        native_slots, true, public_slots, true));
    take_owned_temp(ctx, &temp, expr.span)
}

/// Calls one protocol with an already captured key and a false result for nullable absence.
fn invoke(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, key: &Expr,
    operation: ObjectDimension, method: &str, expr: &Expr,
) -> LoweredValue {
    let op = if value_is_nullable(ctx, object.value) { Op::NullsafeMethodCall }
        else { Op::MethodCall };
    let result = lower_method_call_with_receiver(ctx, object, method, &[key.clone()], op, expr);
    if operation == ObjectDimension::Isset { ctx.truthy_consuming(result, Some(expr.span)) }
    else {
        crate::ir_lower::ownership::release_if_owned(ctx, result, Some(expr.span));
        emit_bool_literal(ctx, false, Some(expr.span))
    }
}
