//! Purpose:
//! Emits complete checked integer radix parsing after explicit base selection.
//!
//! Called from:
//! - Checked intval string conversion for non-decimal requested bases.
//!
//! Key details:
//! - A bounded grammar pass precedes arithmetic, so malformed input cannot masquerade as overflow.
//! - A signed negative accumulator retains both endpoints without range saturation.

use super::{abi, ins, range_if, Arch, Emitter, BASE, STRING};

const PREFIX: &str = "__rt_checked_radix_to_int";

/// Parses pointer/length/base C arguments with exact digit, prefix and signed range contracts.
pub(super) fn emit(emitter: &mut Emitter) {
    emitter.label_global(PREFIX); abi::emit_frame_prologue(emitter, 48);
    abi::emit_store_to_sp(emitter, abi::int_arg_reg_name(emitter.target, 0), 16);
    abi::emit_store_to_sp(emitter, abi::int_arg_reg_name(emitter.target, 1), 24);
    ins(emitter, "mov x9, x0", "mov r8, rdi");
    ins(emitter, "mov x10, x1", "mov r9, rsi");
    ins(emitter, "mov x11, x2", "mov r10, rdx");
    ins(emitter, "cmp x11, #10", "cmp r10, 10"); branch(emitter, "eq", "je", "decimal");
    ins(emitter, "cmp x11, #0", "cmp r10, 0"); branch(emitter, "eq", "je", "leading_start");
    ins(emitter, "cmp x11, #2", "cmp r10, 2"); failure(emitter, "ge", "jl", BASE);
    ins(emitter, "cmp x11, #36", "cmp r10, 36"); failure(emitter, "le", "jg", BASE);
    emitter.label(&label("leading_start")); ins(emitter, "mov x12, #0", "xor r11d, r11d");
    emitter.label(&label("leading")); empty(emitter, "invalid"); read(emitter);
    whitespace(emitter, "leading_next", "sign");
    emitter.label(&label("leading_next")); advance(emitter); jump(emitter, "leading");
    emitter.label(&label("sign"));
    ins(emitter, "cmp w13, #45", "cmp edx, 45"); branch(emitter, "ne", "jne", "plus");
    ins(emitter, "mov x12, #1", "mov r11, 1"); advance(emitter); jump(emitter, "prefix");
    emitter.label(&label("plus"));
    ins(emitter, "cmp w13, #43", "cmp edx, 43"); branch(emitter, "ne", "jne", "prefix"); advance(emitter);
    emitter.label(&label("prefix"));
    ins(emitter, "cmp x10, #2", "cmp r9, 2"); branch(emitter, "lo", "jb", "auto"); read(emitter);
    ins(emitter, "cmp w13, #48", "cmp edx, 48"); branch(emitter, "ne", "jne", "auto");
    ins(emitter, "ldrb w13, [x9, #1]", "movzx edx, BYTE PTR [r8 + 1]");
    ins(emitter, "orr w13, w13, #32", "or edx, 32");
    ins(emitter, "cmp w13, #120", "cmp edx, 120"); branch(emitter, "ne", "jne", "binary_prefix");
    ins(emitter, "cmp x11, #0", "cmp r10, 0"); branch(emitter, "eq", "je", "hex");
    ins(emitter, "cmp x11, #16", "cmp r10, 16"); branch(emitter, "ne", "jne", "auto");
    emitter.label(&label("hex")); ins(emitter, "mov x11, #16", "mov r10, 16"); jump(emitter, "take_prefix");
    emitter.label(&label("binary_prefix"));
    ins(emitter, "cmp w13, #98", "cmp edx, 98"); branch(emitter, "ne", "jne", "auto");
    ins(emitter, "cmp x11, #0", "cmp r10, 0"); branch(emitter, "eq", "je", "binary");
    ins(emitter, "cmp x11, #2", "cmp r10, 2"); branch(emitter, "ne", "jne", "auto");
    emitter.label(&label("binary")); ins(emitter, "mov x11, #2", "mov r10, 2");
    emitter.label(&label("take_prefix")); advance(emitter); advance(emitter);
    emitter.label(&label("auto")); empty(emitter, "invalid");
    ins(emitter, "cmp x11, #0", "cmp r10, 0"); branch(emitter, "ne", "jne", "digits_start"); read(emitter);
    ins(emitter, "mov x11, #10", "mov r10, 10");
    ins(emitter, "cmp w13, #48", "cmp edx, 48"); branch(emitter, "ne", "jne", "digits_start");
    ins(emitter, "mov x11, #8", "mov r10, 8");
    emitter.label(&label("digits_start"));
    for (register, offset) in [(if arm(emitter) { "x12" } else { "r11" }, 0),
        (if arm(emitter) { "x11" } else { "r10" }, 8), (if arm(emitter) { "x9" } else { "r8" }, 16)] {
        abi::emit_store_to_sp(emitter, register, offset);
    }
    ins(emitter, "mov x0, #0", "xor eax, eax");
    emitter.label(&label("grammar")); empty(emitter, "grammar_done"); read(emitter);
    digit(emitter, "grammar_letter", "grammar_digit");
    emitter.label(&label("grammar_letter"));
    ins(emitter, "orr w14, w13, #32", "mov ecx, edx");
    if !arm(emitter) { emitter.instruction("or ecx, 32"); }                     // fold only ASCII letters before digit classification
    ins(emitter, "sub w14, w14, #97", "sub ecx, 97");
    ins(emitter, "cmp w14, #25", "cmp ecx, 25"); branch(emitter, "hi", "ja", "trailing");
    ins(emitter, "add w14, w14, #10", "add ecx, 10");
    emitter.label(&label("grammar_digit"));
    ins(emitter, "cmp x14, x11", "cmp rcx, r10"); branch(emitter, "hs", "jae", "invalid");
    ins(emitter, "mov x0, #1", "mov eax, 1"); advance(emitter); jump(emitter, "grammar");
    emitter.label(&label("trailing"));
    abi::emit_store_to_sp(emitter, if arm(emitter) { "x9" } else { "r8" }, 24);
    emitter.label(&label("trailing_loop")); empty(emitter, "grammar_valid"); read(emitter);
    whitespace(emitter, "trailing_next", "invalid");
    emitter.label(&label("trailing_next")); advance(emitter); jump(emitter, "trailing_loop");
    emitter.label(&label("grammar_done"));
    abi::emit_store_to_sp(emitter, if arm(emitter) { "x9" } else { "r8" }, 24);
    emitter.label(&label("grammar_valid"));
    ins(emitter, "cmp x0, #0", "cmp rax, 0"); branch(emitter, "eq", "je", "invalid");
    abi::emit_load_temporary_stack_slot(emitter, if arm(emitter) { "x9" } else { "r8" }, 16);
    abi::emit_load_temporary_stack_slot(emitter, if arm(emitter) { "x10" } else { "r9" }, 24);
    ins(emitter, "mov x0, #0", "xor eax, eax");
    emitter.label(&label("accumulate"));
    ins(emitter, "cmp x9, x10", "cmp r8, r9"); branch(emitter, "eq", "je", "apply_sign"); read(emitter);
    digit(emitter, "accumulate_letter", "accumulate_digit");
    emitter.label(&label("accumulate_letter"));
    ins(emitter, "orr w14, w13, #32", "mov ecx, edx");
    if !arm(emitter) { emitter.instruction("or ecx, 32"); }                     // preserve the grammar pass's ASCII digit value during accumulation
    ins(emitter, "sub w14, w14, #87", "sub ecx, 87");
    emitter.label(&label("accumulate_digit"));
    if arm(emitter) {
        emitter.instruction("smulh x15, x0, x11");                              // inspect the signed high product before replacing the negative accumulator
        emitter.instruction("mul x16, x0, x11");                                // retain the low product independently of its range proof
        emitter.instruction("cmp x15, x16, asr #63");                           // require the high word to equal the low word's sign extension
        range_if(emitter, "eq", "jo");
        emitter.instruction("mov x0, x16");                                     // publish the proven in-range product before subtracting its digit
    } else {
        emitter.instruction("imul rax, r10");                                   // multiply the exact negative accumulator by the selected integer radix
        range_if(emitter, "eq", "jo");
    }
    ins(emitter, "subs x0, x0, x14", "sub rax, rcx"); range_if(emitter, "vc", "jo");
    ins(emitter, "add x9, x9, #1", "inc r8"); jump(emitter, "accumulate");
    emitter.label(&label("apply_sign"));
    abi::emit_load_temporary_stack_slot(emitter, if arm(emitter) { "x12" } else { "r11" }, 0);
    ins(emitter, "cmp x12, #0", "cmp r11, 0"); branch(emitter, "ne", "jne", "done");
    ins(emitter, "negs x0, x0", "neg rax"); range_if(emitter, "vc", "jo");
    emitter.label(&label("done")); abi::emit_frame_restore(emitter, 48); abi::emit_return(emitter);
    emitter.label(&label("invalid")); abi::emit_jump(emitter, STRING);
    emitter.label(&label("decimal"));
    abi::emit_load_temporary_stack_slot(emitter, abi::int_arg_reg_name(emitter.target, 0), 16);
    abi::emit_load_temporary_stack_slot(emitter, abi::int_arg_reg_name(emitter.target, 1), 24);
    abi::emit_call_label(emitter, "__rt_str_numeric_value");
    ins(emitter, "cmp x2, #0", "cmp rdx, 0");
    failure(emitter, "eq", "jne", STRING);
    abi::emit_load_temporary_stack_slot(emitter, abi::int_arg_reg_name(emitter.target, 0), 16);
    abi::emit_load_temporary_stack_slot(emitter, abi::int_arg_reg_name(emitter.target, 1), 24);
    abi::emit_call_label(emitter, "__rt_checked_decimal_to_int");
    abi::emit_frame_restore(emitter, 48); abi::emit_return(emitter);
}

/// Selects caller-saved registers for one supported architecture.
fn arm(emitter: &Emitter) -> bool { emitter.target.arch == Arch::AArch64 }

/// Keeps every local branch in the radix parser's own symbol atom.
fn label(suffix: &str) -> String { format!("{PREFIX}_{suffix}") }

/// Reads a byte only after the remaining count proved the source address valid.
fn read(emitter: &mut Emitter) { ins(emitter, "ldrb w13, [x9]", "movzx edx, BYTE PTR [r8]"); }

/// Advances one bounded input byte without changing independent grammar state.
fn advance(emitter: &mut Emitter) {
    ins(emitter, "add x9, x9, #1", "inc r8"); ins(emitter, "sub x10, x10, #1", "dec r9");
}

/// Tests bounded range exhaustion before any byte dereference.
fn empty(emitter: &mut Emitter, suffix: &str) {
    ins(emitter, "cmp x10, #0", "cmp r9, 0"); branch(emitter, "eq", "je", suffix);
}

/// Classifies the ten ASCII decimal digits without losing the original byte.
fn digit(emitter: &mut Emitter, other: &str, accepted: &str) {
    ins(emitter, "sub w14, w13, #48", "lea ecx, [rdx - 48]");
    ins(emitter, "cmp w14, #9", "cmp ecx, 9"); branch(emitter, "hi", "ja", other); jump(emitter, accepted);
}

/// Accepts exactly PHP's six ASCII whitespace bytes.
fn whitespace(emitter: &mut Emitter, accepted: &str, rejected: &str) {
    ins(emitter, "cmp w13, #32", "cmp edx, 32"); branch(emitter, "eq", "je", accepted);
    ins(emitter, "sub w14, w13, #9", "lea ecx, [rdx - 9]");
    ins(emitter, "cmp w14, #4", "cmp ecx, 4"); branch(emitter, "ls", "jbe", accepted); jump(emitter, rejected);
}

/// Emits target-equivalent local control flow for the preceding comparison.
fn branch(emitter: &mut Emitter, condition: &str, x86: &str, suffix: &str) {
    ins(emitter, &format!("b.{condition} {}", label(suffix)), &format!("{x86} {}", label(suffix)));
}

/// Transfers locally without imposing architecture-specific branch syntax on the parser.
fn jump(emitter: &mut Emitter, suffix: &str) { abi::emit_jump(emitter, &label(suffix)); }

/// Keeps conditional failure transfers inside Apple symbol atoms before a wide jump.
fn failure(emitter: &mut Emitter, inverse_arm: &str, x86: &str, target: &str) {
    if arm(emitter) {
        emitter.instruction(&format!("b.{inverse_arm} 1f"));                    // keep the conditional edge local before crossing a helper boundary
        abi::emit_jump(emitter, target); emitter.label("1");
    } else { emitter.instruction(&format!("{x86} {target}")); }                 // report the checked base-domain failure without parsing its source
}
