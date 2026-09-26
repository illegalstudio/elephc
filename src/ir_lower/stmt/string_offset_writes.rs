//! Purpose:
//! Lowers PHP's string offset write `$s[$i] = $v` on a local that holds a string.
//!
//! Called from:
//! - `crate::ir_lower::stmt::array_write_core::lower_array_assign_with_diagnosed_key()`.
//! - `crate::ir_lower::expr::assignments::lower_assignment_expr()` for the expression form.
//!
//! Key details:
//! - The write never mutates the subject: `RuntimeCallTarget::StringOffsetSet` builds the
//!   updated string and the ordinary local store takes ownership of it, retiring the old
//!   value. Another variable that shares the old string keeps it, which is PHP's
//!   copy-on-write behaviour for strings.
//! - The offset resolves exactly like a string offset read (`coerce_string_offset_index`), and
//!   the value converts to a string like any string context. The runtime helper owns the
//!   illegal-offset warning, the first-byte warning, the space padding, and the empty-value
//!   `Error`.
//! - The expression's value is the byte now stored at the offset, re-read silently. After an
//!   illegal offset (no write) that re-read misses and yields an empty string where PHP gives
//!   `null` — a documented divergence.

use super::*;

/// Returns whether `$name[...] = ...` writes into a local whose storage is a concrete string.
pub(in crate::ir_lower) fn local_is_string_offset_target(
    ctx: &LoweringContext<'_, '_>,
    name: &str,
) -> bool {
    matches!(ctx.local_type(name).codegen_repr(), PhpType::Str)
}

/// Lowers `$name[$index] = $value` on a string local and returns the resolved integer offset.
///
/// Evaluates the index and value in PHP's order, resolves the offset, converts the value to a
/// string, then calls the runtime helper with the subject loaded at store time and stores the
/// updated string back into the local. The owned value temporary is pinned across the helper
/// because an empty value throws a catchable `Error` from inside it.
pub(in crate::ir_lower) fn lower_string_offset_assign(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    index: &Expr,
    value: &Expr,
    span: Span,
) -> LoweredValue {
    let (index_value, value_value) = lower_write_key_and_value(ctx, index, value);
    let offset = crate::ir_lower::expr::coerce_string_offset_index(ctx, index_value, index.span);
    let value_str = crate::ir_lower::expr::coerce_to_string_at_span(ctx, value_value, Some(span));
    let subject = ctx.load_local(name, Some(span));
    let pins = crate::ir_lower::expr::pin_in_flight_owners(ctx, &[value_str.value], span);
    let updated = ctx.emit_value(
        Op::RuntimeCall,
        vec![subject.value, offset.value, value_str.value],
        Some(Immediate::RuntimeCall(RuntimeCallTarget::StringOffsetSet)),
        PhpType::Str,
        crate::ir::Effects::READS_HEAP
            | crate::ir::Effects::ALLOC_HEAP
            | crate::ir::Effects::ALLOC_CONCAT
            | crate::ir::Effects::MAY_WARN
            | crate::ir::Effects::MAY_THROW
            | crate::ir::Effects::MAY_FATAL,
        Some(span),
    );
    crate::ir_lower::expr::unpin_in_flight_owners(ctx, pins, span);
    release_persisted_string_operand(ctx, value_str, span);
    ctx.store_local(name, updated, PhpType::Str, Some(span));
    offset
}

/// Lowers the expression form `($name[$index] = $value)` on a string local.
///
/// Performs the write, then re-reads the byte at the resolved offset without a missing-offset
/// warning: PHP's assignment result is the one-byte string it stored. When an illegal negative
/// offset wrote nothing, the silent re-read yields an empty string (PHP gives `null`).
pub(in crate::ir_lower) fn lower_string_offset_assign_expr(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    index: &Expr,
    value: &Expr,
    span: Span,
) -> LoweredValue {
    let offset = lower_string_offset_assign(ctx, name, index, value, span);
    let current = ctx.load_local(name, Some(span));
    ctx.emit_value(
        Op::StrCharAt,
        vec![current.value, offset.value],
        Some(Immediate::Bool(false)),
        PhpType::Str,
        Op::StrCharAt.default_effects(),
        Some(span),
    )
}
