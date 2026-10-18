//! Purpose:
//! Coordinates native WeakMap entry cells for increment and mutating consumers.
//! Stages stable receivers and balances managed entry leases.
//!
//! Called from:
//! - Update statements/expressions, nested append and literal settype lowering.
//!
//! Key details:
//! - Receiver and key are evaluated once and remain owned throughout RHS evaluation.
//! - The ordinary call ledger retires operand and entry-cell leases on success and throws.

use super::*;

/// Updates a parser-generated increment through its managed entry cell.
pub(crate) fn lower_weak_map_incdec_stmt(
    ctx: &mut LoweringContext<'_, '_>,
    value: &Expr,
    span: Span,
) -> bool {
    let ExprKind::BinaryOp { left, op, right } = &value.kind else { return false; };
    if !implicit_increment_one(op, right, value.span) { return false; }
    let Some(variable) = begin_entry_update(ctx, left, span) else { return false; };
    let ExprKind::Variable(name) = &variable.kind else { unreachable!("entry alias is a local"); };
    let result = lower_inc_dec(ctx, name, matches!(op, BinOp::Add), false, value);
    if ctx.value_needs_release_after_use(result) {
        crate::ir_lower::ownership::release_if_owned(ctx, result, Some(span));
    }
    finish_entry_update(ctx);
    true
}

/// Redirects the parser's old/new expression snapshots through the same entry cell.
pub(super) fn lower_weak_map_incdec_expression(
    ctx: &mut LoweringContext<'_, '_>,
    target: &Expr,
    value: &Expr,
    result_target: Option<&Expr>,
    prelude: &[crate::parser::ast::Stmt],
    expr: &Expr,
) -> Option<LoweredValue> {
    if !matches!(&target.kind, ExprKind::ArrayAccess { array, .. }
        if instance_object_class_for_ir(ctx, array)
            .is_some_and(|class| class.eq_ignore_ascii_case("WeakMap")))
        || prelude.len() < 2
    { return None; }
    let split = prelude.len() - 2;
    let StmtKind::Assign { name: old, value: read } = &prelude[split].kind else { return None; };
    let StmtKind::Assign { name: new, value: next } = &prelude[split + 1].kind else { return None; };
    let ExprKind::BinaryOp { left, op, right } = &next.kind else { return None; };
    if read != target || !crate::names::is_generated_local_name(old)
        || !crate::names::is_generated_local_name(new)
        || !matches!(&left.kind, ExprKind::Variable(name) if name == old)
        || !matches!(&value.kind, ExprKind::Variable(name) if name == new)
        || !result_target.is_some_and(|result|
            matches!(&result.kind, ExprKind::Variable(name) if name == old || name == new))
        || !implicit_increment_one(op, right, expr.span)
    { return None; }
    for stmt in &prelude[..split] { crate::ir_lower::stmt::lower_stmt(ctx, stmt); }
    let variable = begin_entry_update(ctx, target, expr.span)?;
    let mut remaining = prelude[split..].to_vec();
    if let StmtKind::Assign { value, .. } = &mut remaining[0].kind { *value = variable.clone(); }
    let ExprKind::Variable(name) = variable.kind else { unreachable!("entry alias is a local"); };
    let increment = if matches!(op, BinOp::Add) {
        ExprKind::PreIncrement(name)
    } else { ExprKind::PreDecrement(name) };
    if let StmtKind::Assign { value, .. } = &mut remaining[1].kind {
        *value = Expr::new(increment, expr.span);
    }
    for stmt in &remaining { crate::ir_lower::stmt::lower_stmt(ctx, stmt); }
    let result = lower_expr(ctx, result_target.expect("validated incdec result"));
    finish_entry_update(ctx);
    Some(result)
}

/// Distinguishes the parser's implicit one in ++/-- from a source literal in +=/-=.
fn implicit_increment_one(op: &BinOp, right: &Expr, span: Span) -> bool {
    matches!(op, BinOp::Add | BinOp::Sub)
        && matches!(right.kind, ExprKind::IntLiteral(1)) && right.span == span
}

/// Pins the receiver/key and publishes the entry alias in one unwind ledger.
pub(super) fn begin_entry_update(ctx: &mut LoweringContext<'_, '_>, target: &Expr, span: Span) -> Option<Expr> {
    let ExprKind::ArrayAccess { array, index } = &target.kind else { return None; };
    if !instance_object_class_for_ir(ctx, array)
        .is_some_and(|class| class.eq_ignore_ascii_case("WeakMap"))
    {
        return None;
    }
    begin_call_argument_evaluation(ctx);
    let receiver = stage_update_operand(ctx, array);
    let key = stage_update_operand(ctx, index);
    let source = Expr::new(ExprKind::ArrayAccess {
        array: Box::new(receiver), index: Box::new(key),
    }, target.span);
    let alias = ctx.declare_synthetic_php_local(PhpType::Mixed);
    lower_ref_assign_array_elem(ctx, &alias, &source, span);
    publish_scoped_ref_receiver_alias(ctx, &alias, span);
    Some(Expr::new(ExprKind::Variable(alias), target.span))
}

/// Releases the implicit entry alias before its frozen key and receiver.
pub(super) fn finish_entry_update(ctx: &mut LoweringContext<'_, '_>) {
    let owners = finish_call_argument_evaluation(ctx, &mut []);
    retire_call_argument_intermediates(ctx, &owners);
}

/// Appends through the entry reference instead of replacing its array value on write-back.
pub(crate) fn lower_weak_map_append(
    ctx: &mut LoweringContext<'_, '_>, target: &Expr, value: &Expr, span: Span,
) -> bool {
    let Some(variable) = begin_entry_update(ctx, target, span) else { return false; };
    let ExprKind::Variable(array) = variable.kind else { unreachable!("entry alias is a local"); };
    let push = crate::parser::ast::Stmt::new(StmtKind::ArrayPush {
        array, value: value.clone(),
    }, span);
    crate::ir_lower::stmt::lower_stmt(ctx, &push);
    finish_entry_update(ctx);
    true
}

/// Freezes one operand in an unwind-visible owner or a non-owning scalar scratch slot.
fn stage_update_operand(ctx: &mut LoweringContext<'_, '_>, expr: &Expr) -> Expr {
    let value = lower_expr(ctx, expr);
    let ty = ctx.builder.value_php_type(value.value);
    let name = ctx.declare_synthetic_php_local(ty.clone());
    ctx.store_local(&name, value, ty.clone(), Some(expr.span));
    if Ownership::php_type_needs_lifetime_tracking(&ty) {
        publish_scoped_value_local(ctx, &name, expr.span);
    }
    Expr::new(ExprKind::Variable(name), expr.span)
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Pins the parser distinction between implicit increment and source compound operands.
    //!
    //! Called from:
    //! - The EIR expression lowering unit tests.
    //!
    //! Key details:
    //! - Synthetic update groups retain the implicit one's update span.

    use super::*;

    /// Actual ++ and +=1 parses remain distinguishable before lowering.
    #[test]
    fn increment_one_origin_survives_statement_desugaring() {
        let tokens = crate::lexer::tokenize("<?php $map[$key]++; $map[$key] += 1;").unwrap();
        let program = crate::parser::parse_internal(&tokens).unwrap();
        let mut origins = Vec::new();
        for stmt in &program { collect_update_origins(stmt, &mut origins); }
        assert_eq!(origins, vec![true, false]);
    }

    /// Collects updates through parser-generated index snapshot groups.
    fn collect_update_origins(stmt: &crate::parser::ast::Stmt, origins: &mut Vec<bool>) {
        match &stmt.kind {
            StmtKind::Synthetic(body) => {
                for stmt in body { collect_update_origins(stmt, origins); }
            }
            StmtKind::ArrayAssign { value, .. } => {
                let ExprKind::BinaryOp { op, right, .. } = &value.kind else { panic!("expected update"); };
                origins.push(implicit_increment_one(op, right, value.span));
            }
            _ => {}
        }
    }
}
