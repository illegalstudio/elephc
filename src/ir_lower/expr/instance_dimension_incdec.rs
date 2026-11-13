//! Purpose:
//! Reuses write-context selection for native-capable indexed incdec expressions.
//!
//! Called from:
//! - Assignment expression lowering before replaying parser capture statements.
//!
//! Key details:
//! - Recognizes only the parser's canonical element arithmetic captures.
//! - The statement writer publishes both results without replaying getters.
//! - Concrete ordinary arrays and buffers retain their established lowering.

use super::*;

/// Converts canonical element arithmetic to the shared old-value and working-copy kernel.
pub(super) fn lower(
    ctx: &mut LoweringContext<'_, '_>, target: &Expr, value: &Expr,
    result: Option<&Expr>, prelude: &[Stmt], conditional: Option<&str>, expr: &Expr,
) -> Option<LoweredValue> {
    if conditional.is_some() { return None; }
    let ExprKind::ArrayAccess { array, .. } = &target.kind else { return None; };
    if !matches!(array.kind, ExprKind::Variable(_) | ExprKind::PropertyAccess { .. }
        | ExprKind::DynamicPropertyAccess { .. }) { return None; }
    let ty = array_cast_source::array_cast_source_type_for_ir(ctx, array);
    if !property_access::may_have_native_xml_dimensions(ctx, &ty) { return None; }
    let (operation, earlier) = prelude.split_last()?;
    let StmtKind::Assign { name: next, value: operation } = &operation.kind else { return None; };
    let (read, captures) = earlier.split_last()?;
    let StmtKind::Assign { name: old, value: read } = &read.kind else { return None; };
    if read != target || !crate::names::is_generated_local_name(old)
        || !old.contains("__elephc_incdec_old_")
        || !matches!(&value.kind, ExprKind::Variable(name) if name == next) { return None; }
    let result = result?;
    if !matches!(&result.kind, ExprKind::Variable(name) if name == old || name == next) { return None; }
    let ExprKind::BinaryOp { left, op, right } = &operation.kind else { return None; };
    if !matches!(&left.kind, ExprKind::Variable(name) if name == old)
        || !matches!(right.kind, ExprKind::IntLiteral(1)) { return None; }
    let increment = match op { BinOp::Add => true, BinOp::Sub => false, _ => return None };
    let span = expr.span;
    if earlier.last()?.span != span || operation.span != span { return None; }
    let kernel = Expr::new(ExprKind::Assignment {
        target: Box::new(Expr::new(ExprKind::Variable(next.clone()), span)),
        value: Box::new(Expr::new(if increment { ExprKind::PreIncrement(next.clone()) }
            else { ExprKind::PreDecrement(next.clone()) }, span)),
        result_target: None,
        prelude: vec![
            Stmt::new(StmtKind::Assign { name: old.clone(), value: target.clone() }, span),
            Stmt::new(StmtKind::Assign { name: next.clone(),
                value: Expr::new(ExprKind::Variable(old.clone()), span) }, span),
        ], conditional_value_temp: None,
    }, span);
    let statement = crate::parser::stmt::assignment_target_store_stmt(target.clone(), kernel, span).ok()?;
    for capture in captures { crate::ir_lower::stmt::lower_stmt(ctx, capture); }
    crate::ir_lower::stmt::lower_stmt(ctx, &Stmt::new(statement, span));
    if ctx.builder.insertion_block_is_terminated() {
        return Some(LoweredValue { value: ctx.builder.emit_const_null(), ir_type: IrType::I64 });
    }
    let result = lower_expr(ctx, result);
    let result = ctx.box_value_as_mixed(result, PhpType::Mixed, Some(span));
    Some(crate::ir_lower::ownership::copy_assignment_value(ctx, result, Some(span)))
}
