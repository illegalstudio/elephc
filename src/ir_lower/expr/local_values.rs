//! Purpose:
//! Applies checked value contracts while preserving nullable local and reference storage.
//!
//! Called from:
//! - Ordinary variable expressions and by-value closure captures.
//! - Call arguments keep addressable reference reads outside the projection.
//!
//! Key details:
//! - Proofs are source-point facts, never the function's final environment.
//! - The existing TaggedScalar-to-I64 cast preserves the target-aware payload ABI.

use super::*;

/// Loads physical storage, then projects a proven integer or boxes an uncertain snapshot.
pub(super) fn lower_local_value(
    ctx: &mut LoweringContext<'_, '_>, name: &str, span: Span,
) -> LoweredValue {
    if name == "GLOBALS" {
        return ctx.emit_owned_value(
            Op::LoadGlobalsArray,
            Vec::new(),
            None,
            PhpType::AssocArray {
                key: Box::new(PhpType::Str),
                value: Box::new(PhpType::Mixed),
            },
            Op::LoadGlobalsArray.default_effects(),
            Some(span),
        );
    }
    // A branch-local view must not reinterpret a two-word frame slot as a raw scalar.
    let checked = ctx.local_value_read_types.get(&(ctx.loop_storage_scope.clone(), span, name.to_string()));
    let boxed_read = checked == Some(&PhpType::Mixed);
    let reference_view = checked.filter(|ty| ty.codegen_repr() == PhpType::TaggedScalar
        && ctx.is_ref_bound_local(name)).cloned();
    let physical = reference_view.or_else(|| ctx.local_slots.get(name).and_then(|slot| {
        let local = &ctx.builder.function().locals[slot.as_raw() as usize];
        (local.ir_type == IrType::TaggedScalar).then(|| local.php_type.clone())
    }));
    let previous = physical.as_ref().and_then(|physical|
        ctx.local_types.insert(name.to_string(), physical.clone()));
    let value = ctx.load_local(name, Some(span));
    if physical.is_some() {
        if let Some(previous) = previous { ctx.local_types.insert(name.to_string(), previous); }
        else { ctx.local_types.remove(name); }
    }
    if boxed_read && value.ir_type == IrType::TaggedScalar {
        ctx.box_value_as_mixed(value, PhpType::Mixed, Some(span))
    } else if projected_local_value_type(ctx, name, span).is_some() && value.ir_type == IrType::TaggedScalar {
        coerce_to_int_at_span(ctx, value, Some(span))
    } else {
        value
    }
}

/// Gives literal and merge planning the same integer shape as the eventual value read.
pub(super) fn projected_local_value_type(
    ctx: &LoweringContext<'_, '_>, name: &str, span: Span,
) -> Option<PhpType> {
    let integer_read = ctx.local_value_read_types
        .get(&(ctx.loop_storage_scope.clone(), span, name.to_string())) == Some(&PhpType::Int);
    let tagged = ctx.local_slots.get(name).is_some_and(|slot|
        ctx.builder.function().locals[slot.as_raw() as usize].ir_type == IrType::TaggedScalar)
        || ctx.local_load_php_type(name).is_some_and(|ty| ty.codegen_repr() == PhpType::TaggedScalar);
    (integer_read && tagged)
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
