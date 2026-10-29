//! Purpose:
//! Keeps ordinary property incdec arithmetic out of PHP capture locals during EIR lowering.
//!
//! Called from:
//! - Assignment-expression lowering after native XML updates have declined.
//!
//! Key details:
//! - Preserves the parser's single old-value read, overflow checks, and stored prefix result.
//! - Only scalar old operands qualify, after their actual storage has been lowered.
//! - Native updates and non-canonical assignments stay on their original paths.

use super::*;

/// Finds canonical arithmetic whose old capture can prove that no overloaded owner is involved.
pub(super) fn captured_arithmetic<'a>(
    target: &Expr, value: &'a Expr, result: Option<&Expr>, prelude: &'a [Stmt],
    conditional: Option<&str>,
) -> Option<(&'a Expr, &'a str)> {
    if conditional.is_some() || !matches!(target.kind,
        ExprKind::PropertyAccess { .. } | ExprKind::DynamicPropertyAccess { .. }) { return None; }
    let ExprKind::Variable(name) = &value.kind else { return None; };
    if !crate::names::is_generated_local_name(name) || !name.contains("__elephc_incdec_new_") {
        return None;
    }
    let Some((last, earlier)) = prelude.split_last() else { return None; };
    let StmtKind::Assign { name: captured, value: operation } = &last.kind else { return None; };
    if captured != name { return None; }
    let ExprKind::BinaryOp { left, op: BinOp::Add | BinOp::Sub, right } = &operation.kind else { return None; };
    if !matches!(right.kind, ExprKind::IntLiteral(1)) { return None; }
    let ExprKind::Variable(old) = &left.kind else { return None; };
    if !crate::names::is_generated_local_name(old) || !old.contains("__elephc_incdec_old_") {
        return None;
    }
    if !matches!(earlier.last().map(|stmt| &stmt.kind),
        Some(StmtKind::Assign { name, value }) if name == old && value == target) { return None; }
    if !result.is_some_and(|result| result == target
        || matches!(&result.kind, ExprKind::Variable(name) if name == old)) { return None; }
    Some((operation, old))
}

/// Tests the old capture after its getter ran, preserving opaque and object arithmetic lifetimes.
pub(super) fn scalar_operand(ctx: &LoweringContext<'_, '_>, old: &str) -> bool {
    scalar_storage(&ctx.local_type(old))
}

/// Scalar declarations and their nullable unions cannot hide an overloaded object owner.
fn scalar_storage(ty: &PhpType) -> bool {
    match ty {
        PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::False | PhpType::Str
        | PhpType::Void | PhpType::TaggedScalar => true,
        PhpType::Union(members) => !members.is_empty() && members.iter().all(scalar_storage),
        _ => false,
    }
}
