//! Purpose:
//! Preserves property setters when a boxed receiver has lost its concrete PHP class.
//!
//! Called from:
//! - Instance property assignment after the native XML and statically known hook paths.
//!
//! Key details:
//! - Runtime guards select ordinary typed method calls without reevaluating receiver or RHS.
//! - Inaccessible descendant hooks retain their storage error instead of reaching a parent hook.
//! - The caller owns the common receiver; each selected arm retires its own argument lease.

use super::*;

/// Dispatches a boxed ordinary property write through the runtime class's setter hook.
pub(super) fn lower(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, property: &str,
    value: LoweredValue, preserve_property_operand: bool, span: Span,
) -> bool {
    let ty = ctx.builder.value_php_type(object.value);
    if !matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
        || crate::ir_lower::expr::singular_object_class(&ty).is_some()
        || ctx.in_own_property_accessor(property) { return false; }
    let method = property_hook_set_method(property);
    let key = php_symbol_key(&method);
    let mut classes = ctx.classes.iter().filter_map(|(name, info)|
        info.methods.contains_key(&key).then(|| name.clone())).collect::<Vec<_>>();
    classes.sort_by_key(|name| std::cmp::Reverse(ctx.classes[name].class_id));
    if classes.is_empty() { return false; }
    dispatch(ctx, object, property, value, &method, &classes, preserve_property_operand, span);
    true
}

/// Preserves write visibility and readonly errors before entering an accessor body.
fn accessible(ctx: &LoweringContext<'_, '_>, class: &str, property: &str) -> bool {
    let info = &ctx.classes[class];
    let declaring = info.property_declaring_classes.get(property)
        .map(String::as_str).unwrap_or(class);
    crate::ir_lower::expr::property_is_accessible_for_ir(ctx, class, info, property)
        && !info.readonly_properties.contains(property)
        && info.property_set_visibilities.get(property).is_none_or(|visibility|
            crate::ir_lower::expr::ir_can_access_member(ctx, declaring, visibility))
}

/// Calls one guarded setter or preserves the existing physical storage fallback.
fn dispatch(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, property: &str,
    value: LoweredValue, method: &str, classes: &[String], preserve_property_operand: bool, span: Span,
) {
    let Some((class, rest)) = classes.split_first() else {
        let retained = receiver_retains_abstract_setter(ctx, object.value, property);
        lower_property_slot_assign(ctx, object, property, value, preserve_property_operand, retained, span);
        return;
    };
    let data = ctx.intern_class_name(class);
    let condition = ctx.emit_value(Op::InstanceOf, vec![object.value], Some(Immediate::Data(data)),
        PhpType::Bool, Op::InstanceOf.default_effects(), Some(span));
    let yes = ctx.builder.create_named_block("property.setter", vec![]);
    let no = ctx.builder.create_named_block("property.setter.next", vec![]);
    let done = ctx.builder.create_named_block("property.setter.done", vec![]);
    let initial = ctx.initialized_slots_snapshot();
    ctx.builder.terminate(Terminator::CondBr { cond: condition.value,
        then_target: yes, then_args: vec![], else_target: no, else_args: vec![] });
    ctx.builder.position_at_end(yes);
    if accessible(ctx, class, property) {
        emit_setter(ctx, object, value, class, method, span);
    } else {
        let retained = receiver_retains_abstract_setter(ctx, object.value, property);
        lower_property_slot_assign(ctx, object, property, value, preserve_property_operand, retained, span);
    }
    branch_to(ctx, done);
    let yes_slots = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(no);
    ctx.restore_initialized_slots(initial.clone());
    dispatch(ctx, object, property, value, method, rest, preserve_property_operand, span);
    branch_to(ctx, done);
    let no_slots = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(done);
    ctx.restore_initialized_slots(merge_initialized_slots(&initial, yes_slots, true, no_slots, true));
}

/// Extracts a typed receiver lease and lets the ordinary call ABI coerce the boxed RHS.
fn emit_setter(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, value: LoweredValue,
    class: &str, method: &str, span: Span,
) {
    let ty = PhpType::Object(class.to_string());
    let receiver = ctx.emit_owned_value(Op::MixedUnbox, vec![object.value], None, ty.clone(),
        Op::mixed_unbox_effects(&ty), Some(span));
    let (receiver, owner) = crate::ir_lower::expr::root_owned_call_operand(ctx, receiver, span);
    let signature = ctx.classes[class].methods.get(&php_symbol_key(method)).cloned();
    let expression = Expr::new(ExprKind::Null, span);
    let mut operands = vec![receiver.value, value.value];
    let extra = crate::ir_lower::expr::object_operators::append_hidden_arguments(
        ctx, signature.as_ref(), &mut operands, 1, &expression,
    );
    let pins = crate::ir_lower::expr::pin_in_flight_owners(ctx, &[value.value], span);
    let data = ctx.intern_string(method);
    ctx.emit_void(Op::MethodCall, operands, Some(Immediate::Data(data)),
        Op::MethodCall.default_effects(), Some(span));
    crate::ir_lower::expr::unpin_in_flight_owners(ctx, pins, span);
    for argument in extra { crate::ir_lower::ownership::release_if_owned(ctx, argument, Some(span)); }
    release_magic_set_value_after_call(ctx, value, span);
    if let Some(owner) = owner { crate::ir_lower::expr::retire_owned_call_operand(ctx, owner, span); }
}
