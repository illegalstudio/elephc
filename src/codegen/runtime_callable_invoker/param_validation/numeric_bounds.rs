//! Purpose:
//! Accepts exact signed integer numeric strings without losing PHP_INT_MAX to binary64 rounding.
//!
//! Called from:
//! - `super::accept_type()` before the float-form numeric-string/range check.
//!
//! Key details:
//! - Reads borrowed pointer/length slots at offsets 8/16; no allocation or libc calls.
//! - Magnitudes are accumulated only after checking the signed-64-bit decimal bound.
//! - Other spellings fall through to the shared PHP numeric scanner, which handles dots/exponents.

use super::*;

/// Accepts in-range decimal integer spellings directly, preserving exact signed-i64 boundary values.
pub(super) fn accept_exact_integer_spelling(
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    valid: &str,
) {
    let (ptr, len, magnitude, threshold, last_digit, byte, byte32) = match emitter.target.arch {
        Arch::AArch64 => ("x3", "x4", "x5", "x6", "x7", "x8", "w8"),
        Arch::X86_64 => ("r8", "r9", "r10", "r11", "rcx", "rdx", "edx"),
    };
    let start = ctx.next_label("parameter_integer_space");
    let sign = ctx.next_label("parameter_integer_sign");
    let negative = ctx.next_label("parameter_integer_negative");
    let advance_sign = ctx.next_label("parameter_integer_advance_sign");
    let first = ctx.next_label("parameter_integer_first_digit");
    let digit = ctx.next_label("parameter_integer_digit");
    let bounded = ctx.next_label("parameter_integer_bounded");
    let accumulate = ctx.next_label("parameter_integer_accumulate");
    let trailing = ctx.next_label("parameter_integer_trailing_space");
    let next = ctx.next_label("parameter_integer_not_exact");
    abi::emit_load_temporary_stack_slot(emitter, ptr, 8);
    abi::emit_load_temporary_stack_slot(emitter, len, 16);
    abi::emit_load_int_immediate(emitter, magnitude, 0);
    abi::emit_load_int_immediate(emitter, threshold, 922337203685477580);
    abi::emit_load_int_immediate(emitter, last_digit, 7);
    emitter.label(&start);
    compare_zero(emitter, len, &next);
    load_byte(emitter, byte32, ptr);
    compare_immediate(emitter, byte, 32);
    conditional(emitter, "eq", &format!("{start}_advance"));
    compare_immediate(emitter, byte, 9);
    conditional(emitter, "lo", &sign);
    compare_immediate(emitter, byte, 13);
    conditional(emitter, "hi", &sign);
    emitter.label(&format!("{start}_advance"));
    advance(emitter, ptr, len);
    abi::emit_jump(emitter, &start);
    emitter.label(&sign);
    compare_immediate(emitter, byte, 45);
    conditional(emitter, "eq", &negative);
    compare_immediate(emitter, byte, 43);
    conditional(emitter, "eq", &advance_sign);
    abi::emit_jump(emitter, &first);
    emitter.label(&negative);
    abi::emit_load_int_immediate(emitter, last_digit, 8);
    emitter.label(&advance_sign);
    advance(emitter, ptr, len);
    emitter.label(&first);
    compare_zero(emitter, len, &next);
    load_byte(emitter, byte32, ptr);
    compare_immediate(emitter, byte, 48);
    conditional(emitter, "lo", &next);
    compare_immediate(emitter, byte, 57);
    conditional(emitter, "hi", &next);
    emitter.label(&digit);
    emitter.instruction(&format!("cmp {magnitude}, {threshold}"));              // check before multiplying so decimal accumulation cannot overflow
    conditional(emitter, "lo", &bounded);
    conditional(emitter, "hi", &next);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("sub {byte}, {byte}, #48")), // decode the final decimal digit before checking the exact signed limit
        Arch::X86_64 => emitter.instruction(&format!("sub {byte}, 48")),        // decode the final decimal digit before checking the exact signed limit
    }
    emitter.instruction(&format!("cmp {byte}, {last_digit}"));                  // allow final digit 7 for positive and 8 for negative magnitudes
    conditional(emitter, "hi", &next);
    abi::emit_jump(emitter, &accumulate);
    emitter.label(&bounded);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("sub {byte}, {byte}, #48")), // decode the next bounded decimal digit
        Arch::X86_64 => emitter.instruction(&format!("sub {byte}, 48")),        // decode the next bounded decimal digit
    }
    emitter.label(&accumulate);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("add {magnitude}, {magnitude}, {magnitude}, lsl #2")); // multiply the bounded magnitude by five
            emitter.instruction(&format!("add {magnitude}, {byte}, {magnitude}, lsl #1")); // multiply by ten and add the next digit
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("imul {magnitude}, {magnitude}, 10")); // multiply the bounded magnitude by ten
            emitter.instruction(&format!("add {magnitude}, {byte}"));           // accumulate the next digit within the signed bound
        }
    }
    advance(emitter, ptr, len);
    compare_zero(emitter, len, valid);
    load_byte(emitter, byte32, ptr);
    compare_immediate(emitter, byte, 48);
    conditional(emitter, "lo", &trailing);
    compare_immediate(emitter, byte, 57);
    conditional(emitter, "hi", &trailing);
    abi::emit_jump(emitter, &digit);
    emitter.label(&trailing);
    compare_immediate(emitter, byte, 32);
    conditional(emitter, "eq", &format!("{trailing}_advance"));
    compare_immediate(emitter, byte, 9);
    conditional(emitter, "lo", &next);
    compare_immediate(emitter, byte, 13);
    conditional(emitter, "hi", &next);
    emitter.label(&format!("{trailing}_advance"));
    advance(emitter, ptr, len);
    compare_zero(emitter, len, valid);
    load_byte(emitter, byte32, ptr);
    abi::emit_jump(emitter, &trailing);
    // php-src promotes overflowing integer spellings to a double before testing the int range.
    // This deliberately permits spellings just below PHP_INT_MIN when binary64 rounds in range.
    emitter.label(&next);
}

/// Branches when no borrowed string bytes remain before the next lexical read.
fn compare_zero(emitter: &mut Emitter, register: &str, label: &str) {
    compare_immediate(emitter, register, 0);
    conditional(emitter, "eq", label);
}

/// Compares a scanned byte/count with a small immediate using the target's assembly spelling.
fn compare_immediate(emitter: &mut Emitter, register: &str, value: u8) {
    let immediate = if emitter.target.arch == Arch::AArch64 { format!("#{value}") } else { value.to_string() };
    emitter.instruction(&format!("cmp {register}, {immediate}"));               // compare one bounded lexical value without reading past the string
}

/// Maps decimal-scan equality/unsigned-bound conditions to the corresponding target branch.
fn conditional(emitter: &mut Emitter, condition: &str, label: &str) {
    let branch = match (emitter.target.arch, condition) {
        (Arch::AArch64, "eq") => "b.eq", (Arch::AArch64, "lo") => "b.lo",
        (Arch::AArch64, _) => "b.hi", (Arch::X86_64, "eq") => "je",
        (Arch::X86_64, "lo") => "jb", (Arch::X86_64, _) => "ja",
    };
    emitter.instruction(&format!("{branch} {label}"));                          // retain the correct unsigned decimal-bound comparison on each target
}

/// Reads one borrowed byte after the caller has checked the remaining length.
fn load_byte(emitter: &mut Emitter, byte: &str, pointer: &str) {
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("ldrb {byte}, [{pointer}]")), // read exactly one byte after checking the remaining length
        Arch::X86_64 => emitter.instruction(&format!("movzx {byte}, BYTE PTR [{pointer}]")), // read exactly one byte after checking the remaining length
    }
}

/// Advances the borrowed pointer and consumes one byte without allocating or calling libc.
fn advance(emitter: &mut Emitter, pointer: &str, remaining: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("add {pointer}, {pointer}, #1"));      // advance within the borrowed PHP string
            emitter.instruction(&format!("sub {remaining}, {remaining}, #1"));  // account for the consumed byte
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("add {pointer}, 1"));                  // advance within the borrowed PHP string
            emitter.instruction(&format!("sub {remaining}, 1"));                // account for the consumed byte
        }
    }
}
