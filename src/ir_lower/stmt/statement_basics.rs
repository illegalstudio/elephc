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
    if ctx.value_needs_release_after_use(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
}

/// Emits the statement-boundary concat-buffer reset expected by the ASM backend.
///
/// Emitted for EVERY statement, including compiler-generated ones. It used to be skipped for
/// any statement whose span was not from source, which is every statement in an injected
/// prelude — and a prelude is exactly where it matters most, because a prelude is where the
/// compiler puts recursive, string-building PHP that the user never sees.
///
/// Without it a prelude's scratch use only ever grows. `__elephc_var_export_float` builds its
/// digit string through a dozen intermediate `substr`/`str_replace`/concat steps, none of
/// which were ever reclaimed, so every float cost roughly 2 KB of the shared 64 KiB
/// `_concat_buf` for the whole call. Two dozen floats in one `var_export()` exhausted it and
/// the next `sprintf` fatalled with `formatted result exceeds the 65536-byte string buffer` —
/// an absurd diagnostic for a one-kilobyte result, and one that pointed at the wrong function.
/// `var_export(opcache_get_configuration())` reproduced it: 54 directives, two of them floats.
///
/// The reset rewinds `_concat_off` to the FRAME base, not to zero, so a prelude function can
/// only ever reclaim what it allocated itself; a caller's accumulated output is out of reach
/// by construction. Values that must outlive a statement have been persisted by then — a `Str`
/// store goes through `__rt_str_persist` — which is the same premise that already made this
/// safe for user code.
pub(super) fn lower_statement_concat_reset(ctx: &mut LoweringContext<'_, '_>, span: Span) {
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

/// Emits output and retires owned reads, including deferred string unboxes from widened slots.
pub(super) fn lower_echo(ctx: &mut LoweringContext<'_, '_>, expr: &Expr, span: Span) {
    let value = lower_expr(ctx, expr);
    if ctx.builder.insertion_block_is_terminated() {
        return;
    }
    ctx.emit_void(
        Op::EchoValue,
        vec![value.value],
        None,
        Op::EchoValue.default_effects(),
        Some(span),
    );
    if ctx.value_needs_release_after_use(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
}
