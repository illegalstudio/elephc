//! Purpose:
//! Provides small helpers for explicit EIR ownership operations during
//! AST-to-EIR lowering.
//!
//! Called from:
//! - `crate::ir_lower::stmt` and `crate::ir_lower::expr` when values cross
//!   assignment, call, and cleanup boundaries.
//!
//! Key details:
//! - Ownership is represented by explicit EIR opcodes even though the legacy
//!   backend is still the production path.

#![allow(dead_code)]

use crate::ir::{Op, Ownership};
use crate::ir_lower::context::{LoweredValue, LoweringContext};
use crate::span::Span;

/// Emits an acquire operation when the value can carry runtime lifetime state.
pub(crate) fn acquire_if_refcounted(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<Span>,
) -> LoweredValue {
    let php_type = ctx.builder.value_php_type(value.value);
    if Ownership::php_type_needs_lifetime_tracking(&php_type) {
        return ctx.emit_value(
            Op::Acquire,
            vec![value.value],
            None,
            php_type,
            Op::Acquire.default_effects(),
            span,
        );
    }
    value
}

/// Emits a release operation when the value can carry runtime lifetime state.
pub(crate) fn release_if_owned(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, span: Option<Span>) {
    let php_type = ctx.builder.value_php_type(value.value);
    if Ownership::php_type_needs_lifetime_tracking(&php_type)
        && !matches!(php_type, crate::types::PhpType::Void)
    {
        ctx.emit_void(
            Op::Release,
            vec![value.value],
            None,
            Op::Release.default_effects(),
            span,
        );
    }
}

/// Emits an explicit cycle-collection safe point after PHP roots were updated.
pub(crate) fn collect_cycles(ctx: &mut LoweringContext<'_, '_>, span: Option<Span>) {
    ctx.emit_void(
        Op::GcCollect,
        Vec::new(),
        None,
        Op::GcCollect.default_effects(),
        span,
    );
}

/// Returns whether an ownership state means the value is potentially released by this path.
pub(crate) fn may_require_release(ownership: Ownership) -> bool {
    matches!(ownership, Ownership::Owned | Ownership::MaybeOwned)
}

/// Releases a boxed `Mixed` temporary produced by overflow-checked integer
/// arithmetic and consumed by a scalar (`int`/`float`/`bool`) cast or coercion whose
/// unboxed result cannot alias the source cell.
///
/// `ICheckedAdd`/`ICheckedSub`/`ICheckedMul` box their result into a fresh `Mixed`
/// cell (to carry an overflow-promoted float). When such a result feeds only a scalar
/// cast/coercion — e.g. the `$i * 2` narrowed to fill an `array<int>` element on
/// `$arr[$i] = $i * 2` — the cast reads the payload into a fresh scalar register and
/// the box has no remaining consumer, yet the emitting `Op::Cast` carries no trailing
/// `release`, so the cell leaks one per cast/coercion. Emitting the release here fixes
/// that. Shared by the explicit-cast path (`expr::lower_cast`) and the implicit
/// numeric-coercion path (`stmt::coerce_to_int`/`coerce_to_float`).
///
/// Scoped narrowly to a checked-arithmetic `Mixed` producer on purpose: a general
/// "owning `Mixed` temporary" would also match an `Op::Acquire`d value that is stored
/// elsewhere and released by its own lifecycle (e.g. `static $c = 0; return ++$c;`,
/// where the `++` box is stored into the static AND cast for the return AND already
/// `release`d twice) — releasing there over-frees the cell and breaks `--web-worker`
/// state persistence. A checked-arithmetic result has no such second consumer.
pub(crate) fn release_unboxed_scalar_source_if_owned(
    ctx: &mut LoweringContext<'_, '_>,
    source: LoweredValue,
    span: Option<Span>,
) {
    if !matches!(
        ctx.builder.value_defining_op(source.value),
        Some(Op::ICheckedAdd | Op::ICheckedSub | Op::ICheckedMul)
    ) {
        return;
    }
    if matches!(
        ctx.builder.value_php_type(source.value).codegen_repr(),
        crate::types::PhpType::Mixed
    ) {
        release_if_owned(ctx, source, span);
    }
}
