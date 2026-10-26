//! Purpose:
//! Applies SimpleXML's container emptiness protocol independently of boolean casts.
//!
//! Called from:
//! - Lazy empty lowering before ordinary property and ArrayAccess handling.
//!
//! Key details:
//! - Receiver and key are captured once, with PHP's dynamic-property evaluation order.
//! - Ordinary objects retain the existing empty path, including magic and uninitialized slots.
//! - Published hidden locals lend operands to callbacks and release them after the decision.

use super::*;

/// One captured operand and its unwind-visible owner, when its type needs one.
struct Capture { name: String, slot: crate::ir::LocalSlotId, rooted: bool }

/// Selects native empty handling only for a container access that can reach an XML object.
pub(super) fn lower(
    ctx: &mut LoweringContext<'_, '_>, construct: &str, arg: &Expr, expr: &Expr,
) -> Option<LoweredValue> {
    if !ctx.classes.contains_key("SimpleXMLElement") { return None; }
    let (object, key, dimension, nullsafe) = match &arg.kind {
        ExprKind::PropertyAccess { object, property } => {
            if property.starts_with("__elephc_") { return None; }
            (object.as_ref(), Expr::new(ExprKind::StringLiteral(property.clone()), arg.span), false, false)
        }
        ExprKind::DynamicPropertyAccess { object, property } => (object.as_ref(), *property.clone(), false, false),
        ExprKind::NullsafePropertyAccess { object, property } => {
            if property.starts_with("__elephc_") { return None; }
            (object.as_ref(), Expr::new(ExprKind::StringLiteral(property.clone()), arg.span), false, true)
        }
        ExprKind::NullsafeDynamicPropertyAccess { object, property } => (object.as_ref(), *property.clone(), false, true),
        ExprKind::ArrayAccess { array, index } => (array.as_ref(), *index.clone(), true, false),
        _ => return None,
    };
    let ty = array_cast_source::array_cast_source_type_for_ir(ctx, object);
    if !may_be_native(ctx, &ty) { return None; }
    let key_first = !nullsafe && matches!(object.kind, ExprKind::Variable(_))
        && (dimension || !matches!(key.kind, ExprKind::StringLiteral(_)));
    let early_key = key_first.then(|| capture_key(ctx, &key, dimension));
    let value = lower_subscript_receiver_silently(ctx, object);
    let receiver = capture(ctx, value, arg.span);
    let result = if nullsafe {
        let value = ctx.load_local(&receiver.name, Some(arg.span));
        let is_null = ctx.emit_value(Op::IsNull, vec![value.value], None, PhpType::Bool,
            Op::IsNull.default_effects(), Some(arg.span));
        let absent = ctx.builder.create_named_block("xml.empty.null", vec![]);
        let present = ctx.builder.create_named_block("xml.empty.nonnull", vec![]);
        let merge = ctx.builder.create_named_block("xml.empty.null.merge", vec![]);
        let temp = ctx.declare_hidden_temp(PhpType::Bool);
        let initial = ctx.initialized_slots_snapshot();
        ctx.builder.terminate(Terminator::CondBr { cond: is_null.value,
            then_target: absent, then_args: vec![], else_target: present, else_args: vec![] });
        ctx.builder.position_at_end(absent);
        let value = emit_bool_literal(ctx, true, Some(arg.span));
        store_value_into_temp(ctx, &temp, PhpType::Bool, value, arg.span);
        branch_to(ctx, merge);
        let absent_slots = ctx.initialized_slots_snapshot();
        ctx.builder.position_at_end(present);
        ctx.restore_initialized_slots(initial.clone());
        let key = capture_key(ctx, &key, dimension);
        let value = dispatch(ctx, &receiver, &key, dimension, construct, arg, expr);
        retire(ctx, key, arg.span);
        store_value_into_temp(ctx, &temp, PhpType::Bool, value, arg.span);
        branch_to(ctx, merge);
        let present_slots = ctx.initialized_slots_snapshot();
        ctx.builder.position_at_end(merge);
        ctx.restore_initialized_slots(merge_initialized_slots_for_expr(&initial,
            absent_slots, true, present_slots, true));
        take_owned_temp(ctx, &temp, arg.span)
    } else {
        let key = early_key.unwrap_or_else(|| capture_key(ctx, &key, dimension));
        let value = dispatch(ctx, &receiver, &key, dimension, construct, arg, expr);
        if key_first {
            retire(ctx, receiver, arg.span);
            retire(ctx, key, arg.span);
            return Some(value);
        }
        retire(ctx, key, arg.span);
        value
    };
    retire(ctx, receiver, arg.span);
    Some(result)
}

/// Probes a chain's final access while its receiver is still on the guarded continuation.
pub(super) fn lower_from_receiver(
    ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, key: &Expr,
    dimension: bool, arg: &Expr,
) -> LoweredValue {
    let receiver = capture(ctx, value, arg.span);
    let key = capture_key(ctx, key, dimension);
    let value = dispatch(ctx, &receiver, &key, dimension, "empty", arg, arg);
    retire(ctx, key, arg.span);
    retire(ctx, receiver, arg.span);
    value
}

/// Evaluates and normalizes one key before publishing its independent captured owner.
fn capture_key(ctx: &mut LoweringContext<'_, '_>, key: &Expr, dimension: bool) -> Capture {
    let value = lower_expr(ctx, key);
    let value = if dimension { value } else {
        property_access::coerce_runtime_property_name(ctx, value, key.span)
    };
    capture(ctx, value, key.span)
}

/// Holds a captured operand in a reusable hidden local, whose call-argument loads borrow it.
fn capture(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, span: Span) -> Capture {
    let ty = ctx.builder.value_php_type(value.value);
    let rooted = Ownership::php_type_needs_lifetime_tracking(&ty);
    let name = ctx.declare_hidden_temp(ty.clone());
    store_value_into_temp(ctx, &name, ty, value, span);
    let slot = ctx.local_slots[&name];
    if rooted { register_owned_call_operand(ctx, slot, span); }
    Capture { name, slot, rooted }
}

/// Detaches the capture's exception record before clearing and releasing its local slot.
fn retire(ctx: &mut LoweringContext<'_, '_>, captured: Capture, span: Span) {
    if captured.rooted { unregister_owned_call_operand(ctx, captured.slot, span); }
    ctx.emit_void(Op::ReleaseLocalSlot, vec![], Some(Immediate::LocalSlot(captured.slot)),
        Op::ReleaseLocalSlot.default_effects(), Some(span));
}

/// Joins native container emptiness with the existing ordinary-object behavior.
fn dispatch(
    ctx: &mut LoweringContext<'_, '_>, receiver: &Capture, key: &Capture,
    dimension: bool, construct: &str, arg: &Expr, expr: &Expr,
) -> LoweredValue {
    let object = ctx.load_local(&receiver.name, Some(arg.span));
    let ty = ctx.builder.value_php_type(object.value);
    if !may_be_native(ctx, &ty) {
        return lower_regular(ctx, receiver, key, dimension, construct, arg, expr);
    }
    if native_dimension_method_for_type(ctx, &ty,
        elephc_builtin_contract::ObjectDimension::Get).is_some()
    {
        let method = if dimension { "__elephc_dimension_empty" } else { "__elephc_property_empty" };
        let key_expr = Expr::new(ExprKind::Variable(key.name.clone()), arg.span);
        let nullable = value_is_nullable(ctx, object.value);
        let object = borrow_native_xml_property_receiver(ctx, object, arg.span);
        return call_known_native(ctx, object, method, key_expr, nullable, expr);
    }
    let class = ctx.intern_class_name("SimpleXMLElement");
    let condition = ctx.emit_value(Op::InstanceOf, vec![object.value], Some(Immediate::Data(class)),
        PhpType::Bool, Op::InstanceOf.default_effects(), Some(arg.span));
    let native = ctx.builder.create_named_block("xml.empty.native", vec![]);
    let regular = ctx.builder.create_named_block("xml.empty.fallback", vec![]);
    let merge = ctx.builder.create_named_block("xml.empty.merge", vec![]);
    let temp = ctx.declare_hidden_temp(PhpType::Bool);
    let initial = ctx.initialized_slots_snapshot();
    ctx.builder.terminate(Terminator::CondBr { cond: condition.value,
        then_target: native, then_args: vec![], else_target: regular, else_args: vec![] });
    ctx.builder.position_at_end(native);
    let object = if singular_object_class(&ty).is_some_and(|(class, _)|
        !crate::types::builtin_classes::has_native_dimensions(class, ctx.classes))
    {
        ctx.emit_value(Op::MixedBox, vec![object.value], None, PhpType::Mixed,
            Op::MixedBox.default_effects(), Some(arg.span))
    } else { borrow_native_xml_property_receiver(ctx, object, arg.span) };
    let method = if dimension { "__elephc_dimension_empty" } else { "__elephc_property_empty" };
    let key_expr = Expr::new(ExprKind::Variable(key.name.clone()), arg.span);
    let value = lower_method_call_with_receiver(ctx, object, method, &[key_expr], Op::MethodCall, expr);
    store_value_into_temp(ctx, &temp, PhpType::Bool, value, arg.span);
    branch_to(ctx, merge);
    let native_slots = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(regular);
    ctx.restore_initialized_slots(initial.clone());
    let value = lower_regular(ctx, receiver, key, dimension, construct, arg, expr);
    store_value_into_temp(ctx, &temp, PhpType::Bool, value, arg.span);
    branch_to(ctx, merge);
    let regular_slots = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(merge);
    ctx.restore_initialized_slots(merge_initialized_slots_for_expr(&initial,
        native_slots, true, regular_slots, true));
    take_owned_temp(ctx, &temp, arg.span)
}

/// Uses the native decision alone, with a boolean empty result on a real nullable receiver.
fn call_known_native(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, method: &str, key: Expr,
    nullable: bool, expr: &Expr,
) -> LoweredValue {
    if !nullable {
        return lower_method_call_with_receiver(ctx, object, method, &[key], Op::MethodCall, expr);
    }
    let absent = ctx.builder.create_named_block("xml.empty.native.null", vec![]);
    let present = ctx.builder.create_named_block("xml.empty.native.present", vec![]);
    let merge = ctx.builder.create_named_block("xml.empty.native.merge", vec![]);
    let temp = ctx.declare_hidden_temp(PhpType::Bool);
    let missing = ctx.emit_value(Op::IsNull, vec![object.value], None, PhpType::Bool,
        Op::IsNull.default_effects(), Some(expr.span));
    let initial = ctx.initialized_slots_snapshot();
    ctx.builder.terminate(Terminator::CondBr { cond: missing.value,
        then_target: absent, then_args: vec![], else_target: present, else_args: vec![] });
    ctx.builder.position_at_end(absent);
    let value = emit_bool_literal(ctx, true, Some(expr.span));
    store_value_into_temp(ctx, &temp, PhpType::Bool, value, expr.span);
    branch_to(ctx, merge);
    let absent_slots = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(present);
    ctx.restore_initialized_slots(initial.clone());
    let value = lower_method_call_with_receiver(ctx, object, method, &[key], Op::MethodCall, expr);
    store_value_into_temp(ctx, &temp, PhpType::Bool, value, expr.span);
    branch_to(ctx, merge);
    let present_slots = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(merge);
    ctx.restore_initialized_slots(merge_initialized_slots_for_expr(&initial,
        absent_slots, true, present_slots, true));
    take_owned_temp(ctx, &temp, expr.span)
}

/// Admits dynamic values, bare objects, and interfaces as well as concrete XML subclasses.
fn may_be_native(ctx: &LoweringContext<'_, '_>, ty: &PhpType) -> bool {
    matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
        || singular_object_class(ty).is_some_and(|(class, _)| class.is_empty()
            || ctx.interfaces.contains_key(class)
            || crate::types::builtin_classes::has_native_dimensions(class, ctx.classes))
}

/// Applies the ordinary property or dimension protocol without emitting an XML method call.
fn lower_regular(
    ctx: &mut LoweringContext<'_, '_>, receiver: &Capture, key: &Capture,
    dimension: bool, construct: &str, arg: &Expr, expr: &Expr,
) -> LoweredValue {
    let object = Box::new(Expr::new(ExprKind::Variable(receiver.name.clone()), arg.span));
    let key = Box::new(Expr::new(ExprKind::Variable(key.name.clone()), arg.span));
    let access = Expr::new(if dimension { ExprKind::ArrayAccess { array: object, index: key } }
        else { ExprKind::DynamicPropertyAccess { object, property: key } }, arg.span);
    lazy_isset::lower_lazy_empty_without_native(ctx, construct, &[access.clone()], expr)
        .unwrap_or_else(|| {
            let value = lower_expr(ctx, &access);
            lazy_isset::lower_empty_of_probed_value(ctx, value, construct, expr)
        })
}
