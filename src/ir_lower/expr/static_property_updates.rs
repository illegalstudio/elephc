//! Purpose:
//! Shares one guarded property-chain receiver between a compound read and its final write.
//!
//! Called from:
//! - Assignment-expression lowering in `crate::ir_lower::expr::assignments`.
//!
//! Key details:
//! - The captured RHS precedes static traversal and null guards precede property reads.
//! - The receiver lease remains unwind-visible until both halves of the update finish.

use super::*;
use crate::ir_lower::stmt::property_write_receiver::PropertyWriteReceiver;

/// One cached target, its independent lease and the scratch alias used by both update halves.
pub(super) struct StaticCompoundReceiver {
    pub(super) target: Expr,
    receiver: PropertyWriteReceiver,
    temp: String,
}

/// Retires the cached compound receiver after the read and write have used the same chain.
pub(super) fn finish_static_compound_receiver(
    ctx: &mut LoweringContext<'_, '_>, receiver: Option<StaticCompoundReceiver>, span: Span,
) {
    if let Some(receiver) = receiver {
        ctx.emit_void(Op::UnsetLocal, Vec::new(), Some(Immediate::LocalSlot(ctx.local_slots[&receiver.temp])),
            Op::UnsetLocal.default_effects(), Some(span));
        receiver.receiver.finish(ctx, span);
    }
}

/// Checks and captures the delayed receiver before lowering a desugared compound property read.
pub(super) fn guard_static_compound_property_read(
    ctx: &mut LoweringContext<'_, '_>, target: &Expr, value: &Expr, stmt: &Stmt, span: Span,
) -> Option<StaticCompoundReceiver> {
    let ExprKind::PropertyAccess { object, property } = &target.kind else { return None; };
    if !is_static_property_write_chain(object) { return None; }
    let ExprKind::Variable(result) = &value.kind else { return None; };
    let StmtKind::Assign { name, value } = &stmt.kind else { return None; };
    let ExprKind::BinaryOp { left, op, right } = &value.kind else { return None; };
    if name != result || left.as_ref() != target { return None; }
    let rhs = ctx.with_borrowed_write_operand(|ctx| lower_expr(ctx, right));
    let mut receiver = lower_static_property_write_chain(ctx, object, rhs, span);
    receiver.narrow_for_assignment(ctx, property, rhs, span);
    let ty = ctx.builder.value_php_type(receiver.value.value);
    let temp = ctx.declare_hidden_temp(ty.clone());
    let borrowed = ctx.emit_value(Op::Borrow, vec![receiver.value.value], None, ty.clone(),
        Op::Borrow.default_effects(), Some(span));
    ctx.builder.set_value_ownership(borrowed.value, Ownership::Borrowed);
    ctx.store_local(&temp, borrowed, ty, Some(span));
    let target = Expr::new(ExprKind::PropertyAccess {
        object: Box::new(Expr::new(ExprKind::Variable(temp.clone()), object.span)),
        property: property.clone(),
    }, left.span);
    let value = Expr::new(ExprKind::BinaryOp {
        left: Box::new(target.clone()), op: op.clone(), right: right.clone(),
    }, value.span);
    crate::ir_lower::stmt::lower_stmt(ctx, &Stmt::new(StmtKind::Assign { name: name.clone(), value }, stmt.span));
    Some(StaticCompoundReceiver { target, receiver, temp })
}
