//! Purpose:
//! Selects declared native XML children of ordinary erased property-write receivers.
//!
//! Called from:
//! - The ordinary branch of native XML property write-fetch lowering.
//!
//! Key details:
//! - Exact runtime classes preserve inherited offsets and declaration access checks.
//! - Write-fetch guards run before the child is acquired into an owned merge temporary.
//! - Other property shapes leave their ordinary writer and writeback semantics unchanged.

use super::*;

/// Fetches an independently owned native child or a null sentinel for ordinary property storage.
pub(super) fn fetch(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, expression: &Expr,
    property: &str, span: Span, mode: PropertyDimensionFetchMode,
) -> Option<LoweredValue> {
    let mut classes = ctx.classes.iter().filter_map(|(class, info)| {
        if info.property_hooks.get(property).is_some_and(|hooks| hooks.any())
            || !crate::ir_lower::expr::property_is_accessible_for_ir(ctx, class, info, property)
        { return None; }
        let _storage = object_type_property_type(ctx, &PhpType::Object(class.clone()), property)?;
        let (_, (_, declared)) = info.visible_property(property)?;
        let (child, _) = crate::ir_lower::expr::singular_object_class(declared)?;
        crate::types::builtin_classes::has_native_dimensions(child, ctx.classes).then(|| class.clone())
    }).collect::<Vec<_>>();
    classes.sort();
    if classes.is_empty() { return None; }
    let result = ctx.declare_owned_hidden_temp(PhpType::Mixed);
    let name = ctx.declare_owned_hidden_temp(PhpType::Str);
    let name_slot = ctx.local_slots[&name];
    crate::ir_lower::expr::register_owned_call_operand(ctx, name_slot, span);
    let objects = ctx.builder.create_named_block("xml.property_child.runtime_class", vec![]);
    let missing = ctx.builder.create_named_block("xml.property_child.ordinary", vec![]);
    let done = ctx.builder.create_named_block("xml.property_child.merge", vec![]);
    let initial = ctx.initialized_slots_snapshot();
    let is_object = ctx.emit_value(Op::TypePredicate, vec![object.value],
        Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::Object)), PhpType::Bool,
        Op::TypePredicate.default_effects(), Some(span));
    ctx.builder.terminate(Terminator::CondBr { cond: is_object.value,
        then_target: objects, then_args: vec![], else_target: missing, else_args: vec![] });
    ctx.builder.position_at_end(objects);
    let target = crate::ir::RuntimeFnId::GetClass;
    let class = ctx.emit_value(Op::RuntimeCall, vec![object.value],
        Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(target))), PhpType::Str,
        target.effects(), Some(span));
    crate::ir_lower::expr::store_value_into_temp(ctx, &name, PhpType::Str, class, span);
    let mut merged = initial.clone();
    for class in classes {
        let matched = ctx.builder.create_named_block("xml.property_child.declared", vec![]);
        let next = ctx.builder.create_named_block("xml.property_child.next_class", vec![]);
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
        let ty = ctx.builder.value_php_type(object.value);
        let receiver = if matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
            ctx.emit_value(Op::MixedUnbox, vec![object.value], None, concrete.clone(),
                Op::mixed_unbox_effects(&concrete), Some(span))
        } else {
            ctx.emit_value(Op::Borrow, vec![object.value], None, concrete,
                Op::Borrow.default_effects(), Some(span))
        };
        ctx.builder.set_value_ownership(receiver.value, Ownership::Borrowed);
        let child = lower_native_dimension_property(ctx, receiver, expression, property, span, mode)
            .expect("the selected declaration has native XML dimensions");
        let child = ctx.with_independent_write_receiver(|ctx| {
            if ctx.value_is_owning_temporary(child) { child } else {
                crate::ir_lower::ownership::acquire_lifetime_pin_if_refcounted(ctx, child, Some(span))
            }
        });
        crate::ir_lower::expr::store_value_into_temp(ctx, &result, PhpType::Mixed, child, span);
        merged = merged.intersection(&ctx.initialized_slots_snapshot()).copied().collect();
        jump(ctx, done);
        ctx.builder.position_at_end(next);
        ctx.restore_initialized_slots(before);
    }
    jump(ctx, missing);
    ctx.builder.position_at_end(missing);
    ctx.restore_initialized_slots(initial);
    let null = lower_expr(ctx, &Expr::new(ExprKind::Null, span));
    crate::ir_lower::expr::store_value_into_temp(ctx, &result, PhpType::Mixed, null, span);
    merged = merged.intersection(&ctx.initialized_slots_snapshot()).copied().collect();
    jump(ctx, done);
    ctx.builder.position_at_end(done);
    ctx.restore_initialized_slots(merged);
    crate::ir_lower::expr::retire_owned_call_operand(ctx, name_slot, span);
    Some(crate::ir_lower::expr::take_owned_temp(ctx, &result, span))
}

/// Ends a selected child arm without adding an edge after a declaration Error.
fn jump(ctx: &mut LoweringContext<'_, '_>, target: BlockId) {
    if !ctx.builder.insertion_block_is_terminated() {
        ctx.builder.terminate(Terminator::Br { target, args: vec![] });
    }
}
