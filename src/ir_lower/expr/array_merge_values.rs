//! Purpose:
//! Preserve implicit null from typed array reads at lazy expression merge boundaries.
//!
//! Called from:
//! - Merge-temp stores and short-ternary value lowering.
//!
//! Key details:
//! - Receiver/index side effects and warnings run once, including under `@`.
//! - Explicit casts and string offsets retain their ordinary conversion rules.
//! - A read ending a `?->` chain is left to the chain lowering, which owns its short-circuit.

use super::*;

/// Lowers an array-read arm without erasing a scalar miss before merge boxing.
pub(super) fn lower_array_value_for_merge(
    ctx: &mut LoweringContext<'_, '_>,
    expr: &Expr,
) -> Option<LoweredValue> {
    if let ExprKind::ErrorSuppress(inner) = &expr.kind {
        // Only intercept suppression around an array read; every other expression keeps
        // the ordinary lowering path, including casts that intentionally consume null.
        let mut read = inner.as_ref();
        while let ExprKind::ErrorSuppress(nested) = &read.kind { read = nested; }
        if !is_split_array_read(read) { return None; }
        ctx.emit_void(Op::ErrorSuppressBegin, Vec::new(), None,
            Op::ErrorSuppressBegin.default_effects(), Some(expr.span));
        let value = lower_array_value_for_merge(ctx, inner).expect("array-read suppression");
        ctx.emit_void(Op::ErrorSuppressEnd, Vec::new(), None,
            Op::ErrorSuppressEnd.default_effects(), Some(expr.span));
        return Some(value);
    }
    if !is_split_array_read(expr) { return None; }
    let ExprKind::ArrayAccess { array, index } = &expr.kind else { return None; };
    let receiver = lower_expr(ctx, array);
    if matches!(receiver.ir_type, IrType::Heap(IrHeapKind::Array | IrHeapKind::Hash))
        && !value_is_nullable(ctx, receiver.value)
    {
        return Some(array_access::lower_array_access_from_value_with_null_preservation(
            ctx, receiver, index, expr, true, true,
        ));
    }
    Some(lower_array_access_from_receiver(ctx, receiver, index, expr, true))
}

/// Returns true for an array read whose receiver this module may lower on its own.
///
/// A read ending a `?->` chain (`$o?->items[0]`) is excluded: the chain lowers as one unit,
/// and splitting its last index off would index the boxed chain result (a boxed buffer reads
/// back null) and evaluate the index even when PHP short-circuits the whole chain.
fn is_split_array_read(expr: &Expr) -> bool {
    matches!(expr.kind, ExprKind::ArrayAccess { .. })
        && super::nullsafe_chain::result_storage_type(expr).is_none()
}
