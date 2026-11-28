//! Purpose:
//! Routes runtime names through initialized named-property and accessor selection.
//!
//! Called from:
//! - Dynamic property lowering after native protocols and ordinary hooks are selected.
//!
//! Key details:
//! - Receivers and names are already evaluated and stay rooted across every branch.
//! - Unknown names reuse the existing boxed runtime magic/hash dispatcher.

use super::*;

/// Selects named initialized slots while preserving runtime-name getter and probe protocols.
pub(super) fn lower(ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, name: LoweredValue,
    mode: PropertyFetchMode, expr: &Expr,
) -> Option<LoweredValue> {
    if !crate::ir_lower::initialization::enabled(ctx) { return None; }
    let ty = ctx.builder.value_php_type(object.value);
    let known = singular_object_class(&ty).filter(|(_, nullable)| !nullable).map(|(class, _)| class.to_string());
    let known_magic = known.as_ref().is_some_and(|class| ctx.classes.get(class)
        .is_some_and(|info| info.methods.contains_key("__get")));
    if known.is_some() && !known_magic { return None; }
    let mut properties = std::collections::BTreeSet::new();
    for (class, info) in ctx.classes {
        if known.as_ref().is_some_and(|known| known != class) || !info.methods.contains_key("__get") { continue; }
        for (property, _) in &info.properties {
            if super::initialization_properties::selectable_slot(ctx, class, property) {
                properties.insert(property.clone());
            }
        }
    }
    if properties.is_empty() && !known_magic { return None; }
    let staging = prepublish_call_result(ctx, &PhpType::Mixed, expr.span);
    let (object, object_owner) = root_value_for_later_use(ctx, object, expr.span);
    let (name, name_owner) = root_value_for_later_use(ctx, name, expr.span);
    let temp = ctx.declare_owned_hidden_temp(PhpType::Mixed);
    let done = ctx.builder.create_named_block("property.initialized.dynamic.done", Vec::new());
    let mut merged: Option<HashSet<LocalSlotId>> = None;
    for property in properties {
        let data = ctx.intern_string(&property);
        let expected = ctx.emit_value(Op::ConstStr, Vec::new(), Some(Immediate::Data(data)),
            PhpType::Str, Op::ConstStr.default_effects(), Some(expr.span));
        let condition = ctx.emit_value(Op::StrictEq, vec![name.value, expected.value], None,
            PhpType::Bool, Op::StrictEq.default_effects(), Some(expr.span));
        let yes = ctx.builder.create_named_block("property.initialized.dynamic.name", Vec::new());
        let next = ctx.builder.create_named_block("property.initialized.dynamic.next", Vec::new());
        let initial = ctx.initialized_slots_snapshot();
        ctx.builder.terminate(Terminator::CondBr { cond: condition.value,
            then_target: yes, then_args: Vec::new(), else_target: next, else_args: Vec::new() });
        ctx.builder.position_at_end(yes);
        let receiver = borrow(ctx, object, expr.span);
        let value = if mode.uses_magic_get() {
            super::property_access::lower_property_get_from_value(ctx, receiver, &property, Op::PropGet, expr)
        } else {
            super::initialization_properties::magic_probe(ctx, receiver, &property, mode, expr)
                .expect("selected accessible untyped magic slot")
        };
        store_value_into_temp(ctx, &temp, PhpType::Mixed, value, expr.span);
        let state = ctx.initialized_slots_snapshot();
        merged = Some(match merged.take() { Some(previous) => previous.intersection(&state).copied().collect(), None => state });
        branch_to(ctx, done);
        ctx.builder.position_at_end(next);
        ctx.restore_initialized_slots(initial);
    }
    let receiver = borrow(ctx, object, expr.span);
    let receiver = if matches!(ctx.builder.value_php_type(receiver.value).codegen_repr(), PhpType::Object(_)) {
        ctx.box_value_as_mixed(receiver, PhpType::Mixed, Some(expr.span))
    } else { receiver };
    let name_view = borrow(ctx, name, expr.span);
    let value = super::property_access::lower_dynamic_property_fetch_physical(ctx, receiver,
        name_view, PhpType::Mixed, mode, expr);
    store_value_into_temp(ctx, &temp, PhpType::Mixed, value, expr.span);
    let fallback = ctx.initialized_slots_snapshot();
    branch_to(ctx, done);
    ctx.builder.position_at_end(done);
    let state = merged.map_or(fallback.clone(), |state| state.intersection(&fallback).copied().collect());
    ctx.restore_initialized_slots(state);
    let result = take_owned_temp(ctx, &temp, expr.span);
    stage_call_result(ctx, staging.as_ref(), result, expr.span);
    if let Some(owner) = name_owner { retire_owned_call_operand(ctx, owner, expr.span); }
    if let Some(owner) = object_owner { retire_owned_call_operand(ctx, owner, expr.span); }
    Some(take_prepublished_call_result(ctx, staging, result, expr.span))
}

/// Lends an operand's existing root to a mutually exclusive read arm.
fn borrow(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, span: Span) -> LoweredValue {
    let ty = ctx.builder.value_php_type(value.value);
    let value = ctx.emit_value(Op::Borrow, vec![value.value], None, ty, Op::Borrow.default_effects(), Some(span));
    ctx.builder.set_value_ownership(value.value, Ownership::Borrowed);
    value
}
