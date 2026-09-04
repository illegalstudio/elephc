//! Purpose:
//! Emits catchable built-in `Error` and `TypeError` objects for codegen guards.
//!
//! Called from:
//! - EIR instruction lowerers that detect PHP runtime type/null errors.
//!
//! Key details:
//! - Active handlers receive a normal throwable through `__rt_throw_current`.
//! - Unhandled errors keep a specific PHP-style fatal diagnostic instead of the
//!   runtime unwinder's generic uncaught-exception fallback.
//! - That diagnostic is written HERE, before the throwable is allocated, so it never reaches
//!   `__rt_report_uncaught_exception` and shares none of its logic. The exit status is therefore
//!   imported rather than spelled out: an uncaught `DivisionByZeroError` and an uncaught
//!   `throw new RuntimeException(...)` must not leave a script with different `$?` values.
//! - These messages carry no ` in <file>:<line>` suffix, unlike the unwinder's. The error is
//!   synthesized by a codegen guard rather than by a user `new`, and the message string is baked
//!   at emit time from a caller that passes no span — so there is no origin to print. Reference
//!   PHP does report one here (the operation's own line), which stays a known gap.
//! - `emit_value_error_unless()` is the shared builtin argument-range guard: it keeps
//!   out-of-range arguments (empty separators, non-positive lengths, negative counts,
//!   oversized array lengths) from ever reaching a runtime helper that would read
//!   uninitialized memory, allocate an unrepresentable size, or loop forever, and raises
//!   reference PHP's catchable `ValueError` instead.
//! - `emit_value_error_from_string_result()` is the same guard outcome for php-src's
//!   `ValueError`s that interpolate the offending values into their wording, where the caller
//!   has already built the exact message at runtime.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen_support::runtime::UNCAUGHT_EXIT_STATUS;
use crate::ir::ValueId;

use super::super::context::FunctionContext;
use super::super::Result;

/// Throws a catchable PHP `Error` carrying a static message.
pub(super) fn emit_error(ctx: &mut FunctionContext<'_>, message: &str) {
    emit_static_exception(ctx, "Error", "_spl_error_class_id", message);
}

/// Throws a catchable PHP `TypeError` carrying a static message.
///
/// NEVER RETURNS. Both targets end in an unconditional jump to `__rt_throw_current`, so a caller
/// may emit one of these in the middle of a straight-line sequence and let the instructions after
/// it be the NOT-taken path — several tag ladders are written that way. Nothing in the emitted
/// shape says so, which is why it is said here.
pub(super) fn emit_type_error(ctx: &mut FunctionContext<'_>, message: &str) {
    emit_static_exception(ctx, "TypeError", "_spl_type_error_class_id", message);
}

/// Throws a catchable PHP `ValueError` carrying a static message.
///
/// Reference PHP raises this `Error` subclass — not a fatal — when a builtin argument has
/// the right type but a value the function cannot honor (`str_pad()` with an empty pad
/// string, `str_split()` with a non-positive chunk length, `str_repeat()` with a negative
/// count, `explode()` with an empty separator, `array_fill()` with a negative count,
/// `random_int()` with `$min > $max`). `catch (ValueError $e)`, `catch (Error $e)`, and
/// `catch (Throwable $e)` all match; callers pass php-src's own verbatim wording.
pub(super) fn emit_value_error(ctx: &mut FunctionContext<'_>, message: &str) {
    emit_static_exception(ctx, "ValueError", "_spl_value_error_class_id", message);
}

/// The register condition a materialized builtin argument must satisfy to skip its
/// `ValueError`.
///
/// The register is inspected right before the runtime helper call, while the argument
/// still sits in its target ABI register, so the same guard works for every supported
/// target without re-materializing the operand.
pub(super) enum ValueGuard<'a> {
    /// The 64-bit register, read as a signed integer, must be `>= minimum`
    /// (`str_split()` chunk length, `str_repeat()`/`array_fill()` counts).
    SignedAtLeast(&'a str, i64),
    /// The 64-bit register, read as a signed integer, must be `<= maximum`
    /// (`array_fill()`'s `$count` ceiling).
    ///
    /// The bound is materialized into a scratch register first, so a limit wider than the
    /// target's compare-immediate encoding (`INT_MAX` does not fit AArch64's 12-bit form)
    /// is still checked exactly instead of being truncated by the assembler.
    SignedAtMost(&'a str, i64),
    /// The 64-bit register, read as a signed integer, must satisfy
    /// `-maximum <= value <= maximum` (`array_pad()`'s `$length` magnitude).
    ///
    /// The bound is checked on the signed argument itself rather than on `abs(value)`
    /// so `PHP_INT_MIN`, whose magnitude is not representable, fails the guard instead
    /// of wrapping back to a negative "absolute" length.
    SignedMagnitudeAtMost(&'a str, i64),
    /// The 64-bit register, read as a signed integer, must satisfy
    /// `minimum <= value <= maximum` (`round()`'s `$mode` enumeration).
    ///
    /// Both ends are inclusive; the guard is used for builtin arguments whose accepted
    /// values are a small contiguous set of PHP constants rather than a magnitude limit.
    SignedInRange(&'a str, i64, i64),
    /// The 64-bit register must not hold the given immediate (`range()`'s zero `$step`).
    NotEqualToImmediate(&'a str, i64),
    /// The first register, read as a signed integer, must be `>= 0` unless the second
    /// register is signed-greater-or-equal to the third (`range()`'s `$step` sign rule).
    ///
    /// PHP only rejects a negative `$step` for an INCREASING range: `range(5, 1, -2)` is
    /// valid while `range(1, 5, -2)` is a `ValueError`. The guard therefore passes as soon
    /// as `start >= end`, and only then checks the sign of the step.
    NonNegativeUnlessSignedBelow(&'a str, &'a str, &'a str),
    /// `|first|` must not exceed the UNSIGNED width the second and third registers span,
    /// unless those two are equal (`range()`'s `$step` magnitude rule).
    ///
    /// PHP rejects a `$step` wider than the interval its endpoints span, but a degenerate
    /// `range($x, $x, $step)` always yields `[$x]` no matter how large the step is, so an
    /// equal pair short-circuits the check. The unsigned comparison makes `PHP_INT_MIN`
    /// (whose negation is itself) read as wider than every span instead of wrapping back
    /// into a negative "magnitude" that would slip past a signed compare.
    ///
    /// The endpoints are ordered before the width is taken, and the subtraction that follows
    /// is read as unsigned, exactly like php-src's `(zend_ulong) (high - low)`. Taking a
    /// signed absolute of the raw difference instead would report `range(PHP_INT_MIN,
    /// PHP_INT_MAX, 2)` as a span of `1` and reject it, where PHP spans `2^64 - 1` and goes on
    /// to reject the range for its size instead.
    MagnitudeWithinSpan(&'a str, &'a str, &'a str),
}

/// Throws a catchable PHP `ValueError` unless the guarded register satisfies `guard`.
///
/// Emits the compare/branch pair for the active target, falls through to the throw
/// sequence when the guard fails, and leaves the caller's continuation label in place so
/// the runtime helper call that follows only ever runs with an in-range argument.
pub(super) fn emit_value_error_unless(
    ctx: &mut FunctionContext<'_>,
    guard: ValueGuard<'_>,
    message: &str,
) {
    let ok_label = ctx.next_label("value_guard_ok");
    match (ctx.emitter.target.arch, &guard) {
        (Arch::AArch64, ValueGuard::SignedAtLeast(reg, minimum)) => {
            ctx.emitter.instruction(&format!("cmp {}, #{}", reg, minimum));     // compare the materialized argument against its PHP minimum
            ctx.emitter.instruction(&format!("b.ge {}", ok_label));             // an argument at or above the minimum is in range
        }
        (Arch::X86_64, ValueGuard::SignedAtLeast(reg, minimum)) => {
            ctx.emitter.instruction(&format!("cmp {}, {}", reg, minimum));      // compare the materialized argument against its PHP minimum
            ctx.emitter.instruction(&format!("jge {}", ok_label));              // an argument at or above the minimum is in range
        }
        (Arch::AArch64, ValueGuard::SignedAtMost(reg, maximum)) => {
            abi::emit_load_int_immediate(ctx.emitter, "x9", *maximum);
            ctx.emitter.instruction(&format!("cmp {}, x9", reg));               // compare the materialized argument against its PHP maximum
            ctx.emitter.instruction(&format!("b.le {}", ok_label));             // an argument at or below the maximum is in range
        }
        (Arch::X86_64, ValueGuard::SignedAtMost(reg, maximum)) => {
            abi::emit_load_int_immediate(ctx.emitter, "r10", *maximum);
            ctx.emitter.instruction(&format!("cmp {}, r10", reg));              // compare the materialized argument against its PHP maximum
            ctx.emitter.instruction(&format!("jle {}", ok_label));              // an argument at or below the maximum is in range
        }
        (Arch::AArch64, ValueGuard::SignedMagnitudeAtMost(reg, maximum)) => {
            let fail_label = ctx.next_label("value_guard_fail");
            ctx.emitter.instruction(&format!("mov x9, #{}", maximum));          // materialize the largest magnitude PHP accepts for this argument
            ctx.emitter.instruction(&format!("cmp {}, x9", reg));               // compare the materialized argument against the positive bound
            ctx.emitter.instruction(&format!("b.gt {}", fail_label));           // a value above the bound is out of range
            ctx.emitter.instruction(&format!("cmn {}, x9", reg));               // compare the materialized argument against the negated bound
            ctx.emitter.instruction(&format!("b.ge {}", ok_label));             // a value at or above the negated bound is in range
            ctx.emitter.label(&fail_label);
        }
        (Arch::X86_64, ValueGuard::SignedMagnitudeAtMost(reg, maximum)) => {
            let fail_label = ctx.next_label("value_guard_fail");
            ctx.emitter.instruction(&format!("cmp {}, {}", reg, maximum));      // compare the materialized argument against the positive bound
            ctx.emitter.instruction(&format!("jg {}", fail_label));             // a value above the bound is out of range
            ctx.emitter.instruction(&format!("cmp {}, -{}", reg, maximum));     // compare the materialized argument against the negated bound
            ctx.emitter.instruction(&format!("jge {}", ok_label));              // a value at or above the negated bound is in range
            ctx.emitter.label(&fail_label);
        }
        (Arch::AArch64, ValueGuard::SignedInRange(reg, minimum, maximum)) => {
            let fail_label = ctx.next_label("value_guard_fail");
            ctx.emitter.instruction(&format!("cmp {}, #{}", reg, minimum));     // compare the materialized argument against the inclusive lower bound
            ctx.emitter.instruction(&format!("b.lt {}", fail_label));           // a value below the range is rejected
            ctx.emitter.instruction(&format!("cmp {}, #{}", reg, maximum));     // compare the materialized argument against the inclusive upper bound
            ctx.emitter.instruction(&format!("b.le {}", ok_label));             // a value at or below the upper bound is in range
            ctx.emitter.label(&fail_label);
        }
        (Arch::X86_64, ValueGuard::SignedInRange(reg, minimum, maximum)) => {
            let fail_label = ctx.next_label("value_guard_fail");
            ctx.emitter.instruction(&format!("cmp {}, {}", reg, minimum));      // compare the materialized argument against the inclusive lower bound
            ctx.emitter.instruction(&format!("jl {}", fail_label));             // a value below the range is rejected
            ctx.emitter.instruction(&format!("cmp {}, {}", reg, maximum));      // compare the materialized argument against the inclusive upper bound
            ctx.emitter.instruction(&format!("jle {}", ok_label));              // a value at or below the upper bound is in range
            ctx.emitter.label(&fail_label);
        }
        (Arch::AArch64, ValueGuard::NotEqualToImmediate(reg, forbidden)) => {
            ctx.emitter.instruction(&format!("cmp {}, #{}", reg, forbidden));   // compare the materialized argument against the value PHP forbids
            ctx.emitter.instruction(&format!("b.ne {}", ok_label));             // any other value is accepted
        }
        (Arch::X86_64, ValueGuard::NotEqualToImmediate(reg, forbidden)) => {
            ctx.emitter.instruction(&format!("cmp {}, {}", reg, forbidden));    // compare the materialized argument against the value PHP forbids
            ctx.emitter.instruction(&format!("jne {}", ok_label));              // any other value is accepted
        }
        (Arch::AArch64, ValueGuard::NonNegativeUnlessSignedBelow(reg, low, high)) => {
            ctx.emitter.instruction(&format!("cmp {}, {}", low, high));         // is the interval decreasing or degenerate?
            ctx.emitter.instruction(&format!("b.ge {}", ok_label));             // a decreasing interval accepts either step sign
            ctx.emitter.instruction(&format!("cmp {}, #0", reg));               // an increasing interval needs a positive step
            ctx.emitter.instruction(&format!("b.gt {}", ok_label));             // a strictly positive step is in range
        }
        (Arch::X86_64, ValueGuard::NonNegativeUnlessSignedBelow(reg, low, high)) => {
            ctx.emitter.instruction(&format!("cmp {}, {}", low, high));         // is the interval decreasing or degenerate?
            ctx.emitter.instruction(&format!("jge {}", ok_label));              // a decreasing interval accepts either step sign
            ctx.emitter.instruction(&format!("cmp {}, 0", reg));                // an increasing interval needs a positive step
            ctx.emitter.instruction(&format!("jg {}", ok_label));               // a strictly positive step is in range
        }
        (Arch::AArch64, ValueGuard::MagnitudeWithinSpan(reg, low, high)) => {
            ctx.emitter.instruction(&format!("cmp {}, {}", low, high));         // is the interval degenerate?
            ctx.emitter.instruction(&format!("b.eq {}", ok_label));             // a single-point interval accepts any step magnitude
            ctx.emitter.instruction(&format!("csel x9, {}, {}, le", low, high));// x9 = the smaller of the two endpoints
            ctx.emitter.instruction(
                &format!("csel x10, {}, {}, le", high, low)
            );                                                                  // x10 = the larger of the two endpoints
            ctx.emitter.instruction("sub x9, x10, x9");                         // x9 = high - low, the spanned interval as an unsigned width
            ctx.emitter.instruction(&format!("cmp {}, #0", reg));               // is the guarded argument negative?
            ctx.emitter.instruction(&format!("cneg x10, {}, lt", reg));         // x10 = |argument|, its unsigned magnitude
            ctx.emitter.instruction("cmp x10, x9");                             // compare the argument magnitude against the spanned width
            ctx.emitter.instruction(&format!("b.ls {}", ok_label));             // an unsigned magnitude within the span is in range
        }
        (Arch::X86_64, ValueGuard::MagnitudeWithinSpan(reg, low, high)) => {
            ctx.emitter.instruction(&format!("cmp {}, {}", low, high));         // is the interval degenerate?
            ctx.emitter.instruction(&format!("je {}", ok_label));               // a single-point interval accepts any step magnitude
            ctx.emitter.instruction(&format!("mov r10, {}", low));              // stage the first endpoint before ordering the pair
            ctx.emitter.instruction(&format!("mov r11, {}", high));             // stage the second endpoint before ordering the pair
            ctx.emitter.instruction(&format!("cmovg r10, {}", high));           // r10 = the smaller of the two endpoints
            ctx.emitter.instruction(&format!("cmovg r11, {}", low));            // r11 = the larger of the two endpoints
            ctx.emitter.instruction("sub r11, r10");                            // r11 = high - low, the spanned interval as an unsigned width
            ctx.emitter.instruction(&format!("mov r10, {}", reg));              // stage the guarded argument before normalizing its magnitude
            ctx.emitter.instruction("neg r10");                                 // negate the guarded argument so a negative one yields its magnitude
            ctx.emitter.instruction(&format!("test {}, {}", reg, reg));         // is the guarded argument negative?
            ctx.emitter.instruction(&format!("cmovns r10, {}", reg));           // r10 = |argument|, its unsigned magnitude
            ctx.emitter.instruction("cmp r10, r11");                            // compare the argument magnitude against the spanned width
            ctx.emitter.instruction(&format!("jbe {}", ok_label));              // an unsigned magnitude within the span is in range
        }
    }
    emit_value_error(ctx, message);
    ctx.emitter.label(&ok_label);
}

/// Throws a catchable PHP `DivisionByZeroError` carrying a static message.
///
/// Reference PHP raises this `ArithmeticError` subclass — not a bare fatal — for a
/// zero divisor, so `catch (DivisionByZeroError $e)`, `catch (ArithmeticError $e)`,
/// `catch (Error $e)`, and `catch (Throwable $e)` all match. Callers pass php-src's
/// own wording (`"Division by zero"` / `"Modulo by zero"`).
pub(super) fn emit_division_by_zero_error(ctx: &mut FunctionContext<'_>, message: &str) {
    emit_static_exception(
        ctx,
        "DivisionByZeroError",
        "_spl_division_by_zero_error_class_id",
        message,
    );
}

/// Throws a catchable PHP `ArithmeticError` carrying a static message.
///
/// Reference PHP raises this for arithmetic that has no representable result but is not a
/// division by zero — currently `<<`/`>>` with a negative shift count
/// (`ArithmeticError: Bit shift by negative number`). `catch (ArithmeticError $e)`,
/// `catch (Error $e)`, and `catch (Throwable $e)` all match; `DivisionByZeroError` does not.
pub(super) fn emit_arithmetic_error(ctx: &mut FunctionContext<'_>, message: &str) {
    emit_static_exception(
        ctx,
        "ArithmeticError",
        "_spl_arithmetic_error_class_id",
        message,
    );
}

/// Throws a catchable PHP `ArgumentCountError` carrying a static message.
///
/// Reference PHP raises this `TypeError` subclass when a call passes fewer arguments than the
/// callee requires, so `catch (ArgumentCountError $e)`, `catch (TypeError $e)`, `catch (Error $e)`
/// and `catch (Throwable $e)` all match.
///
/// THE MESSAGE IS STATIC EVEN THOUGH THE CLASS IS NOT. `new $c(...)` picks its class at run time,
/// but each ladder arm is emitted for ONE class, and the argument count is the site's, so the
/// wording is fully known while lowering that arm. Callers use php-src's two shapes — see
/// `objects::dynamic_mixed_candidates::dynamic_new_mixed_arity_refusals`.
pub(super) fn emit_argument_count_error(
    ctx: &mut FunctionContext<'_>,
    message: &str,
    location: Option<(String, u32)>,
) {
    emit_static_exception_at(
        ctx,
        "ArgumentCountError",
        "_spl_argument_count_error_class_id",
        message,
        location,
    );
}

/// Throws a catchable PHP `TypeError` carrying a static message and a source location.
///
/// The located sibling of `emit_type_error`, for the same reason
/// `emit_argument_count_error` takes one: a `new $c(...)` refusal belongs to a `new` expression the
/// user wrote, so the report ends in ` in FILE:LINE` and `getLine()` answers when it is caught.
pub(super) fn emit_type_error_at(
    ctx: &mut FunctionContext<'_>,
    message: &str,
    location: Option<(String, u32)>,
) {
    emit_static_exception_at(ctx, "TypeError", "_spl_type_error_class_id", message, location);
}

/// Throws a catchable PHP `Error` whose message is a runtime string value.
pub(super) fn emit_error_value(ctx: &mut FunctionContext<'_>, message: ValueId) -> Result<()> {
    let (message_ptr_reg, message_len_reg) = abi::string_result_regs(ctx.emitter);
    ctx.load_string_value_to_regs(message, message_ptr_reg, message_len_reg)?;
    abi::emit_push_reg_pair(ctx.emitter, message_ptr_reg, message_len_reg);
    emit_uncaught_dynamic_throwable_fatal_if_no_handler(ctx, "Error", None);
    emit_dynamic_throwable_object(ctx, "_spl_error_class_id");
    Ok(())
}

/// Throws a catchable PHP `TypeError` whose message already sits in the string-result registers.
///
/// The static `emit_type_error()` covers the guards whose wording is fixed. The ones that reach
/// here name the offending VALUE, which is only known at run time:
///
/// - `count()` prints the value's own spelling — `false`, not `bool` — picked from a table.
/// - `count()` and `stream_context_create()` both name the offending CLASS when the value arrives
///   as a boxed `Mixed`, composed with `emit_message_concat_prefix`/`_suffix` below.
///
/// Emitting each wording as a static throw instead would inline a whole throwable construction
/// per wording at every call site.
pub(super) fn emit_type_error_from_string_result(ctx: &mut FunctionContext<'_>) {
    emit_type_error_from_string_result_at(ctx, None);
}

/// The located sibling: the report ends in ` in FILE:LINE`, carries php's frame #0 and the trace
/// block, and a caught instance answers `getLine()`.
///
/// The message is built at run time; the LOCATION is not, so it arrives the way it does for the
/// static thrower — from the instruction's own span. Without it the report was one line where php
/// writes five.
pub(super) fn emit_type_error_from_string_result_at(
    ctx: &mut FunctionContext<'_>,
    location: Option<(String, u32)>,
) {
    let (message_ptr_reg, message_len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_push_reg_pair(ctx.emitter, message_ptr_reg, message_len_reg);
    emit_uncaught_dynamic_throwable_fatal_if_no_handler(ctx, "TypeError", location.as_ref());
    emit_dynamic_throwable_object(ctx, "_spl_type_error_class_id");
}

/// Throws a catchable PHP `ValueError` whose message already sits in the string-result registers.
///
/// The static `emit_value_error()` covers the builtin guards whose wording is fixed. A few of
/// php-src's own `ValueError`s interpolate the offending values instead — `range()`'s
/// `"The supplied range exceeds the maximum array size: start=… end=… step=…"` is the one that
/// reaches here — so the caller builds the exact message at runtime and hands it over as a
/// persisted pointer/length pair. The uncaught diagnostic names `ValueError` just like the static
/// path, so an unhandled oversized range still reports PHP's error class rather than the
/// unwinder's generic fallback.
pub(super) fn emit_value_error_from_string_result(ctx: &mut FunctionContext<'_>) {
    let (message_ptr_reg, message_len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_push_reg_pair(ctx.emitter, message_ptr_reg, message_len_reg);
    emit_uncaught_dynamic_throwable_fatal_if_no_handler(ctx, "ValueError", None);
    emit_dynamic_throwable_object(ctx, "_spl_value_error_class_id");
}


/// Prepends a static fragment to the message held in the string-result registers.
///
/// php's wordings that name a class are all the same shape — a fixed prefix, the class name, a
/// fixed suffix — and the class name is only known at run time. The caller loads the name into the
/// string-result pair, wraps it with these two, persists it, and hands it to one of the
/// `*_from_string_result` throwers above.
pub(super) fn emit_message_concat_prefix(ctx: &mut FunctionContext<'_>, prefix: &str) {
    let (text_ptr, text_len) = abi::string_result_regs(ctx.emitter);
    let (right_ptr, right_len) = message_concat_right_operand_regs(ctx);
    let (prefix_label, prefix_len) = ctx.data.add_string(prefix.as_bytes());
    ctx.emitter
        .instruction(&format!("mov {}, {}", right_ptr, text_ptr));              // move the built text into the concat right operand
    ctx.emitter
        .instruction(&format!("mov {}, {}", right_len, text_len));              // move its length into the concat right operand
    abi::emit_symbol_address(ctx.emitter, text_ptr, &prefix_label);
    abi::emit_load_int_immediate(ctx.emitter, text_len, prefix_len as i64);
    abi::emit_call_label(ctx.emitter, "__rt_concat");
}

/// Appends a static fragment to the message held in the string-result registers.
pub(super) fn emit_message_concat_suffix(ctx: &mut FunctionContext<'_>, suffix: &str) {
    let (right_ptr, right_len) = message_concat_right_operand_regs(ctx);
    let (suffix_label, suffix_len) = ctx.data.add_string(suffix.as_bytes());
    abi::emit_symbol_address(ctx.emitter, right_ptr, &suffix_label);
    abi::emit_load_int_immediate(ctx.emitter, right_len, suffix_len as i64);
    abi::emit_call_label(ctx.emitter, "__rt_concat");
}

/// The register pair `__rt_concat` reads its right operand from.
///
/// It is NOT the pair it returns in, and the two differ per target, so both are spelled out.
fn message_concat_right_operand_regs(ctx: &FunctionContext<'_>) -> (&'static str, &'static str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => ("x3", "x4"),
        Arch::X86_64 => ("rdi", "rsi"),
    }
}

/// Allocates one built-in throwable and transfers control to the standard unwinder.
fn emit_static_exception(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    class_id_symbol: &str,
    message: &str,
) {
    // php names the CALL SITE for a builtin's TypeError/ValueError, so the instruction being
    // lowered is the location — see `FunctionContext::current_source_location`. These used to pass
    // `None` and the uncaught report came out with no ` in FILE:LINE` at all, while every warning
    // beside it carried one.
    let location = ctx.current_source_location();
    emit_static_exception_at(ctx, class_name, class_id_symbol, message, location);
}

/// Same, for an emitter that KNOWS the source location the throwable belongs to.
///
/// Most codegen-raised throwables have no user `new` behind them — an `ArithmeticError` from a
/// division, a `TypeError` from an argument check — so they carry no location, and PHP would name
/// the internal call site anyway. `new $c(...)` is different: the refusal belongs to a `new`
/// expression the user WROTE, and php reports it, so a caller holding the span passes it and gets
/// both halves php prints — the ` in FILE:LINE` suffix on the uncaught report, and a `getLine()`
/// that answers when the error is CAUGHT.
///
/// A `None` location keeps the previous bytes exactly: the creation-line slot is still cleared to
/// zero, which is what `sentinels::emit_throwable_creation_line_unknown` wrote there.
fn emit_static_exception_at(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    class_id_symbol: &str,
    message: &str,
    location: Option<(String, u32)>,
) {
    let suffix = match &location {
        Some((file, line)) => format!(" in {}:{}", file, line),
        None => String::new(),
    };
    let creation_line = location.as_ref().map_or(0, |(_, line)| *line);
    let fatal_message = format!(
        "\nFatal error: Uncaught {}: {}{}\n",
        class_name, message, suffix
    );
    let (fatal_label, fatal_len) = ctx.data.add_string(fatal_message.as_bytes());
    emit_uncaught_exception_fatal_if_no_handler(ctx, &fatal_label, fatal_len, creation_line);

    let (message_label, message_len) = ctx.data.add_string(message.as_bytes());
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", 56); // compact Throwable: message/code/previous
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction("mov x9, #6");                              // heap kind 6 = throwable object instance
            ctx.emitter.instruction("str x9, [x0, #-8]");                       // stamp the allocation as a runtime object
            ctx.emitter.instruction("bl __rt_object_handle_acquire");           // bind the new object to its PHP object handle
            abi::emit_load_symbol_to_reg(ctx.emitter, "x9", class_id_symbol, 0);
            ctx.emitter.instruction("str x9, [x0]");                            // store the built-in throwable class id
            abi::emit_symbol_address(ctx.emitter, "x9", &message_label);
            ctx.emitter.instruction("str x9, [x0, #8]");                        // store the static exception message pointer
            abi::emit_load_int_immediate(ctx.emitter, "x9", message_len as i64);
            ctx.emitter.instruction("str x9, [x0, #16]");                       // store the exception message length
            ctx.emitter.instruction("str xzr, [x0, #24]");                      // exception code defaults to zero
            super::objects::throwable_new::emit_throwable_creation_line_aarch64(
                ctx,
                "x0",
                "x9",
                creation_line,
            );
            ctx.emitter.instruction("str xzr, [x0, #40]");                      // previous defaults to null
            abi::emit_store_reg_to_symbol(ctx.emitter, "x0", "_exc_value", 0);
            abi::emit_jump(ctx.emitter, "__rt_throw_current");
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rax", 56); // compact Throwable: message/code/previous
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction(
                &format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(6))
            );                                                                  // stamp the canonical x86_64 heap-kind word (magic + kind 6 throwable)
            ctx.emitter.instruction("mov QWORD PTR [rax - 8], r10");            // stamp the allocation as a runtime object
            ctx.emitter.instruction("call __rt_object_handle_acquire");         // bind the new object to its PHP object handle
            abi::emit_load_symbol_to_reg(ctx.emitter, "r10", class_id_symbol, 0);
            ctx.emitter.instruction("mov QWORD PTR [rax], r10");                // store the built-in throwable class id
            abi::emit_symbol_address(ctx.emitter, "r10", &message_label);
            ctx.emitter.instruction("mov QWORD PTR [rax + 8], r10");            // store the static exception message pointer
            abi::emit_load_int_immediate(ctx.emitter, "r10", message_len as i64);
            ctx.emitter.instruction("mov QWORD PTR [rax + 16], r10");           // store the exception message length
            ctx.emitter.instruction("mov QWORD PTR [rax + 24], 0");             // exception code defaults to zero
            super::objects::throwable_new::emit_throwable_creation_line_x86_64(
                ctx,
                "rax",
                creation_line,
            );
            ctx.emitter.instruction("mov QWORD PTR [rax + 40], 0");             // previous defaults to null
            abi::emit_store_reg_to_symbol(ctx.emitter, "rax", "_exc_value", 0);
            abi::emit_jump(ctx.emitter, "__rt_throw_current");
        }
    }
}

/// Writes the specific uncaught diagnostic and exits when no catch handler is active.
fn emit_uncaught_exception_fatal_if_no_handler(
    ctx: &mut FunctionContext<'_>,
    fatal_label: &str,
    fatal_len: usize,
    creation_line: u32,
) {
    let throw_label = ctx.next_label("static_exception_throw");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_symbol_to_reg(ctx.emitter, "x9", "_exc_handler_top", 0);
            ctx.emitter.instruction(&format!("cbnz x9, {}", throw_label));      // use the standard unwinder when a catch handler is active
            // Drain buffered output first: PHP emits it before the report, and this path used to
            // exit without flushing, discarding everything a program had buffered. `bl` leaves sp
            // untouched, so the message slots read below are unaffected.
            ctx.emitter.instruction("bl __rt_ob_flush_all");                    // drain buffered output before the fatal report
            emit_raising_builtin_trace_frame(ctx);                              // php's frame #0 is the builtin that raised
            abi::emit_symbol_address(ctx.emitter, "x1", fatal_label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", fatal_len as i64);
            ctx.emitter.instruction("mov x0, #1");                              // fd = stdout, where PHP writes this report
            ctx.emitter.syscall(4);
            abi::emit_load_int_immediate(ctx.emitter, "x0", i64::from(creation_line));
            // A builtin called directly from `main` has a COMPLETE chain — `#0 <builtin>` then
            // `#1 {main}` — whatever else the module declares. Inside a user function the chain
            // would need that function's frame and its callers, which nothing here can walk, so
            // the module flag decides as before.
            abi::emit_load_int_immediate(ctx.emitter, "x1", i64::from(ctx.is_main));
            ctx.emitter.instruction("bl __rt_trace_write_block");               // this path never reaches the shared report
            emit_uncaught_exit(ctx);
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(ctx.emitter, "r10", "_exc_handler_top", 0);
            ctx.emitter.instruction("test r10, r10");                           // check whether a catch handler is active
            ctx.emitter.instruction(&format!("jnz {}", throw_label));           // use the standard unwinder when a handler can receive the error
            // See the ARM64 path. rsp is SAVED and restored rather than simply aligned: the
            // dynamic form reads its message from temporary stack slots, which `and rsp, -16`
            // alone would move out from under it. r15 is callee-saved and the flush helper
            // touches no callee-saved register.
            ctx.emitter.instruction("mov r15, rsp");                            // stash the entry rsp in r15, which the flush helper preserves
            ctx.emitter.instruction("and rsp, -16");                            // realign the stack to the 16-byte call boundary
            ctx.emitter.instruction("call __rt_ob_flush_all");                  // drain buffered output before the fatal report
            // The trace calls stay INSIDE the aligned window. `mov rsp, r15` restores the entry
            // rsp, which at a call boundary is 8 mod 16 — the very thing `and rsp, -16` exists to
            // fix — so calling from there would hand every callee a misaligned stack. Nothing
            // below reads an rsp-relative temporary: this arm's message is a pre-baked label, and
            // the frame recording reads rbp-relative locals.
            emit_raising_builtin_trace_frame(ctx);                              // php's frame #0 is the builtin that raised
            abi::emit_symbol_address(ctx.emitter, "rsi", fatal_label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", fatal_len as i64);
            ctx.emitter.instruction("mov edi, 1");                              // fd = stdout, where PHP writes this report
            ctx.emitter.instruction("mov eax, 1");                              // Linux x86_64 syscall 1 = write
            ctx.emitter.instruction("syscall");                                 // emit the specific fatal message
            abi::emit_load_int_immediate(ctx.emitter, "rdi", i64::from(creation_line));
            // See the AArch64 arm: `main` is the one frame this lowering can prove complete.
            abi::emit_load_int_immediate(ctx.emitter, "rsi", i64::from(ctx.is_main));
            ctx.emitter.instruction("call __rt_trace_write_block");             // this path never reaches the shared report
            ctx.emitter.instruction("mov rsp, r15");                            // restore the entry rsp before leaving
            emit_uncaught_exit(ctx);
        }
    }
    ctx.emitter.label(&throw_label);
}

/// Writes an uncaught dynamic throwable diagnostic, or continues when a handler exists.
///
/// `class_name` names the PHP class in the fatal line (`Error`, `ValueError`, …); the message
/// itself is read from the 16-byte temporary the caller pushed, so this works for any throwable
/// whose text is only known at runtime.
fn emit_uncaught_dynamic_throwable_fatal_if_no_handler(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    location: Option<&(String, u32)>,
) {
    let throw_label = ctx.next_label("dynamic_error_throw");
    let prefix = format!("\nFatal error: Uncaught {}: ", class_name);
    let (prefix_label, prefix_len) = ctx.data.add_string(prefix.as_bytes());
    // php ends the first line with the location, then writes the trace block. The suffix carries
    // the location because the MESSAGE between them is a run-time pointer, so the two constants
    // cannot be one string.
    let suffix = match location {
        Some((file, line)) => format!(" in {}:{}\n", file, line),
        None => "\n".to_string(),
    };
    let (suffix_label, suffix_len) = ctx.data.add_string(suffix.as_bytes());
    let creation_line = location.map_or(0, |(_, line)| *line);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_symbol_to_reg(ctx.emitter, "x9", "_exc_handler_top", 0);
            ctx.emitter.instruction(&format!("cbnz x9, {}", throw_label));      // use the standard unwinder when a catch handler is active
            // Drain buffered output first: PHP emits it before the report, and this path used to
            // exit without flushing, discarding everything a program had buffered. `bl` leaves sp
            // untouched, so the message slots read below are unaffected.
            ctx.emitter.instruction("bl __rt_ob_flush_all");                    // drain buffered output before the fatal report
            if location.is_some() {
                emit_raising_builtin_trace_frame(ctx);                          // php's frame #0 is the builtin that raised
            }
            ctx.emitter.instruction("mov x0, #1");                              // fd = stdout for the dynamic-error prefix
            abi::emit_symbol_address(ctx.emitter, "x1", &prefix_label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", prefix_len as i64);
            ctx.emitter.syscall(4);
            ctx.emitter.instruction("mov x0, #1");                              // fd = stdout for the runtime error message
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", 0);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x2", 8);
            ctx.emitter.syscall(4);
            ctx.emitter.instruction("mov x0, #1");                              // fd = stdout to terminate the diagnostic
            abi::emit_symbol_address(ctx.emitter, "x1", &suffix_label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", suffix_len as i64);
            ctx.emitter.syscall(4);
            if location.is_some() {
                abi::emit_load_int_immediate(ctx.emitter, "x0", i64::from(creation_line));
                // See the static reporter: a builtin called directly from `main` has a complete
                // chain whatever else the module declares.
                abi::emit_load_int_immediate(ctx.emitter, "x1", i64::from(ctx.is_main));
                ctx.emitter.instruction("bl __rt_trace_write_block");
            }
            emit_uncaught_exit(ctx);
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(ctx.emitter, "r10", "_exc_handler_top", 0);
            ctx.emitter.instruction("test r10, r10");                           // check whether a catch handler is active
            ctx.emitter.instruction(&format!("jnz {}", throw_label));           // use the standard unwinder when a handler can receive the error
            // See the ARM64 path. rsp is SAVED and restored rather than simply aligned: the
            // dynamic form reads its message from temporary stack slots, which `and rsp, -16`
            // alone would move out from under it. r15 is callee-saved and the flush helper
            // touches no callee-saved register.
            ctx.emitter.instruction("mov r15, rsp");                            // stash the entry rsp in r15, which the flush helper preserves
            ctx.emitter.instruction("and rsp, -16");                            // realign the stack to the 16-byte call boundary
            ctx.emitter.instruction("call __rt_ob_flush_all");                  // drain buffered output before the fatal report
            ctx.emitter.instruction("mov rsp, r15");                            // restore the entry rsp so the message stack slots stay addressable
            if location.is_some() {
                emit_raising_builtin_trace_frame(ctx);                          // php's frame #0 is the builtin that raised
            }
            abi::emit_symbol_address(ctx.emitter, "rsi", &prefix_label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", prefix_len as i64);
            ctx.emitter.instruction("mov edi, 1");                              // fd = stdout for the dynamic-error prefix
            ctx.emitter.instruction("mov eax, 1");                              // Linux x86_64 syscall 1 = write
            ctx.emitter.instruction("syscall");                                 // emit the dynamic-error prefix
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", 0);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdx", 8);
            ctx.emitter.instruction("mov edi, 1");                              // fd = stdout for the runtime error message
            ctx.emitter.instruction("mov eax, 1");                              // Linux x86_64 syscall 1 = write
            ctx.emitter.instruction("syscall");                                 // emit the runtime error message
            abi::emit_symbol_address(ctx.emitter, "rsi", &suffix_label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", suffix_len as i64);
            ctx.emitter.instruction("mov edi, 1");                              // fd = stdout to terminate the diagnostic
            ctx.emitter.instruction("mov eax, 1");                              // Linux x86_64 syscall 1 = write
            ctx.emitter.instruction("syscall");                                 // emit the dynamic-error suffix
            if location.is_some() {
                abi::emit_load_int_immediate(ctx.emitter, "rdi", i64::from(creation_line));
                // See the static reporter: `main` gives the chain its completeness.
                abi::emit_load_int_immediate(ctx.emitter, "rsi", i64::from(ctx.is_main));
                ctx.emitter.instruction("call __rt_trace_write_block");
            }
            emit_uncaught_exit(ctx);
        }
    }
    ctx.emitter.label(&throw_label);
}

/// Publishes any active exact profile before an uncaught generated error exits.
fn emit_uncaught_exit(ctx: &mut FunctionContext<'_>) {
    crate::codegen::frame::emit_instr_terminate(ctx);
    abi::emit_exit(ctx.emitter, UNCAUGHT_EXIT_STATUS);
}

/// Allocates a built-in throwable that owns the runtime message stored on the stack.
///
/// `class_id_symbol` selects the built-in class the object reports (`_spl_error_class_id`,
/// `_spl_value_error_class_id`, …). The message pointer/length come from the 16-byte temporary
/// the caller pushed, which is released once both words have been copied into the object.
fn emit_dynamic_throwable_object(ctx: &mut FunctionContext<'_>, class_id_symbol: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", 56); // compact Throwable: message/code/previous
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction("mov x9, #6");                              // heap kind 6 = throwable object instance
            ctx.emitter.instruction("str x9, [x0, #-8]");                       // stamp the allocation as a runtime object
            ctx.emitter.instruction("bl __rt_object_handle_acquire");           // bind the new object to its PHP object handle
            abi::emit_load_symbol_to_reg(ctx.emitter, "x9", class_id_symbol, 0);
            ctx.emitter.instruction("str x9, [x0]");                            // store the built-in throwable class id
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", 0);
            ctx.emitter.instruction("str x9, [x0, #8]");                        // store the runtime exception message pointer
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", 8);
            ctx.emitter.instruction("str x9, [x0, #16]");                       // store the runtime exception message length
            ctx.emitter.instruction("str xzr, [x0, #24]");                      // exception code defaults to zero
            crate::codegen_support::sentinels::emit_throwable_creation_line_unknown(ctx.emitter, "x0");
            ctx.emitter.instruction("str xzr, [x0, #40]");                      // previous defaults to null
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            abi::emit_store_reg_to_symbol(ctx.emitter, "x0", "_exc_value", 0);
            abi::emit_jump(ctx.emitter, "__rt_throw_current");
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rax", 56); // compact Throwable: message/code/previous
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction(
                &format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(6))
            );                                                                  // stamp the canonical x86_64 heap-kind word (magic + kind 6 throwable)
            ctx.emitter.instruction("mov QWORD PTR [rax - 8], r10");            // stamp the allocation as a runtime object
            ctx.emitter.instruction("call __rt_object_handle_acquire");         // bind the new object to its PHP object handle
            abi::emit_load_symbol_to_reg(ctx.emitter, "r10", class_id_symbol, 0);
            ctx.emitter.instruction("mov QWORD PTR [rax], r10");                // store the built-in throwable class id
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", 0);
            ctx.emitter.instruction("mov QWORD PTR [rax + 8], r10");            // store the runtime exception message pointer
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", 8);
            ctx.emitter.instruction("mov QWORD PTR [rax + 16], r10");           // store the runtime exception message length
            ctx.emitter.instruction("mov QWORD PTR [rax + 24], 0");             // exception code defaults to zero
            crate::codegen_support::sentinels::emit_throwable_creation_line_unknown(ctx.emitter, "rax");
            ctx.emitter.instruction("mov QWORD PTR [rax + 40], 0");             // previous defaults to null
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            abi::emit_store_reg_to_symbol(ctx.emitter, "rax", "_exc_value", 0);
            abi::emit_jump(ctx.emitter, "__rt_throw_current");
        }
    }
}


/// Records the builtin whose call is raising as php's frame `#0`.
///
/// php names an internal function and its arguments — `#0 p.php(2): fopen('', 'r')` — and the
/// instruction being lowered IS that call, so its runtime target names it, its span gives the
/// call-site line, and its operands are the values. The buffer is reset first because php captures
/// a trace at CONSTRUCTION: a builtin exception that was caught earlier must not leave its frame
/// behind for this one.
///
/// Nothing is preserved across these calls. The sequence that follows throws.
fn emit_raising_builtin_trace_frame(ctx: &mut FunctionContext<'_>) {
    let Some((name, line, operands)) = ctx.current_builtin_frame() else {
        return;
    };
    let (name_label, name_len) = ctx.data.add_string(name.as_bytes());
    abi::emit_call_label(ctx.emitter, "__rt_trace_reset");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", i64::from(line));
            abi::emit_symbol_address(ctx.emitter, "x1", &name_label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", name_len as i64);
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rdi", i64::from(line));
            abi::emit_symbol_address(ctx.emitter, "rsi", &name_label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", name_len as i64);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_trace_frame_open");

    for operand in operands {
        if emit_trace_argument(ctx, operand).is_err() {
            // An operand whose shape has no php rendering leaves the argument list SHORT — the
            // frame still closes, so the trace reads `fn('a')` where php would have written
            // `fn('a', <something>)`. That is a smaller lie than a value invented for the shape,
            // and it is why `emit_trace_argument` refuses rather than guessing.
            break;
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_trace_frame_close");
}

/// Records php's frame for a call INTO a builtin class, at the call site.
///
/// `#0 p.php(13): SplFileInfo->getSize()` — php names the method and the line of the CALL, and
/// this is that call. The frame is recorded here rather than where the exception is built because
/// by then the arguments are gone: the synthesized body has its own parameters, not the operands
/// the caller passed.
///
/// Only from `main`, and that is the whole point rather than a limitation. A trace is printed only
/// when it is known COMPLETE, and `main` is the one frame this lowering can prove nothing hides:
/// one builtin-class frame, then `{main}`. Inside a user function the chain would need that
/// function's frame and its callers, which nothing here can walk — so the site publishes zero and
/// the report stays silent, exactly as before.
pub(super) fn emit_builtin_class_call_trace_frame(
    ctx: &mut FunctionContext<'_>,
    inst: &crate::ir::Instruction,
    frame_name: &str,
    arguments: &[crate::ir::ValueId],
) {
    let line = inst.span.map_or(0, |span| span.line);
    let (name_label, name_len) = ctx.data.add_string(frame_name.as_bytes());
    abi::emit_call_label(ctx.emitter, "__rt_trace_reset");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", i64::from(line));
            abi::emit_symbol_address(ctx.emitter, "x1", &name_label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", name_len as i64);
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rdi", i64::from(line));
            abi::emit_symbol_address(ctx.emitter, "rsi", &name_label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", name_len as i64);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_trace_frame_open");
    for operand in arguments {
        if emit_trace_argument(ctx, *operand).is_err() {
            // See `emit_raising_builtin_trace_frame`: a shape with no php rendering ends the
            // argument list rather than inventing a value for it.
            break;
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_trace_frame_close");
}

/// Renders one call argument into the open trace frame.
pub(super) fn emit_trace_argument(
    ctx: &mut FunctionContext<'_>,
    operand: crate::ir::ValueId,
) -> Result<()> {
    use crate::types::PhpType;

    // The runtime tags `__rt_trace_arg` branches on: 0 int, 1 string, 2 float, 3 bool, 4 indexed
    // array, 5 hash, 6 object, 8 null, 9 resource. A `Mixed` operand carries its own tag and is
    // unboxed instead of guessed at.
    //
    // Every shape is named. A blanket `_ => 9` sent arrays and objects through the RESOURCE
    // branch, which renders `Resource id #N` where php renders `Array` — and hands
    // `__rt_resource_id_of` a pointer that is not a resource handle.
    let tag = match ctx.value_php_type(operand)?.codegen_repr() {
        PhpType::Str => 1,
        PhpType::Float => 2,
        PhpType::Bool | PhpType::False => 3,
        PhpType::Void | PhpType::Never => 8,
        PhpType::Int => 0,
        PhpType::Array(_) => 4,
        PhpType::AssocArray { .. } => 5,
        PhpType::Object(_) => 6,
        PhpType::Resource(_) | PhpType::Pointer(_) => 9,
        PhpType::Mixed => -1,
        // Anything else has no rendering php would recognise. Refusing here abandons the rest of
        // the argument list (the caller breaks out of the loop) rather than printing a value
        // invented for the shape.
        _ => return Err(crate::codegen::CodegenIrError::unsupported("trace argument PHP type")),
    };
    ctx.load_value_to_result(operand)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            if tag < 0 {
                ctx.emitter.instruction("bl __rt_mixed_unbox");                 // x0/x1/x2 = tag/lo/hi already
            } else if tag == 1 {
                // A string already sits in the pair `__rt_trace_arg` reads: x1 pointer, x2 length.
                abi::emit_load_int_immediate(ctx.emitter, "x0", tag);
            } else {
                ctx.emitter.instruction("mov x1, x0");                          // a scalar arrives in the int result register
                ctx.emitter.instruction("mov x2, #0");                          // and carries no high word
                abi::emit_load_int_immediate(ctx.emitter, "x0", tag);
            }
        }
        Arch::X86_64 => {
            if tag < 0 {
                ctx.emitter.instruction("call __rt_mixed_unbox");               // rax/rdi/rdx = tag/lo/hi
                ctx.emitter.instruction("mov rsi, rdi");
                ctx.emitter.instruction("mov rdi, rax");
            } else if tag == 1 {
                ctx.emitter.instruction("mov rsi, rax");                        // pointer; rdx already holds the length
                abi::emit_load_int_immediate(ctx.emitter, "rdi", tag);
            } else {
                ctx.emitter.instruction("mov rsi, rax");                        // a scalar arrives in the int result register
                ctx.emitter.instruction("xor edx, edx");                        // and carries no high word
                abi::emit_load_int_immediate(ctx.emitter, "rdi", tag);
            }
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_trace_arg");
    Ok(())
}
