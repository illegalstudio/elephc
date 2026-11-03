//! Purpose:
//! Preserve checked class/interface views on native object receiver values.
//!
//! Called from:
//! - Property reads, writes, unset probes and method-call lowering.
//!
//! Key details:
//! - Checked non-null interface views may project nullable boxes to raw object pointers.
//! - The fresh receiver retains its producer, storage representation and ownership.

use super::*;

/// Lowers a receiver once and applies the checker's view without changing its local slot.
pub(super) fn lower_member_receiver(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    member: &Expr,
) -> LoweredValue {
    lower_member_receiver_at_span(ctx, object, member.span)
}

/// Applies a statement mutation's checked view before its RHS or runtime name is evaluated.
pub(crate) fn lower_member_receiver_at_span(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    span: Span,
) -> LoweredValue {
    let value = lower_expr(ctx, object);
    refine_member_receiver_at_span(ctx, value, span)
}

/// Applies the same view to a receiver already evaluated by a lazy postfix chain.
pub(super) fn refine_member_receiver(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    member: &Expr,
) -> LoweredValue {
    refine_member_receiver_at_span(ctx, value, member.span)
}

/// Refines a freshly evaluated receiver using the checked member's source identity.
pub(crate) fn refine_member_receiver_at_span(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
) -> LoweredValue {
    let original = ctx.builder.value_php_type(value.value);
    let Some(PhpType::Object(class)) = checked_member_receiver_type_at_span(ctx, &original, span) else {
        return value;
    };
    if matches!(original, PhpType::Union(_)) {
        return coerce_value_for_temp(ctx, value, &PhpType::Object(class), span);
    }
    if original != PhpType::Object(class.clone()) {
        ctx.builder.set_object_value_class(value.value, class);
    }
    value
}

/// Resolves the checked raw-object view before a silent probe chooses its property behavior.
pub(super) fn checked_member_receiver_type(
    ctx: &LoweringContext<'_, '_>,
    original: &PhpType,
    member: &Expr,
) -> Option<PhpType> {
    checked_member_receiver_type_at_span(ctx, original, member.span)
}

/// Looks up a raw-object receiver view shared by expression and statement mutation sites.
fn checked_member_receiver_type_at_span(
    ctx: &LoweringContext<'_, '_>,
    original: &PhpType,
    span: Span,
) -> Option<PhpType> {
    let (original, nullable) = singular_object_class(original)?;
    // Raw concrete views remain valid; nullable projection requires a checked interface view.
    if nullable && !ctx.interfaces.contains_key(original) { return None; }
    let Some(Some(PhpType::Object(class))) = ctx.member_receiver_types
        .get(&(ctx.loop_storage_scope.clone(), span))
    else {
        return None;
    };
    (ctx.classes.contains_key(class) || ctx.interfaces.contains_key(class))
        .then(|| PhpType::Object(class.clone()))
}
