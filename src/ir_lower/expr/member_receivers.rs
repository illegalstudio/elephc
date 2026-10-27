//! Purpose:
//! Preserve checked class/interface views on native interface receiver values.
//!
//! Called from:
//! - Property-read and method-call lowering.
//!
//! Key details:
//! - Only raw object pointers are refined; boxed receivers keep their existing dispatch.
//! - The fresh receiver retains its producer, storage representation and ownership.

use super::*;

/// Lowers a receiver once and applies the checker's view without changing its local slot.
pub(super) fn lower_member_receiver(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    member: &Expr,
) -> LoweredValue {
    let value = lower_expr(ctx, object);
    refine_member_receiver(ctx, value, member)
}

/// Applies the same view to a receiver already evaluated by a lazy postfix chain.
pub(super) fn refine_member_receiver(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    member: &Expr,
) -> LoweredValue {
    let original = ctx.builder.value_php_type(value.value);
    let Some(PhpType::Object(class)) = checked_member_receiver_type(ctx, &original, member) else {
        return value;
    };
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
    let PhpType::Object(original) = original else { return None; };
    if !ctx.interfaces.contains_key(original) { return None; }
    let Some(Some(PhpType::Object(class))) = ctx.member_receiver_types
        .get(&(ctx.loop_storage_scope.clone(), member.span))
    else {
        return None;
    };
    (ctx.classes.contains_key(class) || ctx.interfaces.contains_key(class))
        .then(|| PhpType::Object(class.clone()))
}
