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
    let declared_storage = matches!(
        ctx.builder.value_defining_op(value.value),
        Some(Op::PropGet | Op::LoadStaticProperty)
    );
    let Some(PhpType::Object(class)) =
        checked_member_receiver_type_at_span(ctx, &original, span, declared_storage) else {
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
///
/// `declared_storage` is true when the receiver is read from a declared property, whose
/// interface type the program wrote and the checker did not infer.
pub(super) fn checked_member_receiver_type(
    ctx: &LoweringContext<'_, '_>,
    original: &PhpType,
    member: &Expr,
    declared_storage: bool,
) -> Option<PhpType> {
    checked_member_receiver_type_at_span(ctx, original, member.span, declared_storage)
}

/// Looks up a raw-object receiver view shared by expression and statement mutation sites.
///
/// A receiver read from a declared property (`$holder->item instanceof Label`) keeps the checked
/// class whatever else implements the interface: its declared type is the program's, so the
/// `instanceof` narrowing is sound. Only an inferred receiver, an untyped parameter the checker
/// typed by its first call site, needs the sibling-implementer guard below.
fn checked_member_receiver_type_at_span(
    ctx: &LoweringContext<'_, '_>,
    original: &PhpType,
    span: Span,
    declared_storage: bool,
) -> Option<PhpType> {
    let (original, nullable) = singular_object_class(original)?;
    // Raw concrete views remain valid; nullable projection requires a checked interface view.
    if nullable && !ctx.interfaces.contains_key(original) { return None; }
    let Some(Some(PhpType::Object(class))) = ctx.member_receiver_types
        .get(&(ctx.loop_storage_scope.clone(), span))
    else {
        return None;
    };
    if !(ctx.classes.contains_key(class) || ctx.interfaces.contains_key(class)) {
        return None;
    }
    // An interface-typed receiver the checker narrowed to ONE class is only that class when
    // nothing else implements the interface. The checker types an untyped parameter by the first
    // class a call site passes, so `function d($x) { $x instanceof Coll && $x->first(); }` called
    // with `IntColl` and then `Sq` records `IntColl` for the call: retyping the receiver to it
    // dispatched `Sq`'s object through `IntColl`'s method slot.
    if !declared_storage
        && original != class
        && ctx.interfaces.contains_key(original)
        && !ctx.interfaces.contains_key(class)
        && interface_has_implementer_outside(ctx, original, class)
    {
        return None;
    }
    Some(PhpType::Object(class.clone()))
}

/// Whether some declared class implements `interface` without being `class` or one of its
/// descendants.
fn interface_has_implementer_outside(
    ctx: &LoweringContext<'_, '_>,
    interface: &str,
    class: &str,
) -> bool {
    ctx.classes.keys().any(|name| {
        name != class
            && crate::types::instanceof_intersection::object_type_satisfies(
                ctx.classes,
                ctx.interfaces,
                name,
                interface,
            )
            && !crate::types::instanceof_intersection::object_type_satisfies(
                ctx.classes,
                ctx.interfaces,
                name,
                class,
            )
    })
}
