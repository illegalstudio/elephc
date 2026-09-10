//! Purpose:
//! Instanceof and scalar coercion helpers.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers `instanceof`.
///
/// Native and eval predicates borrow their operands. Retire independent owners,
/// including class-name strings detached from widened Mixed locals, after use.
/// Root an owning value operand before evaluating a dynamic target that may throw;
/// a local read can also own a detached payload independently of its source slot.
/// Borrowed objects and the scalar result keep their existing lifetimes.
pub(super) fn lower_instanceof(
    ctx: &mut LoweringContext<'_, '_>,
    value: &Expr,
    target: &InstanceOfTarget,
    expr: &Expr,
) -> LoweredValue {
    let mut value_operand = lower_expr(ctx, value);
    let mut operands = vec![value_operand.value];
    // Set to the frame slot when the value operand is rooted across a dynamic
    // target evaluation that can unwind.
    let mut value_root: Option<crate::ir::LocalSlotId> = None;
    // Set to the lowered dynamic-target operand, which this predicate owns and
    // must retire after the op has read it.
    let mut dynamic_target: Option<LoweredValue> = None;
    let immediate = match target {
        InstanceOfTarget::Name(name) => {
            if name.as_str().trim_start_matches('\\') == "static" && ctx.local_slots.contains_key("this") {
                // Apply the same storage-aware retirement to the implicit target
                // as to an explicit local read. Finalization preserves true borrows.
                let target_operand = ctx.load_local("this", Some(expr.span));
                operands.push(target_operand.value);
                dynamic_target = Some(target_operand);
                None
            } else {
                Some(Immediate::Data(ctx.intern_class_name(&instanceof_target_name(ctx, name.as_str()))))
            }
        }
        InstanceOfTarget::Expr(target_expr) => {
            let (rooted, slot) = root_owned_call_operand(ctx, value_operand, expr.span);
            value_operand = rooted;
            operands[0] = rooted.value;
            value_root = slot;
            let target_operand = lower_expr(ctx, target_expr);
            operands.push(target_operand.value);
            dynamic_target = Some(target_operand);
            None
        }
    };
    let op = if immediate.is_some() { Op::InstanceOf } else { Op::InstanceOfDynamic };
    let result = ctx.emit_value(op, operands, immediate, PhpType::Bool, op.default_effects(), Some(expr.span));
    // Retire the dynamic target operand first, then the value operand, so an
    // independently owned temporary on either side is released exactly once.
    if let Some(target_operand) = dynamic_target {
        if target_operand.value != value_operand.value
            && ctx.value_needs_release_after_use(target_operand)
        {
            crate::ir_lower::ownership::release_if_owned(ctx, target_operand, Some(expr.span));
        }
    }
    match value_root {
        Some(slot) => retire_owned_call_operand(ctx, slot, expr.span),
        None => {
            if ctx.value_needs_release_after_use(value_operand) {
                crate::ir_lower::ownership::release_if_owned(ctx, value_operand, Some(expr.span));
            }
        }
    }
    result
}

/// Resolves lexical `instanceof` target keywords to concrete class names when possible.
pub(super) fn instanceof_target_name(ctx: &LoweringContext<'_, '_>, name: &str) -> String {
    match name.trim_start_matches('\\') {
        "self" => ctx.current_class.clone().unwrap_or_else(|| name.to_string()),
        "parent" => ctx
            .current_class
            .as_deref()
            .and_then(|class_name| ctx.classes.get(class_name))
            .and_then(|class_info| class_info.parent.clone())
            .unwrap_or_else(|| name.to_string()),
        _ => name.to_string(),
    }
}

/// Coerces a value to integer storage before integer-only operations.
pub(super) fn coerce_to_int(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, expr: &Expr) -> LoweredValue {
    coerce_to_int_at_span(ctx, value, Some(expr.span))
}

/// Coerces a value to integer storage using an explicit source span.
pub(crate) fn coerce_to_int_at_span(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<crate::span::Span>,
) -> LoweredValue {
    match value.ir_type {
        IrType::I64 => value,
        IrType::F64 => ctx.emit_value(Op::FToI, vec![value.value], None, PhpType::Int, Op::FToI.default_effects(), span),
        IrType::Str => ctx.emit_value(Op::StrToI, vec![value.value], None, PhpType::Int, Op::StrToI.default_effects(), span),
        _ => {
            let result = ctx.emit_value(
                Op::Cast,
                vec![value.value],
                Some(Immediate::CastTarget(IrType::I64)),
                PhpType::Int,
                Op::Cast.default_effects(),
                span,
            );
            // The cast lowers to `__rt_mixed_cast_int`, which returns a raw
            // scalar that never aliases the source box. Dropping the owning
            // reference here leaked one checked-arithmetic Mixed cell per
            // evaluation for `%`, bitops, comparisons, and coerced array
            // indexes with a compound operand (issue #500).
            release_coerced_source_if_owned(ctx, value, span);
            result
        }
    }
}

/// Coerces a value to float when the storage type allows a direct conversion.
pub(super) fn coerce_to_float(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, expr: &Expr) -> LoweredValue {
    coerce_to_float_at_span(ctx, value, Some(expr.span))
}

/// Coerces a value to float storage using an explicit source span.
pub(super) fn coerce_to_float_at_span(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<crate::span::Span>,
) -> LoweredValue {
    match value.ir_type {
        IrType::F64 => value,
        IrType::I64 => ctx.emit_value(Op::IToF, vec![value.value], None, PhpType::Float, Op::IToF.default_effects(), span),
        _ => {
            let result = ctx.emit_value(
                Op::Cast,
                vec![value.value],
                Some(Immediate::CastTarget(IrType::F64)),
                PhpType::Float,
                Op::Cast.default_effects(),
                span,
            );
            // Mirror of the int coercion above: `__rt_mixed_cast_float`
            // returns a raw scalar, so the owning source box (e.g. a checked
            // `pow` operand, issue #500) must be released here.
            release_coerced_source_if_owned(ctx, value, span);
            result
        }
    }
}

/// Coerces a value to string when possible.
pub(super) fn coerce_to_string(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, expr: &Expr) -> LoweredValue {
    coerce_to_string_at_span(ctx, value, Some(expr.span))
}

/// Coerces a value to string storage using an explicit source span.
pub(super) fn coerce_to_string_at_span(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<crate::span::Span>,
) -> LoweredValue {
    if matches!(ctx.builder.value_php_type(value.value), PhpType::Resource(_)) {
        return ctx.emit_value(
            Op::ResourceToStr,
            vec![value.value],
            None,
            PhpType::Str,
            Op::ResourceToStr.default_effects(),
            span,
        );
    }
    match value.ir_type {
        IrType::Str => value,
        IrType::I64 | IrType::TaggedScalar => ctx.emit_value(Op::IToStr, vec![value.value], None, PhpType::Str, Op::IToStr.default_effects(), span),
        IrType::F64 => ctx.emit_value(Op::FToStr, vec![value.value], None, PhpType::Str, Op::FToStr.default_effects(), span),
        _ => {
            let result = ctx.emit_value(
                Op::Cast,
                vec![value.value],
                Some(Immediate::CastTarget(IrType::Str)),
                PhpType::Str,
                Op::Cast.default_effects(),
                span,
            );
            release_coerced_source_if_owned(ctx, value, span);
            result
        }
    }
}
