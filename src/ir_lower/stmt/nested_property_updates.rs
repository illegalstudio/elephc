//! Purpose:
//! Shares property-root nested update captures with statement and expression lowering.
//!
//! Called from:
//! - Nested array updates and assignment expressions.
//!
//! Key details:
//! - Compound RHS values precede property RW selection and element reads.
//! - Expression results reuse the same attached update instead of replaying the write.

use super::*;

/// Identifies an ordinary named property whose dimensions share one RW container fetch.
pub(super) fn named_property_root(target: &Expr) -> Option<(&Expr, &str)> {
    let mut root = target;
    while let ExprKind::ArrayAccess { array, .. } = &root.kind { root = array; }
    let ExprKind::PropertyAccess { object, property } = &root.kind else { return None; };
    Some((object, property))
}

/// Checks attached boxed storage before capturing any operands or changing their evaluation order.
pub(super) fn attached_property_root(ctx: &LoweringContext<'_, '_>, target: &Expr) -> bool {
    named_property_root(target).is_some_and(|(object, property)|
        crate::ir_lower::expr::nested_property_has_boxed_root(ctx, object, property))
}

/// Settles a compound RHS before the first element read, including expression-result captures.
pub(super) fn capture_compound_rhs(
    ctx: &mut LoweringContext<'_, '_>, value: &mut Expr,
    sources: &mut Vec<super::nested_write_sources::WriteSource>,
) {
    match &mut value.kind {
        ExprKind::Assignment { value, prelude, .. } if prelude.is_empty() =>
            capture_compound_rhs(ctx, value, sources),
        ExprKind::BinaryOp { right, .. } => {
            *right = Box::new(super::nested_write_sources::capture(ctx, right, sources));
        }
        _ => {}
    }
}

/// Shares the nested RW protocol with a compound expression before its prelude reads the leaf.
pub(crate) fn lower_nested_property_compound_expression(
    ctx: &mut LoweringContext<'_, '_>, target: &Expr, value: &Expr,
    result: Option<&Expr>, prelude: &[Stmt], conditional: Option<&str>, span: Span,
) -> Option<LoweredValue> {
    if conditional.is_some() || !attached_property_root(ctx, target)
        || !matches!(&target.kind, ExprKind::ArrayAccess { array, .. }
            if matches!(array.kind, ExprKind::ArrayAccess { .. })) { return None; }
    let ExprKind::Variable(result_name) = &value.kind else { return None; };
    let (last, prefix) = prelude.split_last()?;
    let StmtKind::Assign { name, value: operation } = &last.kind else { return None; };
    if name != result_name || !matches!(desugared_element_update(operation, span, |read|
        read == target), Some(ElementUpdate::Compound)) { return None; }
    for stmt in prefix { lower_stmt(ctx, stmt); }
    let captured = Expr::new(ExprKind::Assignment {
        target: Box::new(value.clone()), value: Box::new(operation.clone()),
        result_target: None, prelude: Vec::new(), conditional_value_temp: None,
    }, span);
    super::nested_array_writes::lower_nested_array_assign(ctx, target, &captured, span);
    Some(lower_expr(ctx, result.unwrap_or(value)))
}
