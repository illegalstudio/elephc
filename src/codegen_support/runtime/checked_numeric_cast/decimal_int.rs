//! Purpose:
//! Emits exact decimal-to-int truncation for already validated full numeric strings.
//!
//! Called from:
//! - The checked numeric cast runtime after bounded PHP grammar validation.
//!
//! Key details:
//! - Only caller-saved registers are used, and every byte read stays inside the source range.
//! - A negative accumulator represents PHP_INT_MIN without overflowing an intermediate.

use super::{abi, ins, range_if, Arch, Emitter};

const PREFIX: &str = "__rt_checked_decimal_to_int";

/// Emits sign, decimal-position and exact-digit accumulation without a floating intermediate.
pub(super) fn emit(emitter: &mut Emitter) {
    emitter.label_global(PREFIX);
    abi::emit_frame_prologue(emitter, 48);
    ins(emitter, "mov x9, x0", "mov r8, rdi");
    ins(emitter, "mov x10, x1", "mov r9, rsi");
    ins(emitter, "mov x11, #0", "xor r10d, r10d");
    emitter.label(&label("leading"));
    read(emitter);
    ins(emitter, "cmp w15, #32", "cmp edx, 32"); branch_eq(emitter, "leading_next");
    ins(emitter, "sub w17, w15, #9", "lea eax, [rdx - 9]");
    ins(emitter, "cmp w17, #4", "cmp eax, 4");
    branch(emitter, "ls", "jbe", "leading_next"); jump(emitter, "sign");
    emitter.label(&label("leading_next")); advance(emitter); jump(emitter, "leading");
    emitter.label(&label("sign"));
    ins(emitter, "cmp w15, #45", "cmp edx, 45"); branch_ne(emitter, "plus");
    ins(emitter, "mov x11, #1", "mov r10, 1"); advance(emitter); jump(emitter, "mantissa");
    emitter.label(&label("plus"));
    ins(emitter, "cmp w15, #43", "cmp edx, 43"); branch_ne(emitter, "mantissa"); advance(emitter);
    emitter.label(&label("mantissa"));
    abi::emit_store_to_sp(emitter, if arm(emitter) { "x11" } else { "r10" }, 0);
    abi::emit_store_to_sp(emitter, if arm(emitter) { "x9" } else { "r8" }, 8);
    ins(emitter, "mov x12, #0", "xor r11d, r11d");
    ins(emitter, "mov x17, #0", "xor eax, eax");
    emitter.label(&label("scan"));
    empty(emitter, "no_exponent"); read(emitter);
    ins(emitter, "cmp w15, #46", "cmp edx, 46"); branch_ne(emitter, "scan_digit");
    ins(emitter, "mov x17, #1", "mov eax, 1"); advance(emitter); jump(emitter, "scan");
    emitter.label(&label("scan_digit"));
    ins(emitter, "cmp w15, #48", "cmp edx, 48"); branch(emitter, "lo", "jb", "scan_end");
    ins(emitter, "cmp w15, #57", "cmp edx, 57"); branch(emitter, "hi", "ja", "scan_end");
    ins(emitter, "cmp x17, #0", "cmp rax, 0"); branch_ne(emitter, "scan_next");
    ins(emitter, "add x12, x12, #1", "inc r11");
    emitter.label(&label("scan_next")); advance(emitter); jump(emitter, "scan");
    emitter.label(&label("scan_end"));
    ins(emitter, "orr w15, w15, #32", "or edx, 32");
    ins(emitter, "cmp w15, #101", "cmp edx, 101"); branch_ne(emitter, "no_exponent");
    abi::emit_store_to_sp(emitter, if arm(emitter) { "x9" } else { "r8" }, 16);
    advance(emitter); read(emitter);
    ins(emitter, "mov x16, #0", "xor edi, edi");
    ins(emitter, "cmp w15, #45", "cmp edx, 45"); branch_ne(emitter, "exponent_plus");
    ins(emitter, "mov x16, #1", "mov edi, 1"); advance(emitter); jump(emitter, "exponent_start");
    emitter.label(&label("exponent_plus"));
    ins(emitter, "cmp w15, #43", "cmp edx, 43"); branch_ne(emitter, "exponent_start"); advance(emitter);
    emitter.label(&label("exponent_start"));
    ins(emitter, "mov x14, #0", "xor ecx, ecx");
    abi::emit_load_int_immediate(emitter, if arm(emitter) { "x13" } else { "rsi" }, i64::MAX / 10);
    ins(emitter, "mov x17, #10", "mov eax, 10");
    emitter.label(&label("exponent_scan"));
    empty(emitter, "exponent_apply"); read(emitter);
    ins(emitter, "sub w15, w15, #48", "sub edx, 48");
    ins(emitter, "cmp w15, #9", "cmp edx, 9"); branch(emitter, "hi", "ja", "exponent_apply");
    ins(emitter, "cmp x14, x13", "cmp rcx, rsi"); branch(emitter, "gt", "jg", "exponent_saturate");
    ins(emitter, "mul x14, x14, x17", "imul rcx, rcx, 10");
    ins(emitter, "adds x14, x14, x15", "add rcx, rdx"); branch(emitter, "vs", "jo", "exponent_saturate");
    jump(emitter, "exponent_next");
    emitter.label(&label("exponent_saturate"));
    abi::emit_load_int_immediate(emitter, if arm(emitter) { "x14" } else { "rcx" }, i64::MAX);
    emitter.label(&label("exponent_next")); advance(emitter); jump(emitter, "exponent_scan");
    emitter.label(&label("exponent_apply"));
    ins(emitter, "cmp x16, #0", "cmp rdi, 0"); branch_ne(emitter, "negative_exponent");
    ins(emitter, "adds x12, x12, x14", "add r11, rcx"); branch(emitter, "vc", "jno", "accumulate");
    abi::emit_load_int_immediate(emitter, if arm(emitter) { "x12" } else { "r11" }, i64::MAX); jump(emitter, "accumulate");
    emitter.label(&label("negative_exponent"));
    ins(emitter, "sub x12, x12, x14", "sub r11, rcx"); jump(emitter, "accumulate");
    emitter.label(&label("no_exponent"));
    abi::emit_store_to_sp(emitter, if arm(emitter) { "x9" } else { "r8" }, 16);
    emitter.label(&label("accumulate"));
    ins(emitter, "mov x0, #0", "xor eax, eax");
    ins(emitter, "cmp x12, #0", "cmp r11, 0"); branch(emitter, "le", "jle", "done");
    abi::emit_load_temporary_stack_slot(emitter, if arm(emitter) { "x9" } else { "r8" }, 8);
    abi::emit_load_temporary_stack_slot(emitter, if arm(emitter) { "x10" } else { "r9" }, 16);
    abi::emit_load_int_immediate(emitter, if arm(emitter) { "x13" } else { "rsi" }, i64::MIN / 10);
    ins(emitter, "mov x17, #10", "mov ecx, 10");
    emitter.label(&label("digit"));
    ins(emitter, "cmp x12, #0", "cmp r11, 0"); branch_eq(emitter, "apply_sign");
    ins(emitter, "cmp x9, x10", "cmp r8, r9"); branch_eq(emitter, "padding"); read(emitter);
    ins(emitter, "add x9, x9, #1", "inc r8");
    ins(emitter, "cmp w15, #46", "cmp edx, 46"); branch_eq(emitter, "digit");
    ins(emitter, "sub w15, w15, #48", "sub edx, 48");
    threshold(emitter);
    ins(emitter, "cmp w15, #8", "cmp edx, 8"); branch(emitter, "ls", "jbe", "digit_append");
    ins(emitter, "cmp x0, x13", "cmp rax, rsi"); fail(emitter, "eq", "je");
    emitter.label(&label("digit_append"));
    ins(emitter, "mul x0, x0, x17", "imul rax, rax, 10");
    ins(emitter, "sub x0, x0, x15", "sub rax, rdx");
    ins(emitter, "sub x12, x12, #1", "dec r11"); jump(emitter, "digit");
    emitter.label(&label("padding"));
    ins(emitter, "cmp x0, #0", "cmp rax, 0"); branch_eq(emitter, "done");
    emitter.label(&label("pad_digit")); threshold(emitter);
    ins(emitter, "mul x0, x0, x17", "imul rax, rax, 10");
    ins(emitter, "subs x12, x12, #1", "dec r11"); branch_ne(emitter, "pad_digit");
    emitter.label(&label("apply_sign"));
    abi::emit_load_temporary_stack_slot(emitter, if arm(emitter) { "x11" } else { "r10" }, 0);
    ins(emitter, "cmp x11, #0", "cmp r10, 0"); branch_ne(emitter, "done");
    ins(emitter, "negs x0, x0", "neg rax"); fail(emitter, "vs", "jo");
    emitter.label(&label("done"));
    abi::emit_frame_restore(emitter, 48); abi::emit_return(emitter);
}

/// Selects the architecture without leaking register choices into semantic control flow.
fn arm(emitter: &Emitter) -> bool { emitter.target.arch == Arch::AArch64 }

/// Builds labels private to the exact decimal conversion graph.
fn label(suffix: &str) -> String { format!("{PREFIX}_{suffix}") }

/// Reads the bounded current byte after the caller or grammar proof established its presence.
fn read(emitter: &mut Emitter) { ins(emitter, "ldrb w15, [x9]", "movzx edx, BYTE PTR [r8]"); }

/// Advances one byte and retires it from the bounded scan count.
fn advance(emitter: &mut Emitter) {
    ins(emitter, "add x9, x9, #1", "inc r8"); ins(emitter, "sub x10, x10, #1", "dec r9");
}

/// Detects an exhausted byte range without touching memory outside it.
fn empty(emitter: &mut Emitter, suffix: &str) {
    ins(emitter, "cmp x10, #0", "cmp r9, 0"); branch_eq(emitter, suffix);
}

/// Jumps to a private graph block using the shared target-aware ABI helper.
fn jump(emitter: &mut Emitter, suffix: &str) { abi::emit_jump(emitter, &label(suffix)); }

/// Selects equivalent signed or unsigned conditional branches on each architecture.
fn branch(emitter: &mut Emitter, condition: &str, x86: &str, suffix: &str) {
    ins(emitter, &format!("b.{condition} {}", label(suffix)), &format!("{x86} {}", label(suffix)));
}

/// Branches on exact equality of the preceding comparison.
fn branch_eq(emitter: &mut Emitter, suffix: &str) { branch(emitter, "eq", "je", suffix); }

/// Branches on inequality without changing arithmetic flags.
fn branch_ne(emitter: &mut Emitter, suffix: &str) { branch(emitter, "ne", "jne", suffix); }

/// Routes a range failure through the shared catchable ValueError entry.
fn fail(emitter: &mut Emitter, condition: &str, x86: &str) {
    range_if(emitter, match condition { "eq" => "ne", "lt" => "ge", "vs" => "vc",
        _ => unreachable!("decimal conversion has only equality, lower-bound and overflow failures") }, x86);
}

/// Proves multiplication by ten can retain a signed negative integer accumulator.
fn threshold(emitter: &mut Emitter) {
    ins(emitter, "cmp x0, x13", "cmp rax, rsi"); fail(emitter, "lt", "jl");
}
