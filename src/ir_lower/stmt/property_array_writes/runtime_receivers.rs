//! Purpose:
//! Resolves declared array-property writes whose receiver has lost its concrete class.
//!
//! Called from:
//! - The indexed property writer after receiver, key and value evaluation.
//!
//! Key details:
//! - Exact runtime classes prevent an ancestor arm from bypassing a subclass hook.
//! - Ordinary declared slots reuse their typed COW writer without invoking a magic setter.
//! - Other receivers retain the existing stdClass and dynamic-property fallback.

use super::*;

/// Dispatches an erased receiver to an accessible ordinary declared container slot.
pub(super) fn lower(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, property: &str,
    index: LoweredValue, value: LoweredValue, span: Span, diagnosed: bool,
) -> bool {
    let ty = ctx.builder.value_php_type(object.value);
    if !matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
        && !matches!(ty.codegen_repr(), PhpType::Object(class)
            if class.is_empty() || ctx.interfaces.contains_key(&class)) { return false; }
    let mut classes: Vec<_> = ctx.classes.iter().filter_map(|(name, info)| {
        if info.methods.contains_key("__get") || info.methods.contains_key("__set")
            || info.readonly_properties.contains(property)
            || info.property_hooks.get(property).is_some_and(|hooks| hooks.any())
            || !crate::ir_lower::expr::property_is_accessible_for_ir(ctx, name, info, property)
        { return None; }
        if let Some(visibility) = info.property_set_visibilities.get(property) {
            let declaring = info.property_declaring_classes.get(property).map(String::as_str)
                .unwrap_or(name);
            if !crate::ir_lower::expr::ir_can_access_member(ctx, declaring, visibility) { return None; }
        }
        let (_, (_, property_ty)) = info.visible_property(property)?;
        (property_ty.is_php_array() || matches!(property_ty.codegen_repr(),
            PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Mixed))
            .then(|| name.clone())
    }).collect();
    classes.sort();
    if classes.is_empty() { return false; }

    let name = ctx.declare_owned_hidden_temp(PhpType::Str);
    let name_slot = ctx.local_slots[&name];
    crate::ir_lower::expr::register_owned_call_operand(ctx, name_slot, span);
    let ordinary = ctx.builder.create_named_block("property.array.runtime_class", vec![]);
    let missing = ctx.builder.create_named_block("property.array.runtime_fallback", vec![]);
    let done = ctx.builder.create_named_block("property.array.runtime_done", vec![]);
    let initial = ctx.initialized_slots_snapshot();
    let is_object = ctx.emit_value(Op::TypePredicate, vec![object.value],
        Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::Object)), PhpType::Bool,
        Op::TypePredicate.default_effects(), Some(span));
    ctx.builder.terminate(Terminator::CondBr { cond: is_object.value,
        then_target: ordinary, then_args: vec![], else_target: missing, else_args: vec![] });
    ctx.builder.position_at_end(ordinary);
    let target = crate::ir::RuntimeFnId::GetClass;
    let class = ctx.emit_value(Op::RuntimeCall, vec![object.value],
        Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(target))),
        PhpType::Str, target.effects(), Some(span));
    crate::ir_lower::expr::store_value_into_temp(ctx, &name, PhpType::Str, class, span);
    let mut merged = initial.clone();
    for class in classes {
        let matched = ctx.builder.create_named_block("property.array.declared", vec![]);
        let next = ctx.builder.create_named_block("property.array.next_class", vec![]);
        let current = ctx.load_local(&name, Some(span));
        ctx.builder.set_value_ownership(current.value, Ownership::Borrowed);
        let data = ctx.intern_string(class.trim_start_matches('\\'));
        let expected = ctx.emit_value(Op::ConstStr, vec![], Some(Immediate::Data(data)), PhpType::Str,
            Op::ConstStr.default_effects(), Some(span));
        let condition = ctx.emit_value(Op::StrictEq, vec![current.value, expected.value], None,
            PhpType::Bool, Op::StrictEq.default_effects(), Some(span));
        let before = ctx.initialized_slots_snapshot();
        ctx.builder.terminate(Terminator::CondBr { cond: condition.value,
            then_target: matched, then_args: vec![], else_target: next, else_args: vec![] });
        ctx.builder.position_at_end(matched);
        let concrete = PhpType::Object(class);
        let receiver = if matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
            ctx.emit_value(Op::MixedUnbox, vec![object.value], None, concrete.clone(),
                Op::mixed_unbox_effects(&concrete), Some(span))
        } else {
            ctx.emit_value(Op::Borrow, vec![object.value], None, concrete,
                Op::Borrow.default_effects(), Some(span))
        };
        ctx.builder.set_value_ownership(receiver.value, Ownership::Borrowed);
        lower_property_array_assign_value(ctx, receiver, property, index, value, span, diagnosed);
        if !ctx.builder.insertion_block_is_terminated() {
            merged = merged.intersection(&ctx.initialized_slots_snapshot()).copied().collect();
            ctx.builder.terminate(Terminator::Br { target: done, args: vec![] });
        }
        ctx.builder.position_at_end(next);
        ctx.restore_initialized_slots(before);
    }
    ctx.builder.terminate(Terminator::Br { target: missing, args: vec![] });
    ctx.builder.position_at_end(missing);
    ctx.restore_initialized_slots(initial);
    fallback(ctx, object, property, index, value, span);
    merged = merged.intersection(&ctx.initialized_slots_snapshot()).copied().collect();
    ctx.builder.terminate(Terminator::Br { target: done, args: vec![] });
    ctx.builder.position_at_end(done);
    ctx.restore_initialized_slots(merged);
    crate::ir_lower::expr::retire_owned_call_operand(ctx, name_slot, span);
    true
}

/// Keeps the existing runtime property writer for receivers without an ordinary declared slot.
pub(super) fn fallback(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, property: &str,
    index: LoweredValue, value: LoweredValue, span: Span,
) {
    let data = ctx.intern_string(property);
    ctx.emit_void(Op::RuntimeCall, vec![object.value, index.value, value.value],
        Some(Immediate::Data(data)), effects_lookup::runtime_effects(), Some(span));
}
