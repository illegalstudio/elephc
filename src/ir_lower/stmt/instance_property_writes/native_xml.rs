//! Purpose:
//! Dispatches native XML property writes before PHP slots and user magic methods.
//!
//! Called from:
//! - Instance-property assignment lowering after receiver and RHS evaluation.
//!
//! Key details:
//! - A boxed receiver is tested once, and both branches share the existing receiver lease.
//! - Native arguments are boxed, pinned during the call, and retired independently.

use super::*;

/// Handles typed and boxed native XML receivers, returning false for ordinary typed objects.
pub(super) fn lower(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, property: &str,
    value_expr: &Expr, value: LoweredValue, preserve_property_operand: bool, span: Span,
) -> bool {
    if property.starts_with("__elephc_") { return false; }
    let ty = ctx.builder.value_php_type(object.value);
    if let PhpType::Object(class) = ty.codegen_repr() {
        let Some(method) = crate::types::builtin_classes::native_property_method(&class, property,
            elephc_builtin_contract::ObjectProperty::Set, ctx.classes) else { return false; };
        emit_set(ctx, object, property, value, method, span);
        return true;
    }
    if !matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
        || !ctx.classes.contains_key("SimpleXMLElement") { return false; }
    let class = ctx.intern_class_name("SimpleXMLElement");
    let condition = ctx.emit_value(Op::InstanceOf, vec![object.value], Some(Immediate::Data(class)),
        PhpType::Bool, Op::InstanceOf.default_effects(), Some(span));
    let native = ctx.builder.create_named_block("xml.property.set", vec![]);
    let regular = ctx.builder.create_named_block("xml.property.set.fallback", vec![]);
    let merge = ctx.builder.create_named_block("xml.property.set.merge", vec![]);
    let initial = ctx.initialized_slots_snapshot();
    ctx.builder.terminate(Terminator::CondBr { cond: condition.value,
        then_target: native, then_args: vec![], else_target: regular, else_args: vec![] });
    ctx.builder.position_at_end(native);
    emit_set(ctx, object, property, value, "__elephc_property_set", span);
    branch_to(ctx, merge);
    let native_initialized = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(regular);
    ctx.restore_initialized_slots(initial.clone());
    lower_property_assign_without_native(ctx, object, property, value_expr, value,
        preserve_property_operand, span);
    branch_to(ctx, merge);
    let regular_initialized = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(merge);
    ctx.restore_initialized_slots(merge_initialized_slots(&initial,
        native_initialized, true, regular_initialized, true));
    true
}

/// Calls the reviewed native setter with a borrowed receiver and an independent boxed value.
fn emit_set(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, property: &str,
    value: LoweredValue, method: &str, span: Span,
) {
    let data = ctx.intern_string(property);
    let name = ctx.emit_value(Op::ConstStr, vec![], Some(Immediate::Data(data)), PhpType::Str,
        Op::ConstStr.default_effects(), Some(span));
    emit_set_name(ctx, object, name, value, method, span);
}

/// Emits the native setter for a property name whose PHP evaluation is already complete.
fn emit_set_name(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, name: LoweredValue,
    value: LoweredValue, method: &str, span: Span,
) {
    let value = ctx.box_value_as_mixed(value, PhpType::Mixed, Some(span));
    let signature = ctx.classes.get("SimpleXMLElement")
        .and_then(|info| info.methods.get(&php_symbol_key(method))).cloned();
    let expression = Expr::new(ExprKind::Null, span);
    let mut operands = vec![object.value, name.value, value.value];
    let extra = crate::ir_lower::expr::object_operators::append_hidden_arguments(
        ctx, signature.as_ref(), &mut operands, 2, &expression,
    );
    let pins = crate::ir_lower::expr::pin_in_flight_owners(ctx, &operands, span);
    let method = ctx.intern_string(method);
    ctx.emit_void(Op::MethodCall, operands,
        Some(Immediate::Data(method)), Op::MethodCall.default_effects(), Some(span));
    crate::ir_lower::expr::unpin_in_flight_owners(ctx, pins, span);
    for value in extra { crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span)); }
    crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
}

/// Dispatches a runtime-name write while its caller owns receiver, name, and value cleanup.
pub(super) fn lower_dynamic(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, name: LoweredValue,
    value: LoweredValue, span: Span,
) -> bool {
    let ty = ctx.builder.value_php_type(object.value);
    if let PhpType::Object(class) = ty.codegen_repr() {
        let Some(method) = crate::types::builtin_classes::native_property_method(&class, "",
            elephc_builtin_contract::ObjectProperty::Set, ctx.classes) else { return false; };
        emit_set_name(ctx, object, name, value, method, span);
        return true;
    }
    if !matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
        || !ctx.classes.contains_key("SimpleXMLElement") { return false; }
    let class = ctx.intern_class_name("SimpleXMLElement");
    let condition = ctx.emit_value(Op::InstanceOf, vec![object.value], Some(Immediate::Data(class)),
        PhpType::Bool, Op::InstanceOf.default_effects(), Some(span));
    let native = ctx.builder.create_named_block("xml.dynamic.set", vec![]);
    let regular = ctx.builder.create_named_block("xml.dynamic.set.fallback", vec![]);
    let merge = ctx.builder.create_named_block("xml.dynamic.set.merge", vec![]);
    let initial = ctx.initialized_slots_snapshot();
    ctx.builder.terminate(Terminator::CondBr { cond: condition.value,
        then_target: native, then_args: vec![], else_target: regular, else_args: vec![] });
    ctx.builder.position_at_end(native);
    emit_set_name(ctx, object, name, value, "__elephc_property_set", span);
    branch_to(ctx, merge);
    let native_initialized = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(regular);
    ctx.restore_initialized_slots(initial.clone());
    ctx.emit_void(Op::DynamicPropSet, vec![object.value, name.value, value.value], None,
        Op::DynamicPropSet.default_effects(), Some(span));
    branch_to(ctx, merge);
    let regular_initialized = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(merge);
    ctx.restore_initialized_slots(merge_initialized_slots(&initial,
        native_initialized, true, regular_initialized, true));
    true
}
