//! Purpose:
//! Retains managed property references behind ordinary-object receivers.
//!
//! Called from:
//! - Checked known or runtime-selected reference arguments, including native XML fallbacks.
//!
//! Key details:
//! - Exact runtime classes select their authoritative property layout and owned cell metadata.
//! - The receiver is evaluated once; other property layouts retain their existing fallback.

use super::*;

/// Selects a supported declared owned cell without mistaking a borrowed raw payload for boxed storage.
pub(super) fn named(ctx: &mut LoweringContext<'_, '_>, object: LoweredValue,
    property: &str, alias: &str, expr: &Expr) -> LoweredValue {
    let mut classes = ctx.classes.iter().filter_map(|(name, info)| {
        eligible_property(info, property, ctx.semantic_profile).then(|| name.clone())
    }).collect::<Vec<_>>();
    classes.sort();
    if classes.is_empty() {
        return lower_property_get_without_native_dispatch(ctx, object, property, Op::PropGet, expr);
    }
    let span = expr.span;
    let staging = prepublish_call_result(ctx, &PhpType::Mixed, span);
    let target = crate::ir::RuntimeFnId::GetClass;
    let name = ctx.emit_value(Op::RuntimeCall, vec![object.value],
        Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::Function(target))),
        PhpType::Str, target.effects(), Some(span));
    let (name, owner) = root_owned_call_operand(ctx, name, span);
    let result = ctx.declare_owned_hidden_temp(PhpType::Mixed);
    let initial = ctx.initialized_slots_snapshot();
    let done = ctx.builder.create_named_block("reference.property.done", Vec::new());
    let mut merged: Option<HashSet<LocalSlotId>> = None;
    for class in classes {
        let data = ctx.intern_string(class.trim_start_matches('\\'));
        let expected = ctx.emit_value(Op::ConstStr, Vec::new(), Some(Immediate::Data(data)),
            PhpType::Str, Op::ConstStr.default_effects(), Some(span));
        let matched = ctx.emit_value(Op::StrictEq, vec![name.value, expected.value], None,
            PhpType::Bool, Op::StrictEq.default_effects(), Some(span));
        let selected = ctx.builder.create_named_block("reference.property.selected", Vec::new());
        let next = ctx.builder.create_named_block("reference.property.next", Vec::new());
        ctx.builder.terminate(Terminator::CondBr { cond: matched.value, then_target: selected,
            then_args: Vec::new(), else_target: next, else_args: Vec::new() });
        ctx.builder.position_at_end(selected);
        let ty = PhpType::Object(class);
        let receiver = if matches!(ctx.builder.value_php_type(object.value).codegen_repr(), PhpType::Object(_)) {
            ctx.emit_value(Op::Borrow, vec![object.value], None, ty,
                Op::Borrow.default_effects(), Some(span))
        } else {
            ctx.emit_value(Op::MixedUnbox, vec![object.value], None, ty.clone(),
                Op::mixed_unbox_effects(&ty), Some(span))
        };
        // The enclosing dispatch owns the object until its property cell has been leased.
        ctx.builder.set_value_ownership(receiver.value, Ownership::Borrowed);
        bind_property_reference_cell(ctx, alias, receiver, property, expr, span);
        let value = lower_invoker_ref_arg_marker(ctx, alias, span);
        store_value_into_temp(ctx, &result, PhpType::Mixed, value, span);
        merge_initialized_branch(ctx, &mut merged);
        branch_to(ctx, done);
        ctx.builder.position_at_end(next); ctx.restore_initialized_slots(initial.clone());
    }
    let value = lower_property_get_without_native_dispatch(ctx, object, property, Op::PropGet, expr);
    store_value_into_temp(ctx, &result, PhpType::Mixed, value, span);
    merge_initialized_branch(ctx, &mut merged);
    branch_to(ctx, done); ctx.builder.position_at_end(done);
    ctx.restore_initialized_slots(merged.expect("property dispatch includes its fallback"));
    let value = take_owned_temp(ctx, &result, span);
    stage_call_result(ctx, staging.as_ref(), value, span);
    if let Some(owner) = owner { retire_owned_call_operand(ctx, owner, span); }
    take_prepublished_call_result(ctx, staging, value, span)
}

/// Matches the evaluated property bytes before reusing the exact-class reference binder.
pub(super) fn dynamic(ctx: &mut LoweringContext<'_, '_>, object: LoweredValue,
    property: LoweredValue, alias: &str, expr: &Expr) -> LoweredValue {
    let mut properties = ctx.classes.values().flat_map(|info| info.properties.iter()
        .filter(|(name, _)| eligible_property(info, name, ctx.semantic_profile)).map(|(name, _)| name.clone()))
        .collect::<Vec<_>>();
    properties.sort(); properties.dedup();
    if properties.is_empty() {
        return lower_dynamic_property_fetch_without_native(ctx, object, property,
            PhpType::Mixed, PropertyFetchMode::Read, expr);
    }
    let span = expr.span; let result = ctx.declare_owned_hidden_temp(PhpType::Mixed);
    let initial = ctx.initialized_slots_snapshot();
    let done = ctx.builder.create_named_block("reference.name.done", Vec::new());
    let mut merged = None;
    for name in properties {
        let data = ctx.intern_string(&name);
        let expected = ctx.emit_value(Op::ConstStr, Vec::new(), Some(Immediate::Data(data)),
            PhpType::Str, Op::ConstStr.default_effects(), Some(span));
        let matched = ctx.emit_value(Op::StrictEq, vec![property.value, expected.value], None,
            PhpType::Bool, Op::StrictEq.default_effects(), Some(span));
        let selected = ctx.builder.create_named_block("reference.name.selected", Vec::new());
        let next = ctx.builder.create_named_block("reference.name.next", Vec::new());
        ctx.builder.terminate(Terminator::CondBr { cond: matched.value, then_target: selected,
            then_args: Vec::new(), else_target: next, else_args: Vec::new() });
        ctx.builder.position_at_end(selected);
        let value = named(ctx, object, &name, alias, expr);
        store_value_into_temp(ctx, &result, PhpType::Mixed, value, span);
        merge_initialized_branch(ctx, &mut merged); branch_to(ctx, done);
        ctx.builder.position_at_end(next); ctx.restore_initialized_slots(initial.clone());
    }
    let value = lower_dynamic_property_fetch_without_native(ctx, object, property,
        PhpType::Mixed, PropertyFetchMode::Read, expr);
    store_value_into_temp(ctx, &result, PhpType::Mixed, value, span);
    merge_initialized_branch(ctx, &mut merged); branch_to(ctx, done);
    ctx.builder.position_at_end(done);
    ctx.restore_initialized_slots(merged.expect("property-name dispatch includes its fallback"));
    take_owned_temp(ctx, &result, span)
}

/// Shares managed slot eligibility between literal and runtime property names.
fn eligible_property(info: &crate::types::ClassInfo, property: &str, profile: elephc_semantics::SemanticProfile) -> bool {
    let Some(index) = info.visible_property_index(property) else { return false; };
    !info.is_internal && (info.properties[index].1 == PhpType::Mixed
        || (profile.contains(elephc_semantics::StrictFeature::Casts)
            && crate::types::canonical_property_reference_payload(&info.properties[index].1).is_some()))
        && info.property_slot_owns_reference(index)
        && !info.readonly_properties.contains(property)
        && info.property_hooks.get(property).is_none_or(|hooks| !hooks.any())
}

/// Keeps only initialization facts established by every completed dispatch arm.
fn merge_initialized_branch(ctx: &LoweringContext<'_, '_>, merged: &mut Option<HashSet<LocalSlotId>>) {
    let slots = ctx.initialized_slots_snapshot();
    *merged = Some(match merged.take() { Some(previous) => previous.intersection(&slots).copied().collect(),
        None => slots });
}

/// Retires a temporary ordinary receiver after leasing its cell, before the selected call begins.
pub(super) fn dynamic_argument(ctx: &mut LoweringContext<'_, '_>, object: LoweredValue,
    property: LoweredValue, alias: &str, expr: &Expr) -> LoweredValue {
    let staging = prepublish_call_result(ctx, &PhpType::Mixed, expr.span);
    let (object, object_owner) = root_owned_call_operand(ctx, object, expr.span);
    let (property, property_owner) = root_owned_call_operand(ctx, property, expr.span);
    let value = dynamic(ctx, object, property, alias, expr);
    stage_call_result(ctx, staging.as_ref(), value, expr.span);
    if let Some(owner) = property_owner { retire_owned_call_operand(ctx, owner, expr.span); }
    if let Some(owner) = object_owner { retire_owned_call_operand(ctx, owner, expr.span); }
    take_prepublished_call_result(ctx, staging, value, expr.span)
}
