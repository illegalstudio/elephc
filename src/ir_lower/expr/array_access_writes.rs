//! Purpose:
//! Routes erased object offset writes through native or public dimension methods.
//!
//! Called from:
//! - Boxed offset writes after string receiver dispatch.
//!
//! Key details:
//! - Native XML dimensions take priority over a subclass's public ArrayAccess methods.
//! - Object offsets retain their original PHP kinds instead of entering array-key normalization.
//! - Independent argument snapshots and a checked receiver use shared call ownership.

use super::*;

/// Selects native dimensions first, then public ArrayAccess and the non-object write path.
pub(crate) fn lower_array_access_set_or<'m, 'f>(
    ctx: &mut LoweringContext<'m, 'f>, receiver: LoweredValue,
    key: LoweredValue, value: LoweredValue, span: Span,
    fallback: impl FnOnce(&mut LoweringContext<'m, 'f>),
) {
    let public = |ctx: &mut LoweringContext<'m, 'f>| {
        if ctx.interfaces.contains_key("ArrayAccess") {
            lower_offset_set_or(ctx, receiver, key, value, span, ("ArrayAccess", "offsetSet"), fallback);
        } else {
            fallback(ctx);
        }
    };
    let class = "SimpleXMLElement";
    if let Some(method) = crate::types::builtin_classes::native_dimension_method(
        class, elephc_builtin_contract::ObjectDimension::Set, ctx.classes,
    ) {
        lower_offset_set_or(ctx, receiver, key, value, span, (class, method), public);
    } else {
        public(ctx);
    }
}

/// Invokes a dimension method only after its receiver type is established.
fn lower_offset_set_or<'m, 'f>(
    ctx: &mut LoweringContext<'m, 'f>, receiver: LoweredValue,
    key: LoweredValue, value: LoweredValue, span: Span,
    target: (&str, &str), fallback: impl FnOnce(&mut LoweringContext<'m, 'f>),
) {
    let (class, _) = target;
    let class = ctx.intern_class_name(class);
    let accepted = ctx.emit_value(Op::InstanceOf, vec![receiver.value],
        Some(Immediate::Data(class)), PhpType::Bool, Op::InstanceOf.default_effects(), Some(span));
    let object = ctx.builder.create_named_block("offset.object.store", Vec::new());
    let other = ctx.builder.create_named_block("offset.other.store", Vec::new());
    let merge = ctx.builder.create_named_block("offset.object.done", Vec::new());
    let initialized = ctx.initialized_slots_snapshot();
    ctx.builder.terminate(Terminator::CondBr { cond: accepted.value,
        then_target: object, then_args: Vec::new(), else_target: other, else_args: Vec::new() });
    ctx.builder.position_at_end(object);
    ctx.with_independent_write_receiver(|ctx| {
        lower_selected_offset_set(ctx, receiver, key, value, span, target);
    });
    let selected_initialized = ctx.initialized_slots_snapshot();
    branch_to(ctx, merge);
    ctx.builder.position_at_end(other);
    ctx.restore_initialized_slots(initialized);
    fallback(ctx);
    let mut joined = ctx.initialized_slots_snapshot();
    joined.retain(|slot| selected_initialized.contains(slot));
    branch_to(ctx, merge);
    ctx.builder.position_at_end(merge);
    ctx.restore_initialized_slots(joined);
}

/// Invokes a proven object receiver with independent key and value snapshots.
fn lower_selected_offset_set(
    ctx: &mut LoweringContext<'_, '_>, receiver: LoweredValue,
    key: LoweredValue, value: LoweredValue, span: Span, target: (&str, &str),
) {
    let (class, method) = target;
    let object = ctx.emit_owned_value(Op::MixedUnbox, vec![receiver.value], None,
        PhpType::Object(class.into()), Op::MixedUnbox.default_effects()
            | crate::ir::Effects::WRITES_HEAP | crate::ir::Effects::REFCOUNT_OP, Some(span));
    let key = crate::ir_lower::ownership::borrow_value(ctx, key, Some(span));
    let key = ctx.box_value_as_mixed(key, PhpType::Mixed, Some(span));
    let value = crate::ir_lower::ownership::borrow_value(ctx, value, Some(span));
    let value = ctx.box_value_as_mixed(value, PhpType::Mixed, Some(span));
    let mut owners = Vec::new();
    let mut arguments = Vec::new();
    for operand in [object, key, value] {
        let ty = ctx.builder.value_php_type(operand.value);
        let name = ctx.declare_hidden_temp(ty.clone());
        store_value_into_temp(ctx, &name, ty.clone(), operand, span);
        if Ownership::php_type_needs_lifetime_tracking(&ty) {
            let slot = ctx.local_slots[&name];
            register_owned_call_operand(ctx, slot, span);
            owners.push(slot);
        }
        arguments.push(Expr::new(ExprKind::Variable(name), span));
    }
    let expression = Expr::new(ExprKind::MethodCall {
        object: Box::new(arguments[0].clone()), method: method.into(),
        args: arguments[1..].to_vec(),
    }, span);
    let object = lower_expr(ctx, &arguments[0]);
    let result = lower_method_call_with_receiver(ctx, object, method, &arguments[1..],
        Op::MethodCall, &expression);
    crate::ir_lower::ownership::release_if_owned(ctx, result, Some(span));
    for owner in owners.into_iter().rev() { retire_owned_call_operand(ctx, owner, span); }
}
