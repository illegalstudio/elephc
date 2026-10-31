//! Purpose:
//! Preserves PHP property getters when native XML dispatch leaves a boxed ordinary object.
//!
//! Called from:
//! - Named property reads before the physical-slot fallback.
//!
//! Key details:
//! - Runtime class guards select ordinary EIR method calls with their normal hidden metadata.
//! - The shared receiver root survives every getter and is retired after the result is secured.

use super::*;

/// Returns a getter dispatch only when boxed storage has lost its concrete PHP class.
pub(super) fn lower(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, property: &str, op: Op, expr: &Expr,
) -> Option<LoweredValue> {
    if !matches!(ctx.builder.value_php_type(object.value).codegen_repr(), PhpType::Mixed | PhpType::Union(_))
        || singular_object_class(&ctx.builder.value_php_type(object.value)).is_some()
        || ctx.in_own_property_accessor(property) { return None; }
    let method = property_hook_get_method(property);
    let key = php_symbol_key(&method);
    let mut classes = ctx.classes.iter().filter_map(|(name, info)| {
        (info.methods.contains_key(&key) && property_is_accessible_for_ir(ctx, name, info, property))
            .then(|| name.clone())
    }).collect::<Vec<_>>();
    classes.sort_by_key(|name| std::cmp::Reverse(ctx.classes[name].class_id));
    if classes.is_empty() { return None; }
    let staging = prepublish_call_result(ctx, &PhpType::Mixed, expr.span);
    let (receiver, owner) = root_owned_call_operand(ctx, object, expr.span);
    let result = dispatch(ctx, receiver, property, &method, &classes, op, expr);
    stage_call_result(ctx, staging.as_ref(), result, expr.span);
    if let Some(owner) = owner { retire_owned_call_operand(ctx, owner, expr.span); }
    Some(take_prepublished_call_result(ctx, staging, result, expr.span))
}

/// Calls the selected getter or reaches the physical-slot path after all class guards fail.
fn dispatch(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, property: &str, method: &str,
    classes: &[String], op: Op, expr: &Expr,
) -> LoweredValue {
    let Some((class, rest)) = classes.split_first() else {
        return lower_property_get_without_runtime_hooks(ctx, object, property, op, expr);
    };
    let data = ctx.intern_class_name(class);
    let condition = ctx.emit_value(Op::InstanceOf, vec![object.value], Some(Immediate::Data(data)),
        PhpType::Bool, Op::InstanceOf.default_effects(), Some(expr.span));
    let yes = ctx.builder.create_named_block("property.getter", vec![]);
    let no = ctx.builder.create_named_block("property.getter.next", vec![]);
    let done = ctx.builder.create_named_block("property.getter.done", vec![]);
    let temp = ctx.declare_owned_hidden_temp(PhpType::Mixed);
    let initial = ctx.initialized_slots_snapshot();
    ctx.builder.terminate(Terminator::CondBr { cond: condition.value,
        then_target: yes, then_args: vec![], else_target: no, else_args: vec![] });
    ctx.builder.position_at_end(yes);
    let receiver = borrow_receiver(ctx, object, expr.span);
    let value = lower_method_call_with_receiver(ctx, receiver, method, &[], Op::MethodCall, expr);
    store_value_into_temp(ctx, &temp, PhpType::Mixed, value, expr.span);
    branch_to(ctx, done);
    let yes_slots = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(no);
    ctx.restore_initialized_slots(initial.clone());
    let receiver = borrow_receiver(ctx, object, expr.span);
    let value = dispatch(ctx, receiver, property, method, rest, op, expr);
    store_value_into_temp(ctx, &temp, PhpType::Mixed, value, expr.span);
    branch_to(ctx, done);
    let no_slots = ctx.initialized_slots_snapshot();
    ctx.builder.position_at_end(done);
    ctx.restore_initialized_slots(merge_initialized_slots_for_expr(&initial, yes_slots, true, no_slots, true));
    take_owned_temp(ctx, &temp, expr.span)
}

/// Lends the common owner to a mutually exclusive getter or physical read.
fn borrow_receiver(ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, span: Span) -> LoweredValue {
    let ty = ctx.builder.value_php_type(object.value);
    let value = ctx.emit_value(Op::Borrow, vec![object.value], None, ty, Op::Borrow.default_effects(), Some(span));
    ctx.builder.set_value_ownership(value.value, Ownership::Borrowed);
    value
}
