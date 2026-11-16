//! Purpose:
//! Converts a `buffer_new<T>()` length to the `I64` element count the allocation takes.
//!
//! Called from:
//! - `super::scoped_values::lower_buffer_new()`.
//!
//! Key details:
//! - An `int` length passes through unchanged. A boxed `mixed` / `int|float` length (checked int
//!   arithmetic such as `$n + 1` produces one) dispatches on its runtime tag: an int payload is
//!   unboxed exactly, a float payload is truncated with the silent `FToI`, and any other payload
//!   converts both to int and to float with PHP's conversions.
//! - A count is used only when it converts back to the same float and is not `PHP_INT_MAX`,
//!   where PHP's numeric-string conversion saturates. A fraction, NaN, infinity, or a value
//!   outside the int range becomes `-1`, which `__rt_buffer_new` rejects with its length
//!   diagnostic on every target.
//! - Each path converts only the representation it runs with, so no conversion raises a PHP cast
//!   warning even after EIR passes narrow the boxed producer to a raw `int` or `float` (checked
//!   arithmetic over constants folds to a raw float constant, for which a `Cast` to int warns).
//! - Counts stay exact at every magnitude: a zero-size packed element has stride 0, so any
//!   non-negative count allocates and `buffer_len()` would show a rounded one.

use super::*;
use crate::ir::PhpTypePredicate;

/// Lowers a `buffer_new<T>()` length to an `I64` element count.
///
/// The owning boxed temporary, if any, is released once at the join, after every conversion on
/// the taken path has read it.
pub(super) fn lower_buffer_length(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
) -> LoweredValue {
    if value.ir_type == IrType::I64
        && ctx.builder.value_php_type(value.value).codegen_repr() == PhpType::Int
    {
        return value;
    }
    let selected = ctx
        .builder
        .create_named_block("buffer.length", vec![(IrType::I64, PhpType::Int)]);
    if value.ir_type == IrType::F64 {
        select_float_count(ctx, value, selected, span);
    } else {
        if matches!(value.ir_type, IrType::Heap(IrHeapKind::Mixed | IrHeapKind::Union)) {
            let not_int = branch_on_payload(ctx, value, PhpTypePredicate::Int, "int", span);
            let count = emit_cast(ctx, value, IrType::I64, PhpType::Int, span);
            ctx.builder.terminate(Terminator::Br { target: selected, args: vec![count.value] });
            ctx.builder.position_at_end(not_int);
            let other = branch_on_payload(ctx, value, PhpTypePredicate::Float, "float", span);
            let float = emit_cast(ctx, value, IrType::F64, PhpType::Float, span);
            select_float_count(ctx, float, selected, span);
            ctx.builder.position_at_end(other);
        }
        let truncated = emit_cast(ctx, value, IrType::I64, PhpType::Int, span);
        let float = emit_cast(ctx, value, IrType::F64, PhpType::Float, span);
        select_checked_count(ctx, truncated, float, selected, span);
    }
    ctx.builder.position_at_end(selected);
    release_coerced_source_if_owned(ctx, value, Some(span));
    LoweredValue { value: ctx.builder.block_param(selected, 0), ir_type: IrType::I64 }
}

/// Branches on a runtime type test of `value` and positions the builder in the matching block.
///
/// Returns the block for every other payload, which the caller positions at when it is done.
fn branch_on_payload(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    predicate: PhpTypePredicate,
    name: &str,
    span: Span,
) -> BlockId {
    let is_payload = ctx.emit_value(
        Op::TypePredicate,
        vec![value.value],
        Some(Immediate::TypePredicate(predicate)),
        PhpType::Bool,
        Op::TypePredicate.default_effects(),
        Some(span),
    );
    let payload = ctx.builder.create_named_block(format!("buffer.length.{name}"), Vec::new());
    let other = ctx.builder.create_named_block(format!("buffer.length.not_{name}"), Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_payload.value,
        then_target: payload,
        then_args: Vec::new(),
        else_target: other,
        else_args: Vec::new(),
    });
    ctx.builder.position_at_end(payload);
    other
}

/// Emits a PHP scalar `Cast` of `value` without releasing it, so several conversions can read it.
fn emit_cast(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    target: IrType,
    php_type: PhpType,
    span: Span,
) -> LoweredValue {
    ctx.emit_value(
        Op::Cast,
        vec![value.value],
        Some(Immediate::CastTarget(target)),
        php_type,
        Op::Cast.default_effects(),
        Some(span),
    )
}

/// Truncates a raw float with the silent `FToI` and selects it when it is the float's whole count.
fn select_float_count(
    ctx: &mut LoweringContext<'_, '_>,
    float: LoweredValue,
    selected: BlockId,
    span: Span,
) {
    let truncated = ctx.emit_value(
        Op::FToI,
        vec![float.value],
        None,
        PhpType::Int,
        Op::FToI.default_effects(),
        Some(span),
    );
    select_checked_count(ctx, truncated, float, selected, span);
}

/// Branches to `selected` with `truncated` when it is the whole count `float` names, else with -1.
///
/// The count must convert back (`IToF`) to the same float, which rejects a fraction, NaN (an
/// unordered comparison is false on every target), infinity and a float outside the int range.
/// It must also differ from `PHP_INT_MAX`: PHP saturates an out-of-range numeric string there,
/// and its float is 2^63, so for a non-int payload that count cannot be told apart from overflow.
fn select_checked_count(
    ctx: &mut LoweringContext<'_, '_>,
    truncated: LoweredValue,
    float: LoweredValue,
    selected: BlockId,
    span: Span,
) {
    let restored = ctx.emit_value(
        Op::IToF,
        vec![truncated.value],
        None,
        PhpType::Float,
        Op::IToF.default_effects(),
        Some(span),
    );
    let round_trips = ctx.emit_value(
        Op::FCmp,
        vec![restored.value, float.value],
        Some(Immediate::CmpPredicate(CmpPredicate::Eq)),
        PhpType::Bool,
        Op::FCmp.default_effects(),
        Some(span),
    );
    let in_range = ctx.builder.create_named_block("buffer.length.range", Vec::new());
    let accept = ctx.builder.create_named_block("buffer.length.whole", Vec::new());
    let reject = ctx.builder.create_named_block("buffer.length.reject", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: round_trips.value,
        then_target: in_range,
        then_args: Vec::new(),
        else_target: reject,
        else_args: Vec::new(),
    });
    ctx.builder.position_at_end(in_range);
    let saturated = ctx.builder.emit_const_i64(i64::MAX);
    let below_saturation = ctx.emit_value(
        Op::ICmp,
        vec![truncated.value, saturated],
        Some(Immediate::CmpPredicate(CmpPredicate::Ne)),
        PhpType::Bool,
        Op::ICmp.default_effects(),
        Some(span),
    );
    ctx.builder.terminate(Terminator::CondBr {
        cond: below_saturation.value,
        then_target: accept,
        then_args: Vec::new(),
        else_target: reject,
        else_args: Vec::new(),
    });
    ctx.builder.position_at_end(accept);
    ctx.builder.terminate(Terminator::Br { target: selected, args: vec![truncated.value] });
    ctx.builder.position_at_end(reject);
    let rejected = ctx.builder.emit_const_i64(-1);
    ctx.builder.terminate(Terminator::Br { target: selected, args: vec![rejected] });
}
