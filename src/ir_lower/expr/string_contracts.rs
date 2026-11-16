//! Purpose:
//! Enforces original string operand and output formatting contracts through EIR guards.
//!
//! Called from:
//! - Concatenation, interpolation, echo and print lowering before conversion or output.
//!
//! Key details:
//! - Kind predicates never call Stringable hooks or convert a rejected operand.
//! - Formatting permits existing scalar/object handling while refusing implicit arrays.

use super::*;

/// Requires an original string without enabling the independent Coercions guarantee.
pub(super) fn concatenation(
    ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, span: Span,
) -> LoweredValue {
    if crate::semantics::strings::active(ctx.semantic_profile, ctx.source_mode) {
        // Concrete string storage can still hold a typed-array miss. Ownership
        // stabilization preserves that logical null, so inspect it before conversion.
        if ctx.builder.value_php_type(value.value).codegen_repr() == PhpType::Str {
            let pins = pin_in_flight_owners(ctx, &[value.value], span);
            let missing = ctx.emit_value(Op::IsNull, vec![value.value], None, PhpType::Bool,
                Op::IsNull.default_effects(), Some(span));
            crate::ir_lower::semantic_guards::require_false(ctx, missing, Op::ThrowTypeError,
                crate::semantics::strings::CONCAT_MESSAGE, Some(span));
            unpin_in_flight_owners(ctx, pins, span);
        }
        crate::ir_lower::semantic_guards::require_kind(ctx, value,
            crate::ir::PhpTypePredicate::String, "strict-strings", Some(span))
    } else { value }
}

/// Refuses array formatting before an output or interpolation helper can print Array.
pub(in crate::ir_lower) fn formatting(
    ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, span: Span,
) -> LoweredValue {
    if !crate::semantics::strings::active(ctx.semantic_profile, ctx.source_mode) { return value; }
    let pins = pin_in_flight_owners(ctx, &[value.value], span);
    let array = crate::ir_lower::semantic_guards::matches_any_kind(
        ctx, value, &[crate::ir::PhpTypePredicate::Array], Some(span));
    crate::ir_lower::semantic_guards::require_false(ctx, array, Op::ThrowTypeError,
        crate::semantics::strings::FORMAT_MESSAGE, Some(span));
    unpin_in_flight_owners(ctx, pins, span);
    value
}
