//! Purpose:
//! Expression-statement cleanup, block lowering, and echo emission.
//!
//! Called from:
//! - `crate::ir_lower::stmt`.
//!
//! Key details:
//! - Preserves statement ordering, CFG shape, EIR effects, and ownership contracts.

use super::*;

/// Releases a discarded expression-statement result when it may own temporary storage.
pub(super) fn release_expr_statement_result(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
) {
    if ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
}

/// Emits the statement-boundary concat-buffer reset expected by the ASM backend.
///
/// Skipped for compiler-generated statements — which is every statement in an injected prelude,
/// whether it carries `dummy()` or a synthetic span. Testing `line == 0` here would have started
/// emitting resets for prelude loops the moment they were given distinct spans.
pub(super) fn lower_statement_concat_reset(ctx: &mut LoweringContext<'_, '_>, span: Span) {
    if !span.is_from_source() {
        return;
    }
    ctx.emit_void(
        Op::ConcatReset,
        vec![],
        None,
        Op::ConcatReset.default_effects(),
        Some(span),
    );
}

/// Lowers a sequence of statements until the current block terminates.
pub(super) fn lower_block(ctx: &mut LoweringContext<'_, '_>, body: &[Stmt]) {
    for stmt in body {
        lower_stmt(ctx, stmt);
        if ctx.builder.insertion_block_is_terminated() {
            break;
        }
    }
}

/// Returns true when an echoed value can reach the float-to-string formatter.
///
/// A `Mixed` cell is included because its echo ladder dispatches on the runtime tag and takes the
/// float arm for a float — the type that says "could be anything" is exactly the one that cannot
/// rule a NaN out.
fn echo_can_coerce_a_float(ir_type: crate::ir::IrType) -> bool {
    matches!(
        ir_type,
        crate::ir::IrType::F64 | crate::ir::IrType::Heap(crate::ir::IrHeapKind::Mixed)
    )
}

/// Returns whether a value reaching the output formatter can raise a php warning.
///
/// Two shapes can: a float, which warns when it is NaN, and an ARRAY, which prints the literal
/// `Array` and raises `Array to string conversion`. Both warnings are raised inside the runtime,
/// which has no idea what line it is on, so the ` in FILE on line N` tail can only come from the
/// instruction admitting that it may warn. The admission is made here rather than in
/// `default_effects` so an `echo "literal"` — by far the common case, and unable to warn — does
/// not pay for it.
///
/// Shared with `print`, which renders through the same path and raises the same warnings.
pub(crate) fn output_value_can_warn(ir_type: crate::ir::IrType) -> bool {
    echo_can_coerce_a_float(ir_type)
        || matches!(
            ir_type,
            crate::ir::IrType::Heap(
                crate::ir::IrHeapKind::Array
                    | crate::ir::IrHeapKind::Hash
                    | crate::ir::IrHeapKind::Iterable
                    | crate::ir::IrHeapKind::Union
            )
        )
}

/// Emits EIR for `echo`.
pub(super) fn lower_echo(ctx: &mut LoweringContext<'_, '_>, expr: &Expr, span: Span) {
    let value = lower_expr(ctx, expr);
    if ctx.builder.insertion_block_is_terminated() {
        return;
    }
    let mut effects = Op::EchoValue.default_effects();
    if output_value_can_warn(value.ir_type) {
        effects |= crate::ir::Effects::MAY_WARN;
    }
    ctx.emit_void(Op::EchoValue, vec![value.value], None, effects, Some(span));
    if ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
}

