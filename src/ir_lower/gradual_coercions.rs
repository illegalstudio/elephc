//! Purpose:
//! Narrows boxed gradual values at concrete PHP call and return boundaries.
//!
//! Called from:
//! - `crate::ir_lower::expr` and `crate::ir_lower::stmt`.
//!
//! Key details:
//! - Runtime checks precede every representation change, and replacement temporaries own their
//!   retained or converted payload independently of the source boxed cell.

use crate::ir::{Immediate, IrType, Op};
use crate::ir_lower::context::{LoweredValue, LoweringContext};
use crate::ir_lower::ownership;
use crate::span::Span;
use crate::types::PhpType;

/// Reports whether [`coerce_gradual_value_to_boundary`] will actually convert a boxed gradual
/// value into `target`, guard included.
///
/// The checker consults this before accepting a gradual value at a declared-type boundary. Both
/// sides must agree by construction: accepting a target this returns `false` for would let an
/// unconverted `Mixed` reach a concrete slot — a silent miscompile in place of a loud refusal.
/// Keep the arms below in lockstep with the `match` in `coerce_gradual_value_to_boundary`.
/// ⚠️ Deliberately narrower than the `match`: only the ARRAY targets are listed, because only
/// they lower to `MixedToHash`, which tests the runtime tag and raises PHP's `TypeError` for a
/// non-array. The scalar arms lower to `Cast`, which CONVERTS — PHP throws when a non-numeric
/// string reaches a declared `int`, it does not silently yield `0`. Widening this to the scalar
/// targets would trade a loud refusal for the silent wrong value.
pub(crate) fn boundary_coercion_narrows_gradual(target: &PhpType) -> bool {
    match target.codegen_repr() {
        PhpType::Array(element) => element.codegen_repr() == PhpType::Mixed,
        PhpType::AssocArray { key, value } => {
            matches!(key.codegen_repr(), PhpType::Mixed | PhpType::Str)
                && value.codegen_repr() == PhpType::Mixed
        }
        _ => false,
    }
}

/// Effects of a scalar boundary cast: its conversion warnings (NaN to bool, lossy float to int)
/// reach the user error handler, and a handler that throws makes the cast a throw point.
/// The call no longer converts this argument itself, so the call-site coercion model in
/// `effect_refinement` cannot supply the exceptional edge.
fn boundary_cast_effects() -> crate::ir::Effects {
    Op::Cast.default_effects() | crate::ir::Effects::MAY_THROW
}

/// Retypes the native result of a call to the type the method's own body PROVES it has
/// (`Checker::doc_return_sites`): the class of `return new $c(...);` called with a `Foo::class`
/// argument, or the scalar type of `return $p;` when the call cannot be inlined.
///
/// Nothing is checked or converted, because nothing can differ: the value is an instance of
/// exactly that class whatever the annotation says. The native result is a boxed `mixed`; the payload
/// is taken out of the box.
pub(super) fn narrow_doc_call_result(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    expr: &crate::parser::ast::Expr,
) -> LoweredValue {
    if ctx.doc_return_sites.is_empty() || ctx.builder.insertion_block_is_terminated() {
        return value;
    }
    let Some(target) = ctx
        .doc_return_sites
        .get(&(ctx.loop_storage_scope.clone(), expr.span))
        .cloned()
    else {
        return value;
    };
    let span = Some(expr.span);
    let source = ctx.builder.value_php_type(value.value).codegen_repr();
    if source == target.codegen_repr() || matches!(source, PhpType::Void | PhpType::Never) {
        return value;
    }
    let boxed = if source == PhpType::Mixed {
        value
    } else {
        ctx.box_value_as_mixed(value, PhpType::Mixed, span)
    };
    if !matches!(target, PhpType::Object(_)) {
        // A proven identity result: the box holds exactly this scalar, so the read-back converts
        // nothing.
        let narrowed = coerce_gradual_value_to_boundary(ctx, boxed, &target, span);
        return narrowed;
    }
    let unboxed = ctx.emit_owned_value(
        Op::MixedUnbox,
        vec![boxed.value],
        None,
        target.clone(),
        Op::mixed_unbox_effects(&target),
        span,
    );
    if ctx.value_is_owning_temporary(boxed) {
        ownership::release_if_owned(ctx, boxed, span);
    }
    unboxed
}

/// Narrows a boxed gradual value into a concrete boundary representation when supported.
pub(super) fn coerce_gradual_value_to_boundary(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    target: &PhpType,
    span: Option<Span>,
) -> LoweredValue {
    let source = ctx.builder.value_php_type(value.value).codegen_repr();
    if !matches!(source, PhpType::Mixed) {
        return value;
    }
    let target = target.codegen_repr();
    let narrowed = match &target {
        PhpType::Int => ctx.emit_value(
            Op::Cast,
            vec![value.value],
            Some(Immediate::CastTarget(IrType::I64)),
            target.clone(),
            boundary_cast_effects(),
            span,
        ),
        // A pointer word crosses a boxed local as a plain integer (`null` is address 0), so a
        // `$p = null` local whose slot was widened to a Mixed box reads back through the integer
        // cast. Passing the box itself made the callee test the box address against null.
        PhpType::Pointer(_) => ctx.emit_value(
            Op::Cast,
            vec![value.value],
            Some(Immediate::CastTarget(IrType::I64)),
            target.clone(),
            boundary_cast_effects(),
            span,
        ),
        PhpType::Float => ctx.emit_value(
            Op::Cast,
            vec![value.value],
            Some(Immediate::CastTarget(IrType::F64)),
            target.clone(),
            boundary_cast_effects(),
            span,
        ),
        PhpType::Bool => ctx.emit_value(
            Op::Cast,
            vec![value.value],
            Some(Immediate::CastTarget(IrType::I64)),
            target.clone(),
            boundary_cast_effects(),
            span,
        ),
        PhpType::Str => ctx.emit_value(
            Op::Cast,
            vec![value.value],
            Some(Immediate::CastTarget(IrType::Str)),
            target.clone(),
            boundary_cast_effects(),
            span,
        ),
        PhpType::Object(class_name) => {
            let data = ctx.intern_class_name(class_name);
            ctx.emit_value(
                Op::MixedUnbox,
                vec![value.value],
                Some(Immediate::Data(data)),
                target.clone(),
                Op::MixedUnbox.default_effects(),
                span,
            )
        }
        PhpType::Callable => ctx.emit_value(
            Op::NormalizeCallable,
            vec![value.value],
            None,
            target.clone(),
            Op::NormalizeCallable.default_effects(),
            span,
        ),
        PhpType::Iterable => ctx.emit_value(
            Op::MixedUnbox,
            vec![value.value],
            None,
            target.clone(),
            Op::MixedUnbox.default_effects(),
            span,
        ),
        // A LIST target keeps the source's container kind. Asking for the hash here made an
        // indexed source associative, and the bare `array` return contract then restamped that
        // hash as the declared indexed type without converting it — `in_array()`,
        // `array_filter()`, `array_pop()` and `min()` segfaulted on the result while
        // `count()`, `foreach` and `json_encode()` read it correctly, because only some
        // consumers ask `__rt_heap_kind` first. An associative source still becomes a hash:
        // it cannot become a list without discarding its keys.
        PhpType::Array(element) if element.codegen_repr() == PhpType::Mixed => {
            ctx.emit_value(
                Op::MixedToHash,
                vec![value.value],
                None,
                PhpType::Array(Box::new(PhpType::Mixed)),
                Op::MixedToHash.default_effects(),
                span,
            )
        }
        // `MixedToHash` checks the runtime tag (indexed OR associative) and hands back hash
        // storage that carries whatever keys the source array had, so the declared KEY type is a
        // static label over one representation: a `Str`-keyed declaration is satisfied by the same
        // conversion as a `Mixed`-keyed one. The VALUE must stay `Mixed`, because narrowing the
        // elements would need a per-element conversion this boundary does not perform.
        PhpType::AssocArray { key, value: element }
            if matches!(key.codegen_repr(), PhpType::Mixed | PhpType::Str)
                && element.codegen_repr() == PhpType::Mixed =>
        {
            ctx.emit_value(
                Op::MixedToHash,
                vec![value.value],
                None,
                target.clone(),
                Op::MixedToHash.default_effects(),
                span,
            )
        }
        _ => return value,
    };
    if ctx.value_is_owning_temporary(value) {
        ownership::release_if_owned(ctx, value, span);
    }
    narrowed
}
