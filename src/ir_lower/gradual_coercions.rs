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
            Op::Cast.default_effects(),
            span,
        ),
        PhpType::Float => ctx.emit_value(
            Op::Cast,
            vec![value.value],
            Some(Immediate::CastTarget(IrType::F64)),
            target.clone(),
            Op::Cast.default_effects(),
            span,
        ),
        PhpType::Bool => ctx.emit_value(
            Op::Cast,
            vec![value.value],
            Some(Immediate::CastTarget(IrType::I64)),
            target.clone(),
            Op::Cast.default_effects(),
            span,
        ),
        PhpType::Str => ctx.emit_value(
            Op::Cast,
            vec![value.value],
            Some(Immediate::CastTarget(IrType::Str)),
            target.clone(),
            Op::Cast.default_effects(),
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
