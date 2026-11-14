//! Purpose:
//! Retains live Mixed property references behind an erased ordinary-object receiver.
//!
//! Called from:
//! - Checked runtime-selected reference arguments after native XML dispatch.
//!
//! Key details:
//! - Exact runtime classes select their authoritative property layout and owned cell metadata.
//! - The receiver is evaluated once; other property layouts retain their existing fallback.

use super::*;

/// Selects a declared owned Mixed cell without treating a concrete borrowed payload as boxed storage.
pub(super) fn named(ctx: &mut LoweringContext<'_, '_>, object: LoweredValue,
    property: &str, alias: &str, expr: &Expr) -> LoweredValue {
    let mut classes = ctx.classes.iter().filter_map(|(name, info)| {
        let index = info.visible_property_index(property)?;
        (!info.is_internal && info.properties[index].1 == PhpType::Mixed
            && info.property_slot_owns_reference(index)
            && !info.readonly_properties.contains(property)
            && info.property_hooks.get(property).is_none_or(|hooks| !hooks.any()))
            .then(|| name.clone())
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
        let receiver = ctx.emit_value(Op::MixedUnbox, vec![object.value], None, ty.clone(),
            Op::mixed_unbox_effects(&ty), Some(span));
        // The enclosing dispatch owns the object until its property cell has been leased.
        ctx.builder.set_value_ownership(receiver.value, Ownership::Borrowed);
        bind_property_reference_cell(ctx, alias, receiver, property, expr, span);
        let value = lower_invoker_ref_arg_marker(ctx, alias, span);
        store_value_into_temp(ctx, &result, PhpType::Mixed, value, span);
        let slots = ctx.initialized_slots_snapshot();
        merged = Some(match merged { Some(previous) => previous.intersection(&slots).copied().collect(),
            None => slots });
        branch_to(ctx, done);
        ctx.builder.position_at_end(next); ctx.restore_initialized_slots(initial.clone());
    }
    let value = lower_property_get_without_native_dispatch(ctx, object, property, Op::PropGet, expr);
    store_value_into_temp(ctx, &result, PhpType::Mixed, value, span);
    let slots = ctx.initialized_slots_snapshot();
    let merged = merged.map_or_else(|| slots.clone(), |previous| previous.intersection(&slots).copied().collect());
    branch_to(ctx, done); ctx.builder.position_at_end(done); ctx.restore_initialized_slots(merged);
    let value = take_owned_temp(ctx, &result, span);
    stage_call_result(ctx, staging.as_ref(), value, span);
    if let Some(owner) = owner { retire_owned_call_operand(ctx, owner, span); }
    take_prepublished_call_result(ctx, staging, value, span)
}
