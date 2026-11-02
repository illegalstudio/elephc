//! Purpose:
//! Preserves property setters when a receiver has lost its concrete PHP subclass.
//!
//! Called from:
//! - Instance property assignment after the native XML and statically known hook paths.
//!
//! Key details:
//! - Runtime guards select ordinary typed method calls without reevaluating receiver or RHS.
//! - Exact classes leave eval-only subclass overrides on the ordinary runtime dispatcher.
//! - Inaccessible descendant hooks retain their storage error instead of reaching a parent hook.
//! - The caller owns the common receiver; each selected arm retires its own argument lease.

use super::*;
use crate::ir::{PhpTypePredicate, RuntimeCallTarget, RuntimeFnId};
use crate::parser::ast::Visibility;

/// Routes runtime property names through the literal setter dispatcher after XML is excluded.
pub(super) fn lower_dynamic(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, name: LoweredValue,
    value: LoweredValue, span: Span,
) {
    let mut properties = std::collections::BTreeSet::new();
    for info in ctx.classes.values() {
        for property in info.property_hooks.keys() {
            if !ctx.in_own_property_accessor(property)
                && info.methods.contains_key(&php_symbol_key(&property_hook_set_method(property))) {
                properties.insert(property.clone());
            }
        }
    }
    if properties.is_empty() {
        ctx.emit_void(Op::DynamicPropSet, vec![object.value, name.value, value.value], None,
            Op::DynamicPropSet.default_effects(), Some(span));
        return;
    }
    let done = ctx.builder.create_named_block("property.dynamic.setter.done", vec![]);
    let mut merged: Option<HashSet<LocalSlotId>> = None;
    for property in properties {
        let data = ctx.intern_string(&property);
        let expected = ctx.emit_value(Op::ConstStr, vec![], Some(Immediate::Data(data)), PhpType::Str,
            Op::ConstStr.default_effects(), Some(span));
        let condition = ctx.emit_value(Op::StrictEq, vec![name.value, expected.value], None,
            PhpType::Bool, Op::StrictEq.default_effects(), Some(span));
        let yes = ctx.builder.create_named_block("property.dynamic.setter", vec![]);
        let next = ctx.builder.create_named_block("property.dynamic.setter.next", vec![]);
        let initial = ctx.initialized_slots_snapshot();
        ctx.builder.terminate(Terminator::CondBr { cond: condition.value,
            then_target: yes, then_args: vec![], else_target: next, else_args: vec![] });
        ctx.builder.position_at_end(yes);
        let receiver = borrow_dynamic_operand(ctx, object, span);
        let assigned = borrow_dynamic_operand(ctx, value, span);
        if !lower(ctx, receiver, &property, assigned, true, span) {
            ctx.emit_void(Op::DynamicPropSet, vec![receiver.value, name.value, assigned.value], None,
                Op::DynamicPropSet.default_effects(), Some(span));
        }
        let state = ctx.initialized_slots_snapshot();
        merged = Some(match merged.take() { Some(previous) => previous.intersection(&state).copied().collect(),
            None => state });
        branch_to(ctx, done);
        ctx.builder.position_at_end(next);
        ctx.restore_initialized_slots(initial);
    }
    ctx.emit_void(Op::DynamicPropSet, vec![object.value, name.value, value.value], None,
        Op::DynamicPropSet.default_effects(), Some(span));
    let fallback = ctx.initialized_slots_snapshot();
    branch_to(ctx, done);
    ctx.builder.position_at_end(done);
    ctx.restore_initialized_slots(merged.expect("a setter name was selected")
        .intersection(&fallback).copied().collect());
}

/// Lends the caller's existing operand lease to one mutually exclusive setter branch.
fn borrow_dynamic_operand(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, span: Span) -> LoweredValue {
    let ty = ctx.builder.value_php_type(value.value);
    if !Ownership::php_type_needs_lifetime_tracking(&ty) { return value; }
    let value = ctx.emit_value(Op::Borrow, vec![value.value], None, ty,
        Op::Borrow.default_effects(), Some(span));
    ctx.builder.set_value_ownership(value.value, Ownership::Borrowed);
    value
}

/// Dispatches an ordinary property write through a setter absent from its static class.
pub(super) fn lower(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, property: &str,
    value: LoweredValue, preserve_property_operand: bool, span: Span,
) -> bool {
    let ty = ctx.builder.value_php_type(object.value);
    if !matches!(ty.codegen_repr(), PhpType::Object(_) | PhpType::Mixed | PhpType::Union(_))
        || ctx.in_own_property_accessor(property) { return false; }
    let method = property_hook_set_method(property);
    let key = php_symbol_key(&method);
    let base = crate::ir_lower::expr::singular_object_class(&ty).map(|(class, _)| class)
        .filter(|class| !class.is_empty() && !ctx.interfaces.contains_key(*class));
    let mut classes = ctx.classes.iter().filter_map(|(name, info)|
        (info.methods.contains_key(&key) && base.is_none_or(|base|
            crate::ir_lower::expr::class_extends_class(ctx, name, base)))
            .then(|| name.clone())).collect::<Vec<_>>();
    classes.sort_by_key(|name| std::cmp::Reverse(ctx.classes[name].class_id));
    if classes.is_empty() { return false; }
    let select = ctx.builder.create_named_block("property.setter.select", vec![]);
    let fallback = ctx.builder.create_named_block("property.setter.nonobject", vec![]);
    let done = ctx.builder.create_named_block("property.setter.merge", vec![]);
    let initial = ctx.initialized_slots_snapshot();
    let condition = ctx.emit_value(Op::TypePredicate, vec![object.value],
        Some(Immediate::TypePredicate(PhpTypePredicate::Object)), PhpType::Bool,
        Op::TypePredicate.default_effects(), Some(span));
    ctx.builder.terminate(Terminator::CondBr { cond: condition.value,
        then_target: select, then_args: vec![], else_target: fallback, else_args: vec![] });
    ctx.builder.position_at_end(select);
    let target = RuntimeFnId::GetClass;
    let name = ctx.emit_value(Op::RuntimeCall, vec![object.value],
        Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(target))),
        PhpType::Str, target.effects(), Some(span));
    let (name, owner) = crate::ir_lower::expr::root_owned_call_operand(ctx, name, span);
    dispatch(ctx, object, property, value, name, &method, &classes, span);
    if let Some(owner) = owner { crate::ir_lower::expr::retire_owned_call_operand(ctx, owner, span); }
    branch_to(ctx, done);
    let selected_slots = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(fallback);
    ctx.restore_initialized_slots(initial.clone());
    let retained = receiver_retains_abstract_setter(ctx, object.value, property);
    fallback_slot_assign(ctx, object, property, value, preserve_property_operand, retained, span);
    branch_to(ctx, done);
    let fallback_slots = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(done);
    ctx.restore_initialized_slots(merge_initialized_slots(&initial, selected_slots, true, fallback_slots, true));
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

/// Rejects an inaccessible asymmetric setter before the public read slot could accept the write.
fn asymmetric_error(ctx: &LoweringContext<'_, '_>, class: &str, property: &str) -> Option<String> {
    let info = &ctx.classes[class];
    if !crate::ir_lower::expr::property_is_accessible_for_ir(ctx, class, info, property) { return None; }
    let visibility = info.property_set_visibilities.get(property)?;
    let declaring = info.property_declaring_classes.get(property).map(String::as_str).unwrap_or(class);
    if crate::ir_lower::expr::ir_can_access_member(ctx, declaring, visibility) { return None; }
    let visibility = match visibility {
        Visibility::Private => "private", Visibility::Protected => "protected", Visibility::Public => return None,
    };
    let scope = ctx.current_class.as_ref().map_or_else(|| "global scope".to_string(), |class| format!("scope {class}"));
    Some(format!("Cannot modify {visibility}(set) property {declaring}::${property} from {scope}"))
}

/// Calls one guarded setter or preserves the existing physical storage fallback.
fn dispatch(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, property: &str,
    value: LoweredValue, class_name: LoweredValue, method: &str,
    classes: &[String], span: Span,
) {
    let Some((class, rest)) = classes.split_first() else {
        let retained = receiver_retains_abstract_setter(ctx, object.value, property);
        fallback_slot_assign(ctx, object, property, value, true, retained, span);
        return;
    };
    let data = ctx.intern_string(class.trim_start_matches('\\'));
    let expected = ctx.emit_value(Op::ConstStr, vec![], Some(Immediate::Data(data)), PhpType::Str,
        Op::ConstStr.default_effects(), Some(span));
    let condition = ctx.emit_value(Op::StrictEq, vec![class_name.value, expected.value], None,
        PhpType::Bool, Op::StrictEq.default_effects(), Some(span));
    let yes = ctx.builder.create_named_block("property.setter", vec![]);
    let no = ctx.builder.create_named_block("property.setter.next", vec![]);
    let done = ctx.builder.create_named_block("property.setter.done", vec![]);
    let initial = ctx.initialized_slots_snapshot();
    ctx.builder.terminate(Terminator::CondBr { cond: condition.value,
        then_target: yes, then_args: vec![], else_target: no, else_args: vec![] });
    ctx.builder.position_at_end(yes);
    if let Some(message) = asymmetric_error(ctx, class, property) {
        let _pins = crate::ir_lower::expr::pin_in_flight_owners(ctx, &[value.value], span);
        crate::ir_lower::stmt::lower_throw_access_error(ctx, &message, span);
    } else if accessible(ctx, class, property) {
        emit_setter(ctx, object, value, class, method, span);
    } else {
        let retained = receiver_retains_abstract_setter(ctx, object.value, property);
        fallback_slot_assign(ctx, object, property, value, true, retained, span);
    }
    let yes_live = !ctx.builder.insertion_block_is_terminated();
    if yes_live { branch_to(ctx, done); }
    let yes_slots = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(no);
    ctx.restore_initialized_slots(initial.clone());
    dispatch(ctx, object, property, value, class_name, method, rest, span);
    branch_to(ctx, done);
    let no_slots = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(done);
    ctx.restore_initialized_slots(merge_initialized_slots(&initial, yes_slots, yes_live, no_slots, true));
}

/// Extracts a typed receiver and owns parameter conversions explicitly through the setter call.
fn emit_setter(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, value: LoweredValue,
    class: &str, method: &str, span: Span,
) {
    let ty = PhpType::Object(class.to_string());
    let receiver = if matches!(ctx.builder.value_php_type(object.value).codegen_repr(), PhpType::Object(_)) {
        let receiver = ctx.emit_value(Op::Borrow, vec![object.value], None, ty,
            Op::Borrow.default_effects(), Some(span));
        ctx.builder.set_value_ownership(receiver.value, Ownership::Borrowed);
        receiver
    } else {
        ctx.emit_owned_value(Op::MixedUnbox, vec![object.value], None, ty.clone(),
            Op::mixed_unbox_effects(&ty), Some(span))
    };
    let (receiver, owner) = crate::ir_lower::expr::root_owned_call_operand(ctx, receiver, span);
    let signature = ctx.classes[class].methods.get(&php_symbol_key(method)).cloned();
    let expression = Expr::new(ExprKind::Null, span);
    let pins = crate::ir_lower::expr::pin_in_flight_owners(ctx, &[value.value], span);
    let value_type = ctx.builder.value_php_type(value.value);
    let borrowed = ctx.emit_value(Op::Borrow, vec![value.value], None, value_type,
        Op::Borrow.default_effects(), Some(span));
    ctx.builder.set_value_ownership(borrowed.value, Ownership::Borrowed);
    let argument = if let Some(signature) = signature.as_ref() {
        crate::ir_lower::expr::coerce_scalar_arg_to_param_storage(ctx, signature, 0, borrowed, &expression)
    } else { borrowed };
    let (argument, argument_owner) = crate::ir_lower::expr::root_owned_call_operand(ctx, argument, span);
    let mut operands = vec![receiver.value, argument.value];
    let extra = crate::ir_lower::expr::object_operators::append_hidden_arguments(
        ctx, signature.as_ref(), &mut operands, 1, &expression,
    );
    let data = ctx.intern_string(method);
    ctx.emit_void(Op::MethodCall, operands, Some(Immediate::Data(data)),
        Op::MethodCall.default_effects(), Some(span));
    for argument in extra { crate::ir_lower::ownership::release_if_owned(ctx, argument, Some(span)); }
    if let Some(owner) = argument_owner { crate::ir_lower::expr::retire_owned_call_operand(ctx, owner, span); }
    crate::ir_lower::expr::unpin_in_flight_owners(ctx, pins, span);
    release_magic_set_value_after_call(ctx, value, span);
    if let Some(owner) = owner { crate::ir_lower::expr::retire_owned_call_operand(ctx, owner, span); }
}

/// Keeps raw base-class fallbacks representable when only descendants declare the property.
fn fallback_slot_assign(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, property: &str,
    value: LoweredValue, preserve: bool, retained: bool, span: Span,
) {
    let ty = ctx.builder.value_php_type(object.value);
    let (object, owner) = if matches!(ty.codegen_repr(), PhpType::Object(_)) {
        let borrowed = ctx.emit_value(Op::Borrow, vec![object.value], None, ty,
            Op::Borrow.default_effects(), Some(span));
        ctx.builder.set_value_ownership(borrowed.value, Ownership::Borrowed);
        let boxed = ctx.box_value_as_mixed(borrowed, PhpType::Mixed, Some(span));
        crate::ir_lower::expr::root_owned_call_operand(ctx, boxed, span)
    } else { (object, None) };
    let value = box_value_for_runtime_shaped_receiver(ctx, object, value, span);
    lower_property_slot_assign(ctx, object, property, value, preserve || owner.is_some(), retained, span);
    if let Some(owner) = owner { crate::ir_lower::expr::retire_owned_call_operand(ctx, owner, span); }
}
