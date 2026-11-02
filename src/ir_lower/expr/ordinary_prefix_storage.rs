//! Purpose:
//! Preserves typed scalar storage coercion in erased ordinary-property prefix results.
//!
//! Called from:
//! - The ordinary fallback of native XML property incdec lowering after the store succeeds.
//!
//! Key details:
//! - Exact runtime class and property names select only declared scalar backing slots.
//! - Accessor and magic classes keep the captured result without a second observable getter.
//! - The caller holds receiver, name and computed value throughout this non-callback selection.

use super::*;

/// Reads the successfully stored scalar only when its declaration can coerce the prefix value.
pub(super) fn lower(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, name: LoweredValue,
    known_name: Option<&str>, computed: LoweredValue, span: Span,
) -> LoweredValue {
    let mut candidates = Vec::new();
    for (class, info) in ctx.classes {
        if info.methods.contains_key("__get") || info.methods.contains_key("__set")
            || crate::types::builtin_classes::has_native_dimensions(class, ctx.classes) { continue; }
        for (property, ty) in &info.properties {
            if known_name.is_some_and(|name| name != property)
                || !info.declared_properties.contains(property)
                || !scalar(ty)
                || info.property_hooks.get(property).is_some_and(|hooks| hooks.any())
                || !property_is_accessible_for_ir(ctx, class, info, property) { continue; }
            candidates.push((class.clone(), property.clone(), ty.clone()));
        }
    }
    if candidates.is_empty() { return computed; }
    candidates.sort_by(|left, right| (&left.0, &left.1).cmp(&(&right.0, &right.1)));
    let target = crate::ir::RuntimeFnId::GetClass;
    let class_name = ctx.emit_value(Op::RuntimeCall, vec![object.value],
        Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(target))), PhpType::Str,
        target.effects(), Some(span));
    let (class_name, owner) = root_owned_call_operand(ctx, class_name, span);
    let result = ctx.declare_owned_hidden_temp(PhpType::Mixed);
    let done = ctx.builder.create_named_block("property.prefix.stored.merge", vec![]);
    let mut merged: Option<std::collections::HashSet<LocalSlotId>> = None;
    for (class, property, ty) in candidates {
        let data = ctx.intern_string(class.trim_start_matches('\\'));
        let expected = ctx.emit_value(Op::ConstStr, vec![], Some(Immediate::Data(data)), PhpType::Str,
            Op::ConstStr.default_effects(), Some(span));
        let condition = ctx.emit_value(Op::StrictEq, vec![class_name.value, expected.value], None,
            PhpType::Bool, Op::StrictEq.default_effects(), Some(span));
        let stored = ctx.builder.create_named_block("property.prefix.stored", vec![]);
        let next = ctx.builder.create_named_block("property.prefix.next", vec![]);
        let before = ctx.initialized_slots_snapshot();
        let selected = if known_name.is_none() {
            ctx.builder.create_named_block("property.prefix.name", vec![])
        } else { stored };
        ctx.builder.terminate(Terminator::CondBr { cond: condition.value,
            then_target: selected, then_args: vec![], else_target: next, else_args: vec![] });
        if known_name.is_none() {
            ctx.builder.position_at_end(selected);
            let data = ctx.intern_string(&property);
            let expected = ctx.emit_value(Op::ConstStr, vec![], Some(Immediate::Data(data)), PhpType::Str,
                Op::ConstStr.default_effects(), Some(span));
            let condition = ctx.emit_value(Op::StrictEq, vec![name.value, expected.value], None,
                PhpType::Bool, Op::StrictEq.default_effects(), Some(span));
            ctx.builder.terminate(Terminator::CondBr { cond: condition.value,
                then_target: stored, then_args: vec![], else_target: next, else_args: vec![] });
        }
        ctx.builder.position_at_end(stored);
        let concrete = PhpType::Object(class);
        let raw = matches!(ctx.builder.value_php_type(object.value).codegen_repr(), PhpType::Object(_));
        let op = if raw { Op::Borrow } else { Op::MixedUnbox };
        let receiver = ctx.emit_value(op, vec![object.value], None, concrete.clone(),
            if raw { op.default_effects() } else { Op::mixed_unbox_effects(&concrete) }, Some(span));
        ctx.builder.set_value_ownership(receiver.value, Ownership::Borrowed);
        let data = ctx.intern_string(&property);
        let value = ctx.emit_value(Op::PropGet, vec![receiver.value], Some(Immediate::Data(data)), ty,
            Op::PropGet.default_effects(), Some(span));
        store_value_into_temp(ctx, &result, PhpType::Mixed, value, span);
        let state = ctx.initialized_slots_snapshot();
        merged = Some(match merged.take() {
            Some(previous) => previous.intersection(&state).copied().collect(),
            None => state,
        });
        branch_to(ctx, done);
        ctx.builder.position_at_end(next);
        ctx.restore_initialized_slots(before);
    }
    store_value_into_temp(ctx, &result, PhpType::Mixed, computed, span);
    let fallback = ctx.initialized_slots_snapshot();
    let merged = merged.expect("a scalar declaration was selected");
    branch_to(ctx, done);
    ctx.builder.position_at_end(done);
    ctx.restore_initialized_slots(merged.intersection(&fallback).copied().collect());
    if let Some(owner) = owner { retire_owned_call_operand(ctx, owner, span); }
    take_owned_temp(ctx, &result, span)
}

/// Recognizes scalar declarations whose numeric prefix result can be coerced by a property store.
fn scalar(ty: &PhpType) -> bool {
    match ty {
        PhpType::Str | PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::False => true,
        PhpType::Union(members) => !members.is_empty() && members.iter().all(|ty|
            matches!(ty, PhpType::Void) || scalar(ty)),
        _ => false,
    }
}
