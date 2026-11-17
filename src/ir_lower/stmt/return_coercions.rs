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
///
/// `source` is the returned expression's span, which the checker records when its checked
/// type can hold a value a declared scalar return refuses.
pub(super) fn coerce_to_return_type(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<Span>,
    source: Option<Span>,
) -> LoweredValue {
    let return_php_type = ctx.return_php_type.clone();
    let value = ctx.retype_packed_address_for_pointer_sink(value, &return_php_type, span);
    crate::ir_lower::object_boundaries::guard_return(ctx, value, span);
    guard_declared_callable_return_value(ctx, value, span.unwrap_or_else(Span::dummy));
    let value = match guard_declared_scalar_return(ctx, value, span, source) {
        ScalarReturnGuard::Converted(converted) => return converted,
        ScalarReturnGuard::Checked(value) => value,
    };
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

/// What the declared scalar return check left for the rest of the return coercion.
enum ScalarReturnGuard {
    /// The `float` the check already converted, in the return storage.
    Converted(LoweredValue),
    /// The (possibly checked) value, still to be coerced as before.
    Checked(LoweredValue),
}

/// Checks a boxed `mixed`/union value returned from a DECLARED `string`, `float` or `bool`
/// return type, throwing PHP's `f(): Return value must be of type T, U returned`.
///
/// `int` keeps its dedicated `Op::ReturnBoundaryMixedToInt`; nullable, union, `mixed`,
/// `void`, `never`, class and generator returns are untouched, and inferred (undeclared)
/// returns never throw. Only a returned expression the checker recorded is checked.
fn guard_declared_scalar_return(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<Span>,
    source: Option<Span>,
) -> ScalarReturnGuard {
    let declared = ctx.return_php_type.clone();
    if !ctx.return_type_is_declared
        || !matches!(declared, PhpType::Str | PhpType::Float | PhpType::Bool)
        || !matches!(value.ir_type, IrType::Heap(IrHeapKind::Mixed | IrHeapKind::Union))
    {
        return ScalarReturnGuard::Checked(value);
    }
    // The checker records the returned expression when its checked type can hold a refused
    // value; a local whose storage widened to a box while its type stayed scalar (an
    // overflow-promoted accumulator, for example) is not checked.
    if !source.is_some_and(|source| ctx.scalar_guard_arg_sites.contains(&source)) {
        // A boxed scalar union still binds to `bool` by truthiness, not as an integer.
        if declared == PhpType::Bool {
            return ScalarReturnGuard::Converted(crate::ir_lower::expr::lower_truthy_bool(ctx, value, span));
        }
        return ScalarReturnGuard::Checked(value);
    }
    // Named by its PHP label: `{closure:file:line}` for a closure, the PHP name of an
    // include-variant function, `class@anonymous::m` for an anonymous class's method.
    let prefix = format!(
        "{}(): Return value must be of type {}, ",
        crate::ir_lower::object_boundaries::callable_label(ctx),
        crate::types::diagnostic_names::php_type_name(&declared),
    );
    let span = span.unwrap_or_else(Span::dummy);
    match crate::ir_lower::expr::emit_scalar_guard(
        ctx, value, &declared, &prefix, " returned", span,
        crate::ir_lower::expr::ScalarGuardEmit { release_on_refusal: false, also_release: Vec::new(), diagnostic_span: span },
    ) {
        Some(converted) => ScalarReturnGuard::Converted(converted),
        // A checked scalar bound to `bool` converts by PHP truthiness (`"abc"` is true); the
        // generic integer return coercion would read it as `(int)"abc"`, which is false.
        None if declared == PhpType::Bool => {
            ScalarReturnGuard::Converted(crate::ir_lower::expr::lower_truthy_bool(ctx, value, Some(span)))
        }
        None => ScalarReturnGuard::Checked(value),
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
    if !crate::types::declared_int_return_requires_validation(
        ctx.return_type_is_declared, &ctx.return_php_type,
        &ctx.builder.value_php_type(value.value),
    ) {
        return None;
    }
    let prefix = format!(
        "{}(): Return value must be of type int, ",
        crate::ir_lower::object_boundaries::callable_label(ctx),
    );
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

/// Runs PHP's return check for a declared `callable` return type (`callable`, `?callable`, ...)
/// on the returned value, which keeps its PHP shape; any other return type is left alone.
///
/// A by-reference return (`function &f(): callable`) checks the value read through the cell it
/// hands the caller, so both return paths throw the same `TypeError`.
pub(super) fn guard_declared_callable_return_value(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
) {
    let return_php_type = ctx.return_php_type.clone();
    if !ctx.return_type_is_declared || !return_php_type.declares_callable() {
        return;
    }
    let display = ctx
        .builder
        .function()
        .display_name
        .clone()
        .unwrap_or_else(|| ctx.owner_name().to_string());
    crate::ir_lower::expr::guard_declared_callable_return(
        ctx,
        &display,
        value,
        &return_php_type,
        span,
    );
}
