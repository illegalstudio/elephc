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
    let value = guard_generic_object_nominal_return(ctx, value, &return_php_type, span);
    let value = guard_gradual_union_return(ctx, value, &return_php_type, span);
    let source_declared_type = ctx.builder.value_php_type(value.value).clone();
    // A declared `int` return fed by a gradual value (a checked sum that may have become a float)
    // is verified by `declared_int_return_boundary` below, which throws php's TypeError for a
    // float out of range; a plain scalar cast here would wrap it silently.
    let int_boundary_owns_it = ctx.return_type_is_declared
        && return_php_type == PhpType::Int
        && matches!(
            source_declared_type.codegen_repr(),
            PhpType::Mixed | PhpType::Float | PhpType::TaggedScalar
        );
    let scalar_cast = crate::types::param_binding::scalar_param_cast(
        &return_php_type,
        &source_declared_type,
    )
    .filter(|_| !int_boundary_owns_it);
    let value = match scalar_cast {
        Some(cast) => crate::ir_lower::expr::apply_scalar_param_cast(
            ctx,
            cast,
            value,
            &return_php_type,
            span,
        ),
        None => value,
    };
    let value = if int_boundary_owns_it {
        value
    } else {
        crate::ir_lower::gradual_coercions::coerce_gradual_value_to_boundary(
            ctx,
            value,
            &return_php_type,
            span,
        )
    };
    if matches!(return_php_type.codegen_repr(), PhpType::Callable)
        && matches!(
            ctx.builder.value_php_type(value.value).codegen_repr(),
            PhpType::Str | PhpType::Array(_)
        )
    {
        return coerce_to_callable_return(ctx, value, span);
    }
    if let Some(value) = coerce_container_to_return_type(ctx, value, span) {
        return value;
    }
    if let Some(value) = relabel_concrete_container_as_iterable(ctx, value, span) {
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

/// Resolves a runtime string or receiver/method pair to callable descriptor storage.
fn coerce_to_callable_return(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<Span>,
) -> LoweredValue {
    let coerced = ctx.emit_value(
        Op::RuntimeCall,
        vec![value.value],
        None,
        PhpType::Callable,
        effects_lookup::runtime_effects(),
        span,
    );
    if ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, span);
    }
    coerced
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
    let source_repr = ctx.builder.value_php_type(value.value).codegen_repr();
    if !matches!(
        source_repr,
        PhpType::Mixed | PhpType::Float | PhpType::TaggedScalar
    ) {
        return None;
    }
    // A compact nullable scalar (`?int`) is not a boxed Mixed, and the boundary op unboxes what
    // it is given. Boxing first hands it the representation it reads, and null then reaches the
    // null arm and throws the same TypeError PHP does. Symfony's `KernelEvent` stores
    // `private ?int $requestType` and returns it from `getRequestType(): int`, which reference
    // PHP accepts and raises on only when the value really is null.
    let value = if matches!(source_repr, PhpType::TaggedScalar) {
        // The result is a real heap box, so it is typed `mixed`: typed as the compact `int|null`
        // it came from, it was not an owning temporary and the release below never ran, leaking
        // one box per `return $nullableInt;` from a declared `: int` function.
        ctx.box_value_as_mixed(value, PhpType::Mixed, span)
    } else {
        value
    };
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

/// Checks a boxed value returned through a declared scalar/array union at run time.
///
/// A declared `array` is the packed-or-hash union, stored boxed, so a `mixed` value crossing it
/// needs no conversion -- and then nothing checked it: `function relay(mixed $v): array { return
/// $v; }` handed a string back where php raises "relay(): Return value must be of type array,
/// string returned". The parameter guard does the tag check; the data it carries is the full
/// message prefix, which tells the backend to word the error as a return.
fn guard_gradual_union_return(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    return_type: &PhpType,
    span: Option<Span>,
) -> LoweredValue {
    if !ctx.return_type_is_declared {
        return value;
    }
    let source_type = ctx.builder.value_php_type(value.value).clone();
    if !crate::types::param_binding::gradual_union_requires_runtime_param_guard(
        return_type,
        &source_type,
    ) {
        return value;
    }
    let prefix = format!(
        "{}(): Return value must be of type {}, ",
        ctx.owner_name(),
        return_type
    );
    let name = ctx.intern_string(&prefix);
    // Only the check matters: the guard hands back the same box, and returning the original
    // value keeps its ownership, which the backend's return path requires to be unambiguous.
    ctx.emit_value(
        Op::RuntimeCall,
        vec![value.value],
        Some(Immediate::ParameterType {
            name,
            strict_types: ctx.php_strict_types,
        }),
        return_type.clone(),
        crate::ir_lower::effects_lookup::runtime_effects(),
        span,
    );
    value
}

/// Inserts the runtime class guard required when an object crosses a narrower nominal boundary.
fn guard_generic_object_nominal_return(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    return_type: &PhpType,
    span: Option<Span>,
) -> LoweredValue {
    let source_type = ctx.builder.value_php_type(value.value).codegen_repr();
    let needs_object_guard = matches!(source_type, PhpType::Object(_))
        || crate::types::param_binding::gradual_object_requires_runtime_nominal_guard(
            return_type,
            &source_type,
        );
    if !needs_object_guard {
        return value;
    }
    let Some(target_name) =
        crate::types::param_binding::nominal_object_boundary_target(return_type)
    else {
        return value;
    };
    // A generic `object` source (`Object("")`) proves no class: `function f(object $v): ?C
    // { return $v; }` handed any object back unchecked where php throws the return TypeError.
    let generic_object_source = matches!(&source_type, PhpType::Object(name)
        if name.is_empty() || name.eq_ignore_ascii_case("object"));
    if matches!(source_type, PhpType::Object(_))
        && !generic_object_source
        && crate::ir_lower::expr::param_accepts_object_without_string_coercion(
        ctx,
        return_type,
        &source_type,
    ) {
        return value;
    }
    let data = ctx.intern_class_name(&target_name);
    let guarded = ctx.emit_value(
        Op::RuntimeCall,
        vec![value.value],
        Some(Immediate::NominalObject {
            target: data,
            boundary: crate::ir::NominalObjectBoundary::Return,
        }),
        return_type.clone(),
        effects_lookup::runtime_effects(),
        span,
    );
    // A boxed source is unboxed into an object the guard takes its own reference on, so an owning
    // box is done with: `function f(mixed $m): Iterator { return $m->make(); }` leaked the box and
    // the object it still held, and the iterator's destructor never ran.
    // An object source is the same story: the guard materializes its own owner (`__rt_incref` in
    // `store_nominal_object_result`), so an owned call result -- `return parent::add($i);` in an
    // override narrowing `DateTime` to `static` -- leaked one object per call.
    if matches!(
        source_type,
        PhpType::Mixed | PhpType::Union(_) | PhpType::Object(_)
    ) && matches!(return_type.codegen_repr(), PhpType::Object(_))
        && ctx.value_is_owning_temporary(value)
    {
        crate::ir_lower::ownership::release_if_owned(ctx, value, span);
    }
    guarded
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
///
/// The conversions (`HashToMixed`, `ArrayToMixed`, `ArrayToHash`) rewrite a container they hold
/// the only reference to IN PLACE. That is right for a frame local the return consumes and wrong
/// for a container its owner keeps after the return -- a function `static`, a static or object
/// property: `static $h = ['a' => 0]; ...; return $h;` under a wider return type turned the
/// static's own `int` slots into boxes while the slot still read them as ints. Such a source is
/// acquired first, so the conversion copies and the owner keeps its container.
pub(super) fn coerce_container_to_return_type(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<Span>,
) -> Option<LoweredValue> {
    if !matches!(
        ctx.builder.value_defining_op(value.value),
        Some(
            Op::LoadStaticLocal
                | Op::LoadStaticProperty
                | Op::LoadDynamicStaticProperty
                | Op::PropGet
                | Op::DynamicPropGet
                | Op::NullsafePropGet
        )
    ) {
        return coerce_owned_container_to_return_type(ctx, value, span);
    }
    let source_ty = ctx.builder.value_php_type(value.value).codegen_repr();
    let return_ty = ctx.return_php_type.codegen_repr();
    if !container_return_conversion_applies(&source_ty, &return_ty) {
        return None;
    }
    let acquired = crate::ir_lower::ownership::acquire_if_refcounted(ctx, value, span);
    coerce_owned_container_to_return_type(ctx, acquired, span)
}

/// Relabels a concrete array or object as an `iterable` return without converting it.
///
/// An `iterable` result shares the container's own representation, so the value is the same
/// pointer under a different static type. Going through the generic runtime conversion instead
/// declared the result a fresh owner while handing back the source's storage: `return $n;` of a
/// frame-local array then skipped the return retain, the epilogue released the local, and the
/// caller received a freed array (a `mixed` receiver boxed it as null). The relabel is an `Op::Move` of an owner:
/// the return acquires a borrowed source first, and an owning temporary simply moves on.
fn relabel_concrete_container_as_iterable(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<Span>,
) -> Option<LoweredValue> {
    if ctx.return_php_type.codegen_repr() != PhpType::Iterable {
        return None;
    }
    let source_ty = ctx.builder.value_php_type(value.value).codegen_repr();
    if !matches!(
        source_ty,
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_)
    ) {
        return None;
    }
    // A borrowed source (a frame local or a property) keeps its own reference for the frame
    // epilogue, so the returned value takes one of its own; an owning temporary just moves on.
    let owner = if ctx.value_is_owning_temporary(value) {
        value
    } else {
        crate::ir_lower::ownership::acquire_if_refcounted(ctx, value, span)
    };
    Some(ctx.emit_value(
        Op::Move,
        vec![owner.value],
        None,
        PhpType::Iterable,
        Op::Move.default_effects(),
        span,
    ))
}

/// Whether `coerce_owned_container_to_return_type` converts a `source` container for `ret`.
fn container_return_conversion_applies(source: &PhpType, ret: &PhpType) -> bool {
    match (source, ret) {
        // Any hash meets a bare `array` contract by relabelling (`Op::Move`), mixed-valued ones
        // included. Leaving those out sent `return $this->headers;` (a mixed-valued hash
        // property, declared `: array`) to the generic runtime conversion, which handed back the
        // property's own storage as an owned result: the caller's release freed the property,
        // and the next `ResponseHeaderBag::all()` read freed memory (Laravel's sendHeaders()).
        (PhpType::AssocArray { .. }, PhpType::Array(element)) => {
            element.codegen_repr() == PhpType::Mixed
        }
        // Boxed then hashed, or hashed as it is (`ArrayToHash`).
        (PhpType::Array(element), PhpType::AssocArray { value, .. }) => {
            element.as_ref() == &PhpType::Never
                || element.codegen_repr() == value.codegen_repr()
                || value.codegen_repr() == PhpType::Mixed
        }
        (PhpType::Array(source_elem), PhpType::Array(return_elem)) => {
            source_elem.codegen_repr() != PhpType::Mixed
                && return_elem.codegen_repr() == PhpType::Mixed
        }
        (PhpType::AssocArray { value: source_value, .. }, PhpType::AssocArray { value: return_value, .. }) => {
            source_value.codegen_repr() != PhpType::Mixed
                && return_value.codegen_repr() == PhpType::Mixed
        }
        _ => false,
    }
}

/// [`coerce_container_to_return_type`] for a container this return may consume.
fn coerce_owned_container_to_return_type(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<Span>,
) -> Option<LoweredValue> {
    let source_ty = ctx.builder.value_php_type(value.value).codegen_repr();
    let return_ty = ctx.return_php_type.codegen_repr();
    if let (
        PhpType::AssocArray {
            key,
            value: source_value,
        },
        PhpType::Array(return_element),
    ) = (&source_ty, &return_ty)
    {
        if return_element.codegen_repr() == PhpType::Mixed {
            // PHP's bare `array` return contract accepts both indexed and associative layouts.
            // Widen concrete hash slots first, then only retype the same runtime container as
            // the generic array contract; converting the hash itself to indexed storage would
            // discard string keys and change iteration order.
            let source = if source_value.codegen_repr() == PhpType::Mixed {
                value
            } else {
                ctx.emit_value(
                    Op::HashToMixed,
                    vec![value.value],
                    None,
                    PhpType::AssocArray {
                        key: key.clone(),
                        value: Box::new(PhpType::Mixed),
                    },
                    Op::HashToMixed.default_effects(),
                    span,
                )
            };
            return Some(ctx.emit_value(
                Op::Move,
                vec![source.value],
                None,
                return_ty,
                Op::Move.default_effects(),
                span,
            ));
        }
    }
    // A bare `array` contract resolves to the HASH whenever some return path yields one
    // (`Checker::generic_array_return_contract`), so an INDEXED return on another path must be
    // CONVERTED here rather than restamped. `Op::ArrayToHash` carries the element type across
    // unchanged, so a packed vector of raw slots is boxed first when the contract stores Mixed.
    if let (
        PhpType::Array(source_elem),
        PhpType::AssocArray {
            value: return_value,
            ..
        },
    ) = (&source_ty, &return_ty)
    {
        let source_elem = source_elem.codegen_repr();
        if source_elem != PhpType::Never
            && source_elem != return_value.codegen_repr()
            && return_value.codegen_repr() == PhpType::Mixed
        {
            let boxed = ctx.emit_value(
                Op::ArrayToMixed,
                vec![value.value],
                None,
                PhpType::Array(Box::new(PhpType::Mixed)),
                Op::ArrayToMixed.default_effects(),
                span,
            );
            return Some(ctx.emit_value(
                Op::ArrayToHash,
                vec![boxed.value],
                None,
                return_ty,
                Op::ArrayToHash.default_effects(),
                span,
            ));
        }
    }
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
        (
            PhpType::Array(source_elem),
            PhpType::AssocArray {
                value: return_value,
                ..
            },
        ) if source_elem.as_ref() == &PhpType::Never
            || source_elem.codegen_repr() == return_value.codegen_repr() =>
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
    // Mixed uses one integer-sized boxed-cell register regardless of the PHP payload. Preserve
    // the semantic representation at a declared string return boundary instead of rendering the
    // cell pointer as an integer string.
    if matches!(
        ctx.builder.value_php_type(value.value).codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        return ctx.emit_value(
            Op::Cast,
            vec![value.value],
            Some(Immediate::CastTarget(IrType::Str)),
            PhpType::Str,
            Op::Cast.default_effects(),
            span,
        );
    }
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
