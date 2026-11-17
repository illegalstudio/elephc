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
    capture_target_inner(ctx, target, sources, false)
}

/// Keeps storage-only property receivers late while freezing computed update inputs once.
pub(super) fn capture_update_target(
    ctx: &mut LoweringContext<'_, '_>, target: &Expr, sources: &mut Vec<WriteSource>,
) -> Expr {
    capture_target_inner(ctx, target, sources, true)
}

/// Recognizes a parser-scoped runtime property name shared by an indexed read and write.
pub(super) fn scoped_runtime_property_root(target: &Expr) -> bool {
    let mut root = target;
    while let ExprKind::ArrayAccess { array, .. } = &root.kind { root = array; }
    matches!(&root.kind, ExprKind::DynamicPropertyAccess { property, .. }
        if matches!(&property.kind, ExprKind::Variable(name)
            if name.starts_with(crate::parser::ast::SCOPED_WRITE_TEMP_PREFIX)
                && crate::names::is_generated_local_name(name)))
}

/// Shares dimension capture while preserving the caller's receiver-selection timing.
fn capture_target_inner(
    ctx: &mut LoweringContext<'_, '_>, target: &Expr, sources: &mut Vec<WriteSource>,
    late_receiver: bool,
) -> Expr {
    let kind = match &target.kind {
        ExprKind::ArrayAccess { array, index } => {
            let array = capture_target_inner(ctx, array, sources, late_receiver);
            let index = if matches!(index.kind, ExprKind::Variable(_)
                | ExprKind::IntLiteral(_) | ExprKind::StringLiteral(_))
            {
                index.as_ref().clone()
            } else { capture(ctx, index, sources) };
            ExprKind::ArrayAccess { array: Box::new(array), index: Box::new(index) }
        }
        ExprKind::PropertyAccess { object, property } => ExprKind::PropertyAccess {
            object: Box::new(if late_receiver
                && super::property_array_writes::deferred_property_receiver(ctx, object) {
                object.as_ref().clone()
            } else { capture(ctx, object, sources) }), property: property.clone(),
        },
        ExprKind::DynamicPropertyAccess { object, property } if late_receiver => {
            let object = if super::property_array_writes::deferred_property_receiver(ctx, object) {
                object.as_ref().clone()
            } else { capture(ctx, object, sources) };
            let name = crate::ir_lower::scoped_writes::property_name(ctx, property, target.span)
                .unwrap_or_else(|| {
                    let value = lower_expr(ctx, property);
                    crate::ir_lower::expr::coerce_runtime_property_name(ctx, value, target.span)
                });
            let name = capture_lowered(ctx, name, target.span, sources);
            ExprKind::DynamicPropertyAccess { object: Box::new(object), property: Box::new(name) }
        }
        _ => return target.clone(),
    };
    Expr::new(kind, target.span)
}

/// Publishes one snapshot in a PHP-local slot without leaving an unrooted producer owner.
pub(super) fn capture(
    ctx: &mut LoweringContext<'_, '_>, expr: &Expr, sources: &mut Vec<WriteSource>,
) -> Expr {
    let value = lower_expr(ctx, expr);
    capture_lowered(ctx, value, expr.span, sources)
}

/// Publishes an already evaluated source after its caller checks for a terminated RHS.
pub(super) fn capture_lowered(
    ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, span: Span, sources: &mut Vec<WriteSource>,
) -> Expr {
    let ty = ctx.builder.value_php_type(value.value);
    let name = ctx.declare_synthetic_php_local(ty.clone());
    let stored = crate::ir_lower::ownership::acquire_if_refcounted(ctx, value, Some(span));
    ctx.store_call_argument_local(&name, stored, ty.clone(), Some(span));
    if stored.value != value.value && ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
    let owns_heap = crate::ir::Ownership::php_type_needs_lifetime_tracking(&ty)
        && !matches!(ty.codegen_repr(), PhpType::Buffer(_));
    if owns_heap {
        let slot = ctx.local_slots[&name];
        ctx.emit_void(Op::PushCallOperandOwner, Vec::new(), Some(Immediate::LocalSlot(slot)),
            Op::PushCallOperandOwner.default_effects(), Some(span));
    }
    sources.push(WriteSource { name: name.clone(), owns_heap });
    Expr::new(ExprKind::Variable(name), span)
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
