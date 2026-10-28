//! Purpose:
//! Applies indexed property incdec after selecting its writable receiver.
//!
//! Called from:
//! - Property array assignment lowering when the XML capability is present.
//!
//! Key details:
//! - Native XML selects a node without writing the stepped scalar back.
//! - Ordinary property arrays retain the shared PHP incdec kernel and store path.

use super::*;

/// Selects the native node in write context and discards the PHP scalar update.
pub(super) fn native(
    ctx: &mut LoweringContext<'_, '_>, child: LoweredValue, index: LoweredValue,
    increment: bool, span: Span,
) {
    let expression = Expr::new(ExprKind::Null, span);
    let selected = crate::ir_lower::expr::lower_native_dimension_write_select_from_values(
        ctx, child, index, &expression,
    );
    step(ctx, selected, increment, span, |_, _| {});
}

/// Reads and writes one ordinary property element without replaying its receiver or key.
pub(super) fn ordinary(
    ctx: &mut LoweringContext<'_, '_>, receiver: LoweredValue, property: &str,
    index: LoweredValue, key: &Expr, increment: bool, span: Span,
) {
    let ty = ctx.builder.value_php_type(receiver.value);
    let borrowed = ctx.emit_value(Op::Borrow, vec![receiver.value], None, ty.clone(),
        Op::Borrow.default_effects(), Some(span));
    ctx.builder.set_value_ownership(borrowed.value, Ownership::Borrowed);
    let boxed = matches!(ty.codegen_repr(), PhpType::Object(class)
        if class.is_empty() || ctx.interfaces.contains_key(&class));
    let object = if boxed { ctx.box_value_as_mixed(borrowed, PhpType::Mixed, Some(span)) } else { borrowed };
    let mut roots = Vec::new();
    let object = root_property_write_value(ctx, object, span, &mut roots);
    let expression = Expr::new(ExprKind::StringLiteral(property.into()), span);
    let array = crate::ir_lower::expr::lower_property_get_from_value(ctx, object, property, Op::PropGet, &expression);
    let array = root_property_write_value(ctx, array, span, &mut roots);
    let expression = Expr::new(ExprKind::ArrayAccess {
        array: Box::new(Expr::new(ExprKind::Null, span)), index: Box::new(key.clone()),
    }, span);
    let selected = crate::ir_lower::expr::lower_array_access_from_values(
        ctx, array, index, key, &expression, true, false,
    );
    step(ctx, selected, increment, span, |ctx, stored| {
        lower_property_array_assign_value(ctx, object, property, index, stored, span, true);
    });
    retire_property_write_roots(ctx, roots, span);
}

/// Preserves float arithmetic and keeps heap-bearing updates in an unwind-visible local slot.
fn step(
    ctx: &mut LoweringContext<'_, '_>, selected: LoweredValue, increment: bool, span: Span,
    finish: impl FnOnce(&mut LoweringContext<'_, '_>, LoweredValue),
) {
    let selected_type = ctx.builder.value_php_type(selected.value);
    let temp_type = if selected_type.codegen_repr() == PhpType::Float { PhpType::Float } else { PhpType::Mixed };
    let rooted = Ownership::php_type_needs_lifetime_tracking(&temp_type);
    let name = ctx.declare_owned_hidden_temp(temp_type.clone());
    let slot = ctx.local_slots[&name];
    if rooted { crate::ir_lower::expr::register_owned_call_operand(ctx, slot, span); }
    crate::ir_lower::expr::store_value_into_temp(ctx, &name, temp_type, selected, span);
    let expression = Expr::new(if increment { ExprKind::PreIncrement(name) } else { ExprKind::PreDecrement(name) }, span);
    let result = lower_expr(ctx, &expression);
    let ty = ctx.builder.value_php_type(result.value);
    let borrowed = ctx.emit_value(Op::Borrow, vec![result.value], None, ty,
        Op::Borrow.default_effects(), Some(span));
    ctx.builder.set_value_ownership(borrowed.value, Ownership::Borrowed);
    finish(ctx, borrowed);
    if rooted { crate::ir_lower::expr::unregister_owned_call_operand(ctx, slot, span); }
    ctx.emit_void(Op::ReleaseLocalSlot, vec![], Some(Immediate::LocalSlot(slot)),
        Op::ReleaseLocalSlot.default_effects(), Some(span));
}
