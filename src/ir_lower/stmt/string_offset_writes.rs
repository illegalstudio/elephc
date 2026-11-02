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
//! - The offset uses the typed `StringWriteOffset` conversion shared with nested writes,
//!   and the value converts to a string like any string context. The runtime helper owns the
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
    let (index_value, value_value, index_pins) = lower_write_key_and_value(ctx, index, value);
    let value_pins = crate::ir_lower::expr::pin_in_flight_owners(ctx, &[value_value.value], span);
    let offset = lower_string_write_offset(ctx, index_value, index.span);
    let subject = ctx.load_local(name, Some(span));
    // Only an offset still before the start after counting back from the end writes nothing,
    // and the subject length is what that is measured against: `offset + strlen >= 0`.
    let wrote = offset_reaches_string(ctx, offset, subject, span);
    let value_str = lower_checked_write_value(ctx, value_value, wrote, span);
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
    crate::ir_lower::expr::unpin_in_flight_owners(ctx, value_pins, span);
    crate::ir_lower::expr::unpin_in_flight_owners(ctx, index_pins, span);
    crate::ir_lower::ownership::release_if_owned(ctx, index_value, Some(span));
    ctx.store_local(name, updated, PhpType::Str, Some(span));
    retire_raw_write_value(ctx, value_value, value_str, span);
    let byte = want_result.then(|| first_byte(ctx, value_str, span));
    release_persisted_string_operand(ctx, value_str, span);
    Some(StringOffsetWriteResult { wrote, byte: byte? })
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
    let (cell, cell_owner) = super::nested_array_writes::root_static_update_receiver(ctx, cell, span);
    let (index_value, value_value, index_pins) = lower_write_key_and_value(ctx, index, value);
    let value_pins = crate::ir_lower::expr::pin_in_flight_owners(ctx, &[value_value.value], span);
    let offset = lower_string_write_offset(ctx, index_value, index.span);
    let subject = crate::ir_lower::expr::coerce_to_string_at_span(ctx, cell, Some(span));
    let reaches = offset_reaches_string(ctx, offset, subject, span);
    let live = boxed_receiver_has_owner(ctx, cell, span);
    let wrote = ctx.emit_value(Op::IBitAnd, vec![reaches.value, live.value], None,
        PhpType::Bool, Op::IBitAnd.default_effects(), Some(span));
    release_persisted_string_operand(ctx, subject, span);
    let value_str = lower_checked_write_value(ctx, value_value, wrote, span);
    let pins = crate::ir_lower::expr::pin_in_flight_owners(ctx, &[value_str.value], span);
    emit_boxed_string_set(ctx, cell, offset, value_str, live, span);
    crate::ir_lower::expr::unpin_in_flight_owners(ctx, pins, span);
    crate::ir_lower::expr::unpin_in_flight_owners(ctx, value_pins, span);
    crate::ir_lower::expr::unpin_in_flight_owners(ctx, index_pins, span);
    crate::ir_lower::ownership::release_if_owned(ctx, index_value, Some(span));
    retire_raw_write_value(ctx, value_value, value_str, span);
    let byte = first_byte(ctx, value_str, span);
    release_persisted_string_operand(ctx, value_str, span);
    if let Some(owner) = cell_owner {
        crate::ir_lower::expr::retire_owned_call_operand(ctx, owner, span);
    }
    StringOffsetWriteResult { wrote, byte }
}

/// Protects converted bytes while a temporary RHS destructor can throw after the write.
fn retire_raw_write_value(
    ctx: &mut LoweringContext<'_, '_>, raw: LoweredValue,
    string: LoweredValue, span: Span,
) {
    let pins = crate::ir_lower::expr::pin_in_flight_owners(ctx, &[string.value], span);
    crate::ir_lower::ownership::release_if_owned(ctx, raw, Some(span));
    crate::ir_lower::expr::unpin_in_flight_owners(ctx, pins, span);
}

/// Normalizes a write offset through the same typed EIR operation as nested string writes.
fn lower_string_write_offset(
    ctx: &mut LoweringContext<'_, '_>, index: LoweredValue, span: Span,
) -> LoweredValue {
    ctx.emit_value(
        Op::RuntimeCall, vec![index.value],
        Some(Immediate::RuntimeCall(RuntimeCallTarget::StringWriteOffset)), PhpType::Int,
        crate::ir::Effects::all(), Some(span),
    )
}

/// Keeps string keys out of array-key normalization when a boxed receiver holds a string.
pub(super) fn lower_boxed_set(
    ctx: &mut LoweringContext<'_, '_>, receiver: LoweredValue,
    key: LoweredValue, value: LoweredValue, span: Span, key_already_diagnosed: bool,
) {
    if ctx.builder.value_php_type(receiver.value).codegen_repr() != PhpType::Mixed {
        ctx.emit_void(Op::RuntimeCall, vec![receiver.value, key.value, value.value],
            key_already_diagnosed.then_some(Immediate::Bool(true)),
            Op::RuntimeCall.default_effects(), Some(span));
        return;
    }
    let string = ctx.emit_value(
        Op::TypePredicate, vec![receiver.value],
        Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::String)), PhpType::Bool,
        Op::TypePredicate.default_effects(), Some(span),
    );
    let string_block = ctx.builder.create_named_block("string.offset.store", Vec::new());
    let array_block = ctx.builder.create_named_block("array.offset.store", Vec::new());
    let merge = ctx.builder.create_named_block("offset.store.done", Vec::new());
    let initialized = ctx.initialized_slots_snapshot();
    ctx.builder.terminate(Terminator::CondBr {
        cond: string.value, then_target: string_block, then_args: Vec::new(),
        else_target: array_block, else_args: Vec::new(),
    });
    ctx.builder.position_at_end(string_block);
    let offset = lower_string_write_offset(ctx, key, span);
    let live = boxed_receiver_has_owner(ctx, receiver, span);
    emit_boxed_string_set(ctx, receiver, offset, value, live, span);
    branch_to(ctx, merge);
    ctx.builder.position_at_end(array_block);
    ctx.restore_initialized_slots(initialized.clone());
    ctx.emit_void(Op::RuntimeCall, vec![receiver.value, key.value, value.value],
        key_already_diagnosed.then_some(Immediate::Bool(true)),
        Op::RuntimeCall.default_effects(), Some(span));
    branch_to(ctx, merge);
    ctx.builder.position_at_end(merge);
    ctx.restore_initialized_slots(initialized);
}

/// Tests whether the selected cell still belongs to PHP after key conversion ran user code.
fn boxed_receiver_has_owner(
    ctx: &mut LoweringContext<'_, '_>, receiver: LoweredValue, span: Span,
) -> LoweredValue {
    let live = ctx.emit_value(Op::RuntimeCall, vec![receiver.value],
        Some(Immediate::RuntimeCall(RuntimeCallTarget::MixedCellHasOtherOwner)), PhpType::Int,
        crate::ir::Effects::READS_HEAP, Some(span));
    let zero = ctx.builder.emit_const_i64(0);
    ctx.emit_value(Op::ICmp, vec![live.value, zero],
        Some(Immediate::CmpPredicate(CmpPredicate::Ne)), PhpType::Bool,
        Op::ICmp.default_effects(), Some(span))
}

/// Abandons a detached write before converting its RHS or producing another warning.
fn emit_boxed_string_set(
    ctx: &mut LoweringContext<'_, '_>, receiver: LoweredValue, offset: LoweredValue,
    value: LoweredValue, live: LoweredValue, span: Span,
) {
    let write = ctx.builder.create_named_block("string.offset.live", Vec::new());
    let done = ctx.builder.create_named_block("string.offset.done", Vec::new());
    let initialized = ctx.initialized_slots_snapshot();
    ctx.builder.terminate(Terminator::CondBr {
        cond: live.value, then_target: write, then_args: Vec::new(),
        else_target: done, else_args: Vec::new(),
    });
    ctx.builder.position_at_end(write);
    let prepared = if ctx.builder.value_php_type(value.value).codegen_repr() == PhpType::Str {
        None
    } else {
        let subject = crate::ir_lower::expr::coerce_to_string_at_span(ctx, receiver, Some(span));
        let reaches = offset_reaches_string(ctx, offset, subject, span);
        release_persisted_string_operand(ctx, subject, span);
        Some(lower_checked_write_value(ctx, value, reaches, span))
    };
    let rhs = prepared.unwrap_or(value);
    let pins = prepared.map(|rhs| {
        crate::ir_lower::expr::pin_in_flight_owners(ctx, &[rhs.value], span)
    }).unwrap_or_default();
    ctx.emit_void(Op::RuntimeCall, vec![receiver.value, offset.value, rhs.value], None,
        Op::RuntimeCall.default_effects(), Some(span));
    crate::ir_lower::expr::unpin_in_flight_owners(ctx, pins, span);
    if let Some(rhs) = prepared { release_persisted_string_operand(ctx, rhs, span); }
    branch_to(ctx, done);
    ctx.builder.position_at_end(done);
    ctx.restore_initialized_slots(initialized);
}

/// Converts a RHS only when the write reaches its receiver, retaining raw input ownership.
fn lower_checked_write_value(
    ctx: &mut LoweringContext<'_, '_>, value: LoweredValue,
    writable: LoweredValue, span: Span,
) -> LoweredValue {
    let name = ctx.declare_owned_hidden_temp(PhpType::Str);
    let convert = ctx.builder.create_named_block("string.offset.value.convert", Vec::new());
    let skip = ctx.builder.create_named_block("string.offset.value.skip", Vec::new());
    let done = ctx.builder.create_named_block("string.offset.value.done", Vec::new());
    let initialized = ctx.initialized_slots_snapshot();
    ctx.builder.terminate(Terminator::CondBr {
        cond: writable.value, then_target: convert, then_args: Vec::new(),
        else_target: skip, else_args: Vec::new(),
    });
    ctx.builder.position_at_end(convert);
    let ty = ctx.builder.value_php_type(value.value);
    let borrowed = ctx.emit_value(Op::Borrow, vec![value.value], None, ty.clone(),
        Op::Borrow.default_effects(), Some(span));
    if Ownership::php_type_needs_lifetime_tracking(&ty) {
        ctx.builder.set_value_ownership(borrowed.value, Ownership::Borrowed);
    }
    let string = if ty.codegen_repr() == PhpType::Callable {
        let boxed = ctx.emit_owned_value(Op::MixedBox, vec![borrowed.value], None,
            PhpType::Mixed, Op::MixedBox.default_effects(), Some(span));
        let pins = crate::ir_lower::expr::pin_in_flight_owners(ctx, &[boxed.value], span);
        let string = ctx.emit_value(Op::Cast, vec![boxed.value],
            Some(Immediate::CastTarget(IrType::Str)), PhpType::Str,
            Op::Cast.default_effects(), Some(span));
        crate::ir_lower::expr::unpin_in_flight_owners(ctx, pins, span);
        crate::ir_lower::ownership::release_if_owned(ctx, boxed, Some(span));
        string
    } else if matches!(ty.codegen_repr(), PhpType::Bool | PhpType::False) {
        ctx.emit_value(Op::BoolToStr, vec![borrowed.value], None, PhpType::Str,
            Op::BoolToStr.default_effects(), Some(span))
    } else {
        crate::ir_lower::expr::coerce_to_string_at_span(ctx, borrowed, Some(span))
    };
    crate::ir_lower::expr::store_value_into_temp(ctx, &name, PhpType::Str, string, span);
    let converted_initialized = ctx.initialized_slots_snapshot();
    branch_to(ctx, done);
    ctx.builder.position_at_end(skip);
    ctx.restore_initialized_slots(initialized);
    let empty = lower_expr(ctx, &Expr::new(ExprKind::StringLiteral(String::new()), span));
    crate::ir_lower::expr::store_value_into_temp(ctx, &name, PhpType::Str, empty, span);
    let skipped_initialized = ctx.initialized_slots_snapshot();
    branch_to(ctx, done);
    ctx.builder.position_at_end(done);
    ctx.restore_initialized_slots(converted_initialized.intersection(&skipped_initialized).copied().collect());
    let detached = crate::ir_lower::expr::take_owned_temp(ctx, &name, span);
    // A detached merge result owns its bytes; ordinary local loads are excluded from unwind pins.
    ctx.emit_owned_value(Op::EnsureOwned, vec![detached.value], None, PhpType::Str,
        Op::EnsureOwned.default_effects(), Some(span))
}

/// Tests whether an offset reaches the string without rejecting overflowing positive offsets.
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
    let from_end = ctx.emit_value(
        Op::ICmp,
        vec![from_start.value, zero],
        Some(Immediate::CmpPredicate(CmpPredicate::Sge)),
        PhpType::Bool,
        Op::ICmp.default_effects(),
        Some(span),
    );
    let nonnegative = ctx.emit_value(Op::ICmp, vec![offset.value, zero],
        Some(Immediate::CmpPredicate(CmpPredicate::Sge)), PhpType::Bool,
        Op::ICmp.default_effects(), Some(span));
    ctx.emit_value(Op::IBitOr, vec![nonnegative.value, from_end.value], None,
        PhpType::Bool, Op::IBitOr.default_effects(), Some(span))
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
