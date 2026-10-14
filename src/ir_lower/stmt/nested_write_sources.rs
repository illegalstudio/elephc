//! Purpose:
//! Captures nested store inputs before RHS evaluation without reading the writable parent.
//!
//! Called from:
//! - Nested array assignment and fused deep append lowering.
//!
//! Key details:
//! - Computed keys and property receivers are frozen in PHP source order.
//! - Bare variable keys remain store-time reads.
//! - Heap captures have unwind-visible owners retired in reverse publication order.

use super::*;

/// One captured input whose frame slot may own heap storage.
pub(super) struct WriteSource {
    name: String,
    owns_heap: bool,
}

/// Freezes computed dimensions while leaving the writable root and bare keys relocatable.
pub(super) fn capture_target(
    ctx: &mut LoweringContext<'_, '_>, target: &Expr, sources: &mut Vec<WriteSource>,
) -> Expr {
    let kind = match &target.kind {
        ExprKind::ArrayAccess { array, index } => {
            let array = capture_target(ctx, array, sources);
            let index = if matches!(index.kind, ExprKind::Variable(_)
                | ExprKind::IntLiteral(_) | ExprKind::StringLiteral(_))
            {
                index.as_ref().clone()
            } else { capture(ctx, index, sources) };
            ExprKind::ArrayAccess { array: Box::new(array), index: Box::new(index) }
        }
        ExprKind::PropertyAccess { object, property } => ExprKind::PropertyAccess {
            object: Box::new(capture(ctx, object, sources)), property: property.clone(),
        },
        _ => return target.clone(),
    };
    Expr::new(kind, target.span)
}

/// Publishes one snapshot in a PHP-local slot without leaving an unrooted producer owner.
fn capture(
    ctx: &mut LoweringContext<'_, '_>, expr: &Expr, sources: &mut Vec<WriteSource>,
) -> Expr {
    let value = lower_expr(ctx, expr);
    let ty = ctx.builder.value_php_type(value.value);
    let name = ctx.declare_synthetic_php_local(ty.clone());
    let stored = crate::ir_lower::ownership::acquire_if_refcounted(ctx, value, Some(expr.span));
    ctx.store_call_argument_local(&name, stored, ty.clone(), Some(expr.span));
    if stored.value != value.value && ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(expr.span));
    }
    let owns_heap = crate::ir::Ownership::php_type_needs_lifetime_tracking(&ty)
        && !matches!(ty.codegen_repr(), PhpType::Buffer(_));
    if owns_heap {
        let slot = ctx.local_slots[&name];
        ctx.emit_void(Op::PushCallOperandOwner, Vec::new(), Some(Immediate::LocalSlot(slot)),
            Op::PushCallOperandOwner.default_effects(), Some(expr.span));
    }
    sources.push(WriteSource { name: name.clone(), owns_heap });
    Expr::new(ExprKind::Variable(name), expr.span)
}

/// Clears each captured owner before a possibly throwing destructor can leave its release.
pub(super) fn retire(
    ctx: &mut LoweringContext<'_, '_>, sources: Vec<WriteSource>, span: Span,
) {
    for source in sources.into_iter().rev() {
        if source.owns_heap {
            let slot = ctx.local_slots[&source.name];
            crate::ir_lower::expr::retire_owned_call_operand(ctx, slot, span);
        }
    }
}
