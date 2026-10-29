//! Purpose:
//! Keeps ordinary property incdec arithmetic out of PHP capture locals during EIR lowering.
//!
//! Called from:
//! - Assignment-expression lowering after native XML updates have declined.
//!
//! Key details:
//! - Preserves the parser's single old-value read, overflow checks, and stored prefix result.
//! - Leaves native updates and every non-canonical assignment on their original paths.

use super::*;

/// Consumes an ordinary property's checked increment directly without a frame-lifetime Mixed local.
/// The parser's old capture preserves one getter call; prefix results still reread the stored type.
pub(super) fn without_capture<'a>(
    target: &Expr, value: &'a Expr, result: Option<&Expr>, prelude: &'a [Stmt],
    conditional: Option<&str>,
) -> (&'a Expr, &'a [Stmt]) {
    let unchanged = (value, prelude);
    if conditional.is_some() || !matches!(target.kind,
        ExprKind::PropertyAccess { .. } | ExprKind::DynamicPropertyAccess { .. }) { return unchanged; }
    let ExprKind::Variable(name) = &value.kind else { return unchanged; };
    if !crate::names::is_generated_local_name(name) || !name.contains("__elephc_incdec_new_") {
        return unchanged;
    }
    let Some((last, earlier)) = prelude.split_last() else { return unchanged; };
    let StmtKind::Assign { name: captured, value: operation } = &last.kind else { return unchanged; };
    if captured != name { return unchanged; }
    let ExprKind::BinaryOp { left, op: BinOp::Add | BinOp::Sub, right } = &operation.kind else { return unchanged; };
    if !matches!(right.kind, ExprKind::IntLiteral(1)) { return unchanged; }
    let ExprKind::Variable(old) = &left.kind else { return unchanged; };
    if !crate::names::is_generated_local_name(old) || !old.contains("__elephc_incdec_old_") {
        return unchanged;
    }
    if !matches!(earlier.last().map(|stmt| &stmt.kind),
        Some(StmtKind::Assign { name, value }) if name == old && value == target) { return unchanged; }
    if !result.is_some_and(|result| result == target
        || matches!(&result.kind, ExprKind::Variable(name) if name == old)) { return unchanged; }
    (operation, earlier)
}
