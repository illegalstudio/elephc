//! Purpose:
//! Projects checked integer values while preserving nullable local and reference storage.
//!
//! Called from:
//! - Ordinary variable expressions and by-value closure captures.
//! - Call arguments keep addressable reference reads outside the projection.
//!
//! Key details:
//! - Proofs are source-point facts, never the function's final environment.
//! - The existing TaggedScalar-to-I64 cast preserves the target-aware payload ABI.

use super::*;

/// Loads a local's physical value and applies only an agreed source-point integer view.
pub(super) fn lower_local_value(
    ctx: &mut LoweringContext<'_, '_>, name: &str, span: Span,
) -> LoweredValue {
    let value = ctx.load_local(name, Some(span));
    if projected_local_value_type(ctx, name, span).is_some() && value.ir_type == IrType::TaggedScalar {
        coerce_to_int_at_span(ctx, value, Some(span))
    } else {
        value
    }
}

/// Gives literal and merge planning the same integer shape as the eventual value read.
pub(super) fn projected_local_value_type(
    ctx: &LoweringContext<'_, '_>, name: &str, span: Span,
) -> Option<PhpType> {
    let integer_read = ctx.integer_local_read_sites
        .get(&(ctx.loop_storage_scope.clone(), span, name.to_string())) == Some(&true);
    (integer_read && ctx.local_load_php_type(name)
        .is_some_and(|ty| ty.codegen_repr() == PhpType::TaggedScalar))
        .then_some(PhpType::Int)
}

/// Loads a reference argument's actual local slot without projecting its value view.
pub(super) fn lower_reference_local(
    ctx: &mut LoweringContext<'_, '_>, arg: &Expr,
) -> Option<LoweredValue> {
    let mut place = arg;
    while let ExprKind::NamedArg { value, .. } | ExprKind::ErrorSuppress(value) = &place.kind {
        place = value;
    }
    let ExprKind::Variable(name) = &place.kind else { return None; };
    Some(ctx.load_local(name, Some(place.span)))
}
