//! Purpose:
//! Lowers PHP's string offset write `$s[$i] = $v` on a local that holds a string.
//!
//! Called from:
//! - `crate::ir_lower::stmt::array_write_core::lower_array_assign_with_diagnosed_key()`.
//! - `crate::ir_lower::expr::string_offset_assign` for the expression form, including the
//!   runtime-string branch of a boxed (`mixed`) local.
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
//! - A boxed local that holds a string at run time keeps going through the boxed writer
//!   (`__rt_mixed_array_set`), which retains the cell across a warning handler and abandons
//!   the write when the handler replaced the variable, as php does.
//! - The expression form needs PHP's result, which is the first byte of the value, or `null`
//!   when the offset lies before the start and nothing was written. The write reports both
//!   facts (`StringOffsetWriteResult`) instead of re-reading the subject.

use super::*;

/// What the expression form of a string offset write needs after the write ran.
pub(in crate::ir_lower) struct StringOffsetWriteResult {
    /// `true` when the offset reached the string, i.e. a byte was written.
    pub(in crate::ir_lower) wrote: LoweredValue,
    /// An owned one-byte string: the value's first byte, which PHP returns.
    pub(in crate::ir_lower) byte: LoweredValue,
}

/// Returns whether `$name[...] = ...` writes into a local whose storage is a concrete string.
pub(in crate::ir_lower) fn local_is_string_offset_target(
    ctx: &LoweringContext<'_, '_>,
    name: &str,
) -> bool {
    matches!(ctx.local_type(name).codegen_repr(), PhpType::Str)
}

/// Returns whether a boxed local may hold a string at runtime, so an offset write into it may
/// be a string offset write rather than an array write.
///
/// That is a `mixed` local (also a `string` local that `++` gave boxed storage) or a union
/// with a `string` member. A concrete `string` local is `local_is_string_offset_target`.
pub(in crate::ir_lower) fn local_may_hold_boxed_string(
    ctx: &LoweringContext<'_, '_>,
    name: &str,
) -> bool {
    match ctx.local_type(name) {
        PhpType::Mixed => true,
        PhpType::Union(members) => members.iter().any(|member| matches!(member, PhpType::Str)),
        _ => false,
    }
}

/// Lowers the statement form `$name[$index] = $value` on a string local.
pub(in crate::ir_lower) fn lower_string_offset_assign(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    index: &Expr,
    value: &Expr,
    span: Span,
) {
    lower_string_offset_write(ctx, name, index, value, span, false);
}

/// Lowers `$name[$index] = $value` on a concrete `string` local and, when `want_result` is
/// set, returns what the expression form evaluates to.
///
/// Evaluates the index and value once, in PHP's order, resolves the offset, converts the value
/// to a string, then calls the runtime helper with the subject loaded at store time and stores
/// the updated string back into the local. The owned value temporary is pinned across the
/// helper because an empty value throws a catchable `Error` from inside it.
pub(in crate::ir_lower) fn lower_string_offset_write(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    index: &Expr,
    value: &Expr,
    span: Span,
    want_result: bool,
) -> Option<StringOffsetWriteResult> {
    let (index_value, value_value) = lower_write_key_and_value(ctx, index, value);
    let offset = crate::ir_lower::expr::coerce_string_offset_index(ctx, index_value, index.span);
    let value_str = crate::ir_lower::expr::coerce_to_string_at_span(ctx, value_value, Some(span));
    let subject = ctx.load_local(name, Some(span));
    // Only an offset still before the start after counting back from the end writes nothing,
    // and the subject length is what that is measured against: `offset + strlen >= 0`.
    let wrote = want_result.then(|| offset_reaches_string(ctx, offset, subject, span));
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
    let byte = want_result.then(|| first_byte(ctx, value_str, span));
    release_persisted_string_operand(ctx, value_str, span);
    ctx.store_local(name, updated, PhpType::Str, Some(span));
    Some(StringOffsetWriteResult { wrote: wrote?, byte: byte? })
}

/// Lowers `$name[$index] = $value` on a boxed local that holds a string at run time, and
/// returns what the expression form evaluates to.
///
/// The index and value are evaluated once and resolved the way a string offset write resolves
/// them; the write itself stays with the boxed writer that the statement form uses, which owns
/// the cell's copy-on-write detach and its protection against a warning handler that replaces
/// the variable. The subject length is read before the write to tell whether the offset
/// reaches the string.
pub(in crate::ir_lower) fn lower_boxed_string_offset_write(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    index: &Expr,
    value: &Expr,
    span: Span,
) -> StringOffsetWriteResult {
    let cell = load_array_local_for_write(ctx, name, span);
    let (index_value, value_value) = lower_write_key_and_value(ctx, index, value);
    let offset = crate::ir_lower::expr::coerce_string_offset_index(ctx, index_value, index.span);
    let value_str = crate::ir_lower::expr::coerce_to_string_at_span(ctx, value_value, Some(span));
    let subject = crate::ir_lower::expr::coerce_to_string_at_span(ctx, cell, Some(span));
    let wrote = offset_reaches_string(ctx, offset, subject, span);
    release_persisted_string_operand(ctx, subject, span);
    let pins = crate::ir_lower::expr::pin_in_flight_owners(ctx, &[value_str.value], span);
    ctx.emit_void(
        Op::RuntimeCall,
        vec![cell.value, offset.value, value_str.value],
        None,
        Op::RuntimeCall.default_effects(),
        Some(span),
    );
    crate::ir_lower::expr::unpin_in_flight_owners(ctx, pins, span);
    let byte = first_byte(ctx, value_str, span);
    release_persisted_string_operand(ctx, value_str, span);
    StringOffsetWriteResult { wrote, byte }
}

/// Emits `offset + strlen(subject) >= 0`: whether a resolved string offset reaches the string.
fn offset_reaches_string(
    ctx: &mut LoweringContext<'_, '_>,
    offset: LoweredValue,
    subject: LoweredValue,
    span: Span,
) -> LoweredValue {
    let length = ctx.emit_value(
        Op::StrLen,
        vec![subject.value],
        None,
        PhpType::Int,
        Op::StrLen.default_effects(),
        Some(span),
    );
    let from_start = ctx.emit_value(
        Op::IAdd,
        vec![offset.value, length.value],
        None,
        PhpType::Int,
        Op::IAdd.default_effects(),
        Some(span),
    );
    let zero = ctx.builder.emit_const_i64(0);
    ctx.emit_value(
        Op::ICmp,
        vec![from_start.value, zero],
        Some(Immediate::CmpPredicate(CmpPredicate::Sge)),
        PhpType::Bool,
        Op::ICmp.default_effects(),
        Some(span),
    )
}

/// Emits the first byte of an already converted value string as an owned one-byte string.
///
/// That byte is what PHP's string offset assignment evaluates to. `StrCharAt` only points
/// into the value string, which is released right after the write, so the byte is persisted
/// into storage of its own. The read is silent: the runtime helper already threw for an empty
/// value, and an empty value that only reached the illegal-offset path is never returned.
fn first_byte(ctx: &mut LoweringContext<'_, '_>, value_str: LoweredValue, span: Span) -> LoweredValue {
    let zero = ctx.builder.emit_const_i64(0);
    let slice = ctx.emit_value(
        Op::StrCharAt,
        vec![value_str.value, zero],
        Some(Immediate::Bool(false)),
        PhpType::Str,
        Op::StrCharAt.default_effects(),
        Some(span),
    );
    ctx.emit_value(
        Op::StrPersist,
        vec![slice.value],
        None,
        PhpType::Str,
        Op::StrPersist.default_effects(),
        Some(span),
    )
}
