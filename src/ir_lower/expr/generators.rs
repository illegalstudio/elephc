//! Purpose:
//! Yield and yield-from expression lowering.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers `yield`.
pub(super) fn lower_yield(ctx: &mut LoweringContext<'_, '_>, key: Option<&Expr>, value: Option<&Expr>, expr: &Expr) -> LoweredValue {
    let mut operands = Vec::new();
    if let Some(key) = key {
        operands.push(lower_expr(ctx, key).value);
    }
    if let Some(value) = value {
        operands.push(lower_expr(ctx, value).value);
    }
    ctx.emit_value(Op::GeneratorYield, operands, None, PhpType::Mixed, Op::GeneratorYield.default_effects(), Some(expr.span))
}

/// Lowers `yield from`.
///
/// `yield from <generator|Traversable>` lowers to `Op::GeneratorYieldFrom`, which
/// the backend delegates to `__rt_gen_delegate` (forwarding sent values and
/// producing the inner generator's return value). `yield from <array>` is
/// desugared here into an iterator loop that re-yields each key/value pair,
/// reusing the foreach iterator opcodes; its result is PHP null.
pub(super) fn lower_yield_from(ctx: &mut LoweringContext<'_, '_>, inner: &Expr, expr: &Expr) -> LoweredValue {
    let value = lower_expr(ctx, inner);
    let source_ty = ctx.builder.value_php_type(value.value).codegen_repr();
    if matches!(source_ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
        return lower_yield_from_array(ctx, value, expr);
    }
    let result = ctx.emit_value(
        Op::GeneratorYieldFrom,
        vec![value.value],
        None,
        PhpType::Mixed,
        Op::GeneratorYieldFrom.default_effects(),
        Some(expr.span),
    );
    // `__rt_gen_delegate` borrows the inner generator. When it is a fresh owning
    // temporary (`yield from inner()`) nothing else frees it once delegation
    // ends, so release it here; a borrowed local (`yield from $g`) keeps its
    // owner and must not be released (it would double-free at scope end).
    if ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(expr.span));
    }
    result
}

/// Desugars `yield from <array>` into an iterator loop that re-yields each
/// key/value pair, returning a boxed PHP null (arrays have no delegated return
/// value). Reuses the foreach iterator opcodes so every array kind (indexed,
/// associative, by-element-type) is handled by the existing iterator lowering.
pub(super) fn lower_yield_from_array(
    ctx: &mut LoweringContext<'_, '_>,
    source: LoweredValue,
    expr: &Expr,
) -> LoweredValue {
    let span = expr.span;
    let iterator = ctx.emit_value(
        Op::IterStart,
        vec![source.value],
        Some(Immediate::IterStart {
            by_ref: false,
            owner: None,
        }),
        PhpType::Iterable,
        Op::IterStart.default_effects(),
        Some(span),
    );
    let header = ctx.builder.create_named_block("yieldfrom.next", Vec::new());
    let body = ctx.builder.create_named_block("yieldfrom.body", Vec::new());
    let exit = ctx.builder.create_named_block("yieldfrom.exit", Vec::new());
    if !ctx.builder.insertion_block_is_terminated() {
        ctx.builder.terminate(Terminator::Br {
            target: header,
            args: Vec::new(),
        });
    }

    ctx.builder.position_at_end(header);
    let has_next = ctx.emit_value(
        Op::IterNext,
        vec![iterator.value],
        None,
        PhpType::Bool,
        Op::IterNext.default_effects(),
        Some(span),
    );
    ctx.builder.terminate(Terminator::CondBr {
        cond: has_next.value,
        then_target: body,
        then_args: Vec::new(),
        else_target: exit,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(body);
    let key = ctx.emit_value(
        Op::IterCurrentKey,
        vec![iterator.value],
        None,
        PhpType::Mixed,
        Op::IterCurrentKey.default_effects(),
        Some(span),
    );
    let element = ctx.emit_value(
        Op::IterCurrentValue,
        vec![iterator.value],
        None,
        PhpType::Mixed,
        Op::IterCurrentValue.default_effects(),
        Some(span),
    );
    // Re-yield the inner key/value pair through the outer generator. The sent
    // value is discarded (arrays ignore it). The `Immediate::Bool(true)` marks
    // the yield as *delegated*: PHP forwards `yield from` keys verbatim, so an
    // integer key from the inner array must not advance the outer generator's
    // implicit-key counter (`yield from [7 => "x"]` leaves it where it was).
    ctx.emit_value(
        Op::GeneratorYield,
        vec![key.value, element.value],
        Some(Immediate::Bool(true)),
        PhpType::Mixed,
        Op::GeneratorYield.default_effects(),
        Some(span),
    );
    if !ctx.builder.insertion_block_is_terminated() {
        ctx.builder.terminate(Terminator::Br {
            target: header,
            args: Vec::new(),
        });
    }

    ctx.builder.position_at_end(exit);
    // The iterator borrows a freshly-created array (e.g. a literal): release it
    // once iteration ends, mirroring `lower_foreach`.
    if ctx.value_is_owning_temporary(source) {
        crate::ir_lower::ownership::release_if_owned(ctx, source, Some(span));
    }
    let null_value = ctx
        .builder
        .emit_with_effects(
            Op::ConstNull,
            Vec::new(),
            None,
            IrType::I64,
            PhpType::Void,
            Ownership::NonHeap,
            Op::ConstNull.default_effects(),
            Some(span),
        )
        .expect("const_null produces a value");
    // A fresh null is non-refcounted: there is no producer reference to release,
    // so this boxes directly rather than via box_value_as_mixed (issue #484).
    ctx.emit_value(
        Op::MixedBox,
        vec![null_value],
        None,
        PhpType::Mixed,
        Op::MixedBox.default_effects(),
        Some(span),
    )
}
