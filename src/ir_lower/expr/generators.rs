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
    let mut key_owner = if let Some(key) = key {
        let key = lower_expr(ctx, key);
        let (key, owner) = root_owned_call_operand(ctx, key, expr.span);
        operands.push(key.value);
        owner
    } else {
        None
    };
    let yielded = if let Some(value) = value {
        let yielded = lower_expr(ctx, value);
        operands.push(yielded.value);
        Some(yielded)
    } else {
        None
    };
    if key_owner.is_some() && ctx.builder.value_php_type(operands[0]).codegen_repr() == PhpType::Str {
        let owner = key_owner.take().unwrap();
        // The key's box must own its bytes before its source owner retires. Transfer this
        // single-use box to the yield runtime so no new owner remains on a suspended stack.
        let key = ctx.emit_value(
            Op::MixedBox, vec![operands[0]], None, PhpType::Mixed,
            Op::MixedBox.default_effects(), Some(expr.span),
        );
        retire_owned_call_operand(ctx, owner, expr.span);
        operands[0] = key.value;
    }
    let result = ctx.emit_value(Op::GeneratorYield, operands, None, PhpType::Mixed,
        Op::GeneratorYield.default_effects(), Some(expr.span));
    if let Some(owner) = key_owner {
        retire_owned_call_operand(ctx, owner, expr.span);
    }
    // The yield boxes its own copy of the string. The cast copy under `trim`
    // is not that cell and would otherwise stay live for every suspension.
    if let Some(yielded) = yielded {
        crate::ir_lower::ownership::release_marked_normalized_cast_copies(
            ctx, yielded, Some(expr.span),
        );
    }
    result
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
    let staging = prepublish_user_call_result(
        ctx, None, &ReturnArgAlias::None, &PhpType::Mixed, expr.span,
    );
    let (value, owner) = root_owned_call_operand(ctx, value, expr.span);
    let result = ctx.emit_value(
        Op::GeneratorYieldFrom,
        vec![value.value],
        None,
        PhpType::Mixed,
        Op::GeneratorYieldFrom.default_effects(),
        Some(expr.span),
    );
    stage_call_result(ctx, staging.as_ref(), result, expr.span);
    // Delegation can suspend or throw before returning. Its temporary owner must
    // unwind before a same-frame finally, while borrowed locals keep their owner.
    if let Some(owner) = owner {
        retire_owned_call_operand(ctx, owner, expr.span);
    }
    take_prepublished_call_result(ctx, staging, result, expr.span)
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
    let iterator_state = ctx.declare_iterator_state();
    let iterator = ctx.emit_value(
        Op::IterStart,
        vec![source.value],
        Some(Immediate::IterStart(crate::ir::IterStartMetadata::new(
            iterator_state,
            false,
            None,
            None,
        ))),
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
