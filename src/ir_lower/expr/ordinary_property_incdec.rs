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
use super::array_cast_source::array_cast_source_type_for_ir;

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
    // Constant folding removes a prefix readback equal to the target.
    if !result.is_none_or(|result| result == target
        || matches!(&result.kind, ExprKind::Variable(name) if name == old)) { return None; }
    Some((operation, old))
}

/// Tests the old capture after its getter ran, preserving opaque and object arithmetic lifetimes.
pub(super) fn scalar_operand(ctx: &LoweringContext<'_, '_>, old: &str) -> bool {
    scalar_storage(&ctx.local_type(old))
}

/// A folded prefix can reread ordinary storage after weak property coercion, without user code.
pub(super) fn plain_prefix_readback(ctx: &LoweringContext<'_, '_>, target: &Expr) -> bool {
    let (object, property) = match &target.kind {
        ExprKind::PropertyAccess { object, property } => (object.as_ref(), property.as_str()),
        ExprKind::DynamicPropertyAccess { object, property } => {
            let ExprKind::StringLiteral(property) = &property.kind else { return false; };
            (object.as_ref(), property.as_str())
        }
        _ => return false,
    };
    let ty = array_cast_source_type_for_ir(ctx, object);
    let Some((class, _)) = property_access::singular_object_class(&ty) else { return false; };
    let Some(info) = ctx.classes.get(class) else { return false; };
    if !info.properties.iter().any(|(name, _)| name == property)
        || info.property_hooks.contains_key(property) { return false; }
    // A runtime subclass may provide magic access after an unset. Keep that original
    // captured-result path rather than introducing an observable second getter call.
    !ctx.classes.iter().any(|(candidate, info)| {
        if !info.methods.contains_key("__get") && !info.methods.contains_key("__set") { return false; }
        let mut ancestor = Some(candidate.as_str());
        while let Some(name) = ancestor {
            if name == class { return true; }
            ancestor = ctx.classes.get(name).and_then(|info| info.parent.as_deref());
        }
        false
    })
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
