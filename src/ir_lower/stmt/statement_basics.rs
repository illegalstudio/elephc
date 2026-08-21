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

/// Emits EIR for `echo`.
pub(super) fn lower_echo(ctx: &mut LoweringContext<'_, '_>, expr: &Expr, span: Span) {
    let value = lower_expr(ctx, expr);
    if ctx.builder.insertion_block_is_terminated() {
        return;
    }
    // A float reaching the output formatter can be NaN, and PHP warns when it is
    // (`unexpected NAN value was coerced to string`). The warning is raised by `__rt_ftoa` in the
    // runtime, which has no idea what line it is on, so the ` in FILE on line N` tail can only
    // come from this instruction admitting that it may warn. The admission is made HERE rather
    // than in `default_effects` so only an echo that can actually reach a float pays for it: an
    // `echo "literal"` is by far the common case and cannot warn.
    let mut effects = Op::EchoValue.default_effects();
    if echo_can_coerce_a_float(value.ir_type) {
        effects |= crate::ir::Effects::MAY_WARN;
    }
    ctx.emit_void(Op::EchoValue, vec![value.value], None, effects, Some(span));
    if ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
}

