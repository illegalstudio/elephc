//! Purpose:
//! Function return storage coercion and container widening.
//!
//! Called from:
//! - `crate::ir_lower::stmt`.
//!
//! Key details:
//! - Preserves statement ordering, CFG shape, EIR effects, and ownership contracts.
//! - Owned Mixed-to-object returns retain their payload before retiring the source cell.
//! - Owned Mixed-to-container returns release the cell once the runtime unbox holds its own
//!   reference; mixed-slot contracts convert that owned payload, never the cell's borrow.

use super::*;
use crate::ir::IrHeapKind;

/// Coerces a value to the current function return storage type when needed.
pub(super) fn coerce_to_return_type(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<Span>,
) -> LoweredValue {
    let return_php_type = ctx.return_php_type.clone();
    let value = ctx.retype_packed_address_for_pointer_sink(value, &return_php_type, span);
    if let Some(value) = coerce_container_to_return_type(ctx, value, span) {
        return value;
    }
    if value.ir_type == ctx.return_type {
        return value;
    }
    // Lazy merges preserve a typed array miss in a nullable box. Object/callable
    // returns use their ordinary payload ABI; take an independent payload lease
    // before releasing the owning merge box (including its null arm). Nominal class
    // assignability is checked by the frontend; parent/interface returns share the
    // same object payload ABI as the concrete child carried by the merge.
    let return_ty = ctx.return_php_type.codegen_repr();
    let source_ty = ctx.builder.value_php_type(value.value);
    if matches!(return_ty, PhpType::Object(_) | PhpType::Callable)
        && value.ir_type == IrType::Heap(IrHeapKind::Mixed)
        && matches!(source_ty, PhpType::Union(members) if members.iter().all(|member|
            matches!(member, PhpType::Void | PhpType::Never)
                || matches!((&return_ty, member),
                    (PhpType::Object(_), PhpType::Object(_)) | (PhpType::Callable, PhpType::Callable))))
    {
        let unboxed = ctx.emit_owned_value(
            Op::MixedUnbox,
            vec![value.value],
            None,
            return_ty.clone(),
            Op::mixed_unbox_effects(&return_ty),
            span,
        );
        if ctx.value_is_owning_temporary(value) {
            crate::ir_lower::ownership::release_if_owned(ctx, value, span);
        }
        return unboxed;
    }
    match ctx.return_type {
        IrType::I64 => {
            if let Some(verified) = declared_int_return_boundary(ctx, value, span) {
                return verified;
            }
            coerce_return_scalar_source(ctx, value, span, coerce_to_int)
        }
        IrType::F64 => coerce_return_scalar_source(ctx, value, span, coerce_to_float),
        IrType::Str => coerce_return_scalar_source(ctx, value, span, coerce_to_string),
        IrType::TaggedScalar => {
            coerce_return_scalar_source(ctx, value, span, coerce_to_tagged_scalar)
        }
        IrType::Heap(_) if ctx.return_php_type.codegen_repr() == PhpType::Mixed => {
            ctx.box_value_as_mixed(value, ctx.return_php_type.clone(), span)
        }
        IrType::Heap(_) if matches!(ctx.return_php_type.codegen_repr(), PhpType::Object(_))
            && matches!(ctx.builder.value_php_type(value.value).codegen_repr(),
                PhpType::Mixed | PhpType::Union(_)) =>
        {
            // Assignment expressions can hand this boundary an independent owned cell.
            // Retain the returned object before retiring that cell, not just the frame local.
            let result = ctx.emit_owned_value(Op::MixedUnbox, vec![value.value], None,
                ctx.return_php_type.clone(), Op::mixed_unbox_effects(&ctx.return_php_type), span);
            if ctx.value_is_owning_temporary(value) {
                crate::ir_lower::ownership::release_if_owned(ctx, value, span);
            }
            result
        }
        IrType::Heap(_) => coerce_heap_return_source(ctx, value, span),
        IrType::Void => value,
    }
}

/// Coerces a heap value to the declared heap return storage through the runtime boundary.
///
/// A boxed source is unboxed with a reference of its own (`lower_runtime_call`), so an owning
/// cell, such as a `match` or ternary merged into a `mixed` temp, has no reader left afterwards
/// and is released here. Returning without that release leaked the cell and the container it
/// held. An `array<mixed>` or `array<K, mixed>` contract unboxes the owned payload, releases the
/// cell and only then converts the slots: the conversion consumes the reference it is handed and
/// splits a shared payload, so converting the cell's own borrow would take the cell's reference
/// away from a payload the caller may still share.
fn coerce_heap_return_source(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<Span>,
) -> LoweredValue {
    let return_ty = ctx.return_php_type.clone();
    let cell_is_owning = value.ir_type == IrType::Heap(IrHeapKind::Mixed)
        && matches!(
            ctx.builder.value_php_type(value.value).codegen_repr(),
            PhpType::Mixed | PhpType::Union(_)
        )
        && ctx.value_is_owning_temporary(value);
    if cell_is_owning {
        let mixed_slots = match return_ty.codegen_repr() {
            PhpType::Array(elem) if elem.codegen_repr() == PhpType::Mixed => {
                Some((PhpType::Array(Box::new(PhpType::Never)), Op::ArrayToMixed))
            }
            PhpType::AssocArray { key, value: slot } if slot.codegen_repr() == PhpType::Mixed => Some((
                PhpType::AssocArray { key, value: Box::new(PhpType::Never) },
                Op::HashToMixed,
            )),
            _ => None,
        };
        if let Some((payload_ty, op)) = mixed_slots {
            let payload = ctx.emit_value(
                Op::RuntimeCall,
                vec![value.value],
                None,
                payload_ty,
                effects_lookup::runtime_effects(),
                span,
            );
            crate::ir_lower::ownership::release_if_owned(ctx, value, span);
            return ctx.emit_value(op, vec![payload.value], None, return_ty, op.default_effects(), span);
        }
    }
    let converted = ctx.emit_value(
        Op::RuntimeCall,
        vec![value.value],
        None,
        return_ty,
        effects_lookup::runtime_effects(),
        span,
    );
    if cell_is_owning {
        crate::ir_lower::ownership::release_if_owned(ctx, value, span);
    }
    converted
}

/// Verifies a dynamically-typed value against a DECLARED `int` return boundary.
///
/// A declared boundary carries PHP's coercive-mode verification: an in-range float
/// truncates, but a float outside the int range, a non-numeric string, null, or any
/// container throws `TypeError` — where the plain `coerce_to_int` path silently wrapped
/// the value. Applies only when the source type still needs a runtime decision (boxed
/// Mixed, or a raw F64 left by constant folding); statically-int sources keep the direct
/// path, and INFERRED int returns are untouched — an undeclared function must never throw
/// on its own return value.
fn declared_int_return_boundary(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<Span>,
) -> Option<LoweredValue> {
    if !ctx.return_type_is_declared || ctx.return_php_type != PhpType::Int {
        return None;
    }
    if !matches!(
        ctx.builder.value_php_type(value.value).codegen_repr(),
        PhpType::Mixed | PhpType::Float
    ) {
        return None;
    }
    let prefix = format!("{}(): Return value must be of type int, ", ctx.owner_name());
    let data = ctx.intern_string(&prefix);
    let verified = ctx.emit_value(
        Op::ReturnBoundaryMixedToInt,
        vec![value.value],
        Some(Immediate::Data(data)),
        PhpType::Int,
        Op::ReturnBoundaryMixedToInt.default_effects(),
        span,
    );
    if ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, span);
    }
    Some(verified)
}

/// Coerces a return value and releases the old owning temporary when replaced.
pub(super) fn coerce_return_scalar_source(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<Span>,
    coerce: fn(&mut LoweringContext<'_, '_>, LoweredValue, Option<Span>) -> LoweredValue,
) -> LoweredValue {
    let coerced = coerce(ctx, value, span);
    if coerced.value != value.value && ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, span);
    }
    coerced
}

/// Coerces an integer-or-null value into the two-word tagged-scalar return shape.
pub(super) fn coerce_to_tagged_scalar(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<Span>,
) -> LoweredValue {
    if value.ir_type == IrType::TaggedScalar {
        return value;
    }
    if matches!(
        ctx.builder.value_php_type(value.value).codegen_repr(),
        PhpType::Void
    ) {
        return ctx.emit_value(
            Op::ConstNull,
            Vec::new(),
            None,
            PhpType::TaggedScalar,
            Op::ConstNull.default_effects(),
            span,
        );
    }
    ctx.emit_value(
        Op::RuntimeCall,
        vec![value.value],
        None,
        PhpType::TaggedScalar,
        effects_lookup::runtime_effects(),
        span,
    )
}

/// Widens returned container payload storage to the current function return contract.
pub(super) fn coerce_container_to_return_type(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<Span>,
) -> Option<LoweredValue> {
    let source_ty = ctx.builder.value_php_type(value.value).codegen_repr();
    let return_ty = ctx.return_php_type.codegen_repr();
    let op = match (source_ty, return_ty.clone()) {
        (PhpType::Array(source_elem), PhpType::Array(return_elem))
            if source_elem.codegen_repr() != PhpType::Mixed
                && return_elem.codegen_repr() == PhpType::Mixed =>
        {
            Op::ArrayToMixed
        }
        (
            PhpType::AssocArray {
                value: source_value,
                ..
            },
            PhpType::AssocArray {
                value: return_value,
                ..
            },
        ) if source_value.codegen_repr() != PhpType::Mixed
            && return_value.codegen_repr() == PhpType::Mixed =>
        {
            Op::HashToMixed
        }
        (PhpType::Array(source_elem), PhpType::AssocArray { .. })
            if source_elem.as_ref() == &PhpType::Never =>
        {
            Op::ArrayToHash
        }
        _ => return None,
    };
    Some(ctx.emit_value(
        op,
        vec![value.value],
        None,
        return_ty,
        op.default_effects(),
        span,
    ))
}

/// Coerces a value to integer storage.
pub(super) fn coerce_to_int(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<Span>,
) -> LoweredValue {
    match value.ir_type {
        IrType::I64 => value,
        IrType::F64 => ctx.emit_value(
            Op::FToI,
            vec![value.value],
            None,
            PhpType::Int,
            Op::FToI.default_effects(),
            span,
        ),
        IrType::Str => ctx.emit_value(
            Op::StrToI,
            vec![value.value],
            None,
            PhpType::Int,
            Op::StrToI.default_effects(),
            span,
        ),
        _ => ctx.emit_value(
            Op::Cast,
            vec![value.value],
            Some(Immediate::CastTarget(IrType::I64)),
            PhpType::Int,
            Op::Cast.default_effects(),
            span,
        ),
    }
}

/// Coerces a value to float storage.
pub(super) fn coerce_to_float(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<Span>,
) -> LoweredValue {
    match value.ir_type {
        IrType::F64 => value,
        IrType::I64 => ctx.emit_value(
            Op::IToF,
            vec![value.value],
            None,
            PhpType::Float,
            Op::IToF.default_effects(),
            span,
        ),
        IrType::Str => ctx.emit_value(
            Op::StrToF,
            vec![value.value],
            None,
            PhpType::Float,
            Op::StrToF.default_effects(),
            span,
        ),
        _ => ctx.emit_value(
            Op::Cast,
            vec![value.value],
            Some(Immediate::CastTarget(IrType::F64)),
            PhpType::Float,
            Op::Cast.default_effects(),
            span,
        ),
    }
}

/// Coerces a value to string storage.
pub(super) fn coerce_to_string(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<Span>,
) -> LoweredValue {
    match value.ir_type {
        IrType::Str => value,
        IrType::I64 | IrType::TaggedScalar => ctx.emit_value(
            Op::IToStr,
            vec![value.value],
            None,
            PhpType::Str,
            Op::IToStr.default_effects(),
            span,
        ),
        IrType::F64 => ctx.emit_value(
            Op::FToStr,
            vec![value.value],
            None,
            PhpType::Str,
            Op::FToStr.default_effects(),
            span,
        ),
        _ => ctx.emit_value(
            Op::Cast,
            vec![value.value],
            Some(Immediate::CastTarget(IrType::Str)),
            PhpType::Str,
            Op::Cast.default_effects(),
            span,
        ),
    }
}

