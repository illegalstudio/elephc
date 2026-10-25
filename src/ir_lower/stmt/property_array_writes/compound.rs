//! Purpose:
//! Computes compound property-dimension values after staged operands and receiver selection.
//!
//! Called from:
//! - Property array assignments and guarded native XML dimension writers.
//!
//! Key details:
//! - Reuses the shared getter and binary semantics with unwind-visible owners.
//! - Target reads occur after RHS effects without repeating keys or values.

use super::*;

/// Extracts a parser-desugared compound RHS so its target read can occur after RHS effects.
pub(super) fn compound_source<'a>(value: &'a Expr, object: &Expr, property: &str, index: &Expr, span: Span)
    -> Option<(&'a BinOp, &'a Expr)>
{
    if !matches!(desugared_element_update(value, span, |read|
        reads_property_array_element(read, object, property, index)), Some(ElementUpdate::Compound)) { return None; }
    let value = match &value.kind {
        ExprKind::Assignment { value, .. } => value.as_ref(),
        _ => value,
    };
    if let ExprKind::BinaryOp { op, right, .. } = &value.kind { Some((op, right)) } else { None }
}

/// Reads an ordinary property after staged operands and preserves its established write path.
pub(super) fn write_ordinary_compound(
    ctx: &mut LoweringContext<'_, '_>, object: LoweredValue, property: &str,
    index: LoweredValue, value: LoweredValue, compound: Option<(&BinOp, &Expr)>, span: Span, diagnosed: bool,
) {
    let Some((op, key)) = compound else {
        lower_property_array_assign_value(ctx, object, property, index, value, span, diagnosed);
        return;
    };
    let ty = ctx.builder.value_php_type(object.value);
    let borrowed = ctx.emit_value(Op::Borrow, vec![object.value], None, ty,
        Op::Borrow.default_effects(), Some(span));
    ctx.builder.set_value_ownership(borrowed.value, Ownership::Borrowed);
    let expression = Expr::new(ExprKind::StringLiteral(property.into()), span);
    let array = crate::ir_lower::expr::lower_property_get_from_value(ctx, borrowed, property, Op::PropGet, &expression);
    with_compound_value(ctx, array, index, value, op, key, span, |ctx, stored| {
        lower_property_array_assign_value(ctx, object, property, index, stored, span, diagnosed);
    });
}

/// Holds a selected container, its late element read and the computed value through insertion.
pub(super) fn with_compound_value(
    ctx: &mut LoweringContext<'_, '_>, array: LoweredValue, index: LoweredValue,
    rhs: LoweredValue, op: &BinOp, key: &Expr, span: Span,
    write: impl FnOnce(&mut LoweringContext<'_, '_>, LoweredValue),
) {
    let mut roots = Vec::new();
    let array = root_property_write_value(ctx, array, span, &mut roots);
    let read_expr = Expr::new(ExprKind::ArrayAccess {
        array: Box::new(Expr::new(ExprKind::Null, span)), index: Box::new(key.clone()),
    }, span);
    let read = crate::ir_lower::expr::lower_array_access_from_values(ctx, array, index, key, &read_expr, true, false);
    let read = root_property_write_value(ctx, read, span, &mut roots);
    let stored = crate::ir_lower::expr::lower_prepared_binary(ctx, read, op, rhs, span);
    let stored = root_property_write_value(ctx, stored, span, &mut roots);
    write(ctx, stored);
    retire_property_write_roots(ctx, roots, span);
}

