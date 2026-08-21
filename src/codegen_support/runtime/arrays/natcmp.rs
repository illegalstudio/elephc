//! Purpose:
//! Emits `__rt_natcmp` and `__rt_natcasecmp`: php's `strnatcmp`/`strnatcasecmp` natural-order
//! comparison over two length-bounded strings, the comparator behind `natsort()` and
//! `natcasesort()` on string arrays.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - The algorithm is php's `strnatcmp_ex` (Martin Pool's natural order), measured against
//!   `php -n`: whitespace before a field is skipped ("a 1" == "a1"); two digit runs with no
//!   leading zero compare as integers (longer run wins, first difference tie-breaks:
//!   "img2" < "img10"); a leading zero makes the comparison fractional, deciding on the
//!   first differing digit ("a01" < "a1", "1.002" < "1.02"); either string running out
//!   loses. An EMPTY input short-circuits on lengths (" " > "" — the skip loop must not
//!   erase that).
//! - Case folding uses TOUPPER, not tolower — measured: `strnatcasecmp("_", "x")` is +1,
//!   which only holds when 'x' folds up to 'X' (88) below '_' (95).
//! - php's strings are NUL-terminated so its skip loops stop on the terminator; these
//!   strings are `(ptr, len)` pairs, so every read is bounded and past-the-end reads as 0.
//! - Leaf helpers: no calls, no frame, every value lives in caller-saved registers.
//!
//! # ABI (matches `__rt_strcmp`)
//! - **ARM64** — in: `x1` = a ptr, `x2` = a len, `x3` = b ptr, `x4` = b len.
//!   Out: `x0` = -1 / 0 / +1. Clobbers x5, x6, x9-x13.
//! - **x86_64** — in: `rdi`, `rsi`, `rdx`, `rcx` with the same meaning. Out: `rax`.
//!   rbx is used for the digit-run bias and saved around the body.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits one natural-order comparator; `fold_case` selects `strnatcasecmp` over `strnatcmp`.
pub fn emit_natcmp(emitter: &mut Emitter, label: &str, fold_case: bool) {
    if emitter.target.arch == Arch::X86_64 {
        emit_natcmp_linux_x86_64(emitter, label, fold_case);
        return;
    }

    emitter.blank();
    emitter.comment(&format!("--- runtime: {} ---", label));
    emitter.label_global(label);

    // -- an empty side decides on lengths alone, before any whitespace skipping --
    emitter.instruction(&format!("cbnz x2, {}_a_nonempty", label));
    emitter.instruction(&format!("cbnz x4, {}_ret_neg", label));                 // a empty, b not: a first
    emitter.instruction("mov x0, #0");
    emitter.instruction("ret");
    emitter.label(&format!("{}_a_nonempty", label));
    emitter.instruction(&format!("cbz x4, {}_ret_pos", label));                  // b empty, a not: b first
    emitter.instruction("mov x5, xzr");                                         // ai = 0
    emitter.instruction("mov x6, xzr");                                         // bi = 0

    emitter.label(&format!("{}_main", label));
    // -- load ca, skipping whitespace (32, 9..13); past-the-end reads as 0 --
    emitter.label(&format!("{}_load_ca", label));
    emitter.instruction("mov w9, wzr");
    emitter.instruction("cmp x5, x2");
    emitter.instruction(&format!("b.hs {}_ca_ready", label));                    // past the end: ca = 0
    emitter.instruction("ldrb w9, [x1, x5]");
    emitter.instruction("cmp w9, #32");
    emitter.instruction(&format!("b.eq {}_ca_skip", label));
    emitter.instruction("sub w11, w9, #9");
    emitter.instruction("cmp w11, #4");                                         // 9..13 are the other whitespace bytes
    emitter.instruction(&format!("b.hi {}_ca_ready", label));
    emitter.label(&format!("{}_ca_skip", label));
    emitter.instruction("add x5, x5, #1");
    emitter.instruction(&format!("b {}_load_ca", label));
    emitter.label(&format!("{}_ca_ready", label));
    // -- load cb the same way --
    emitter.label(&format!("{}_load_cb", label));
    emitter.instruction("mov w10, wzr");
    emitter.instruction("cmp x6, x4");
    emitter.instruction(&format!("b.hs {}_cb_ready", label));
    emitter.instruction("ldrb w10, [x3, x6]");
    emitter.instruction("cmp w10, #32");
    emitter.instruction(&format!("b.eq {}_cb_skip", label));
    emitter.instruction("sub w11, w10, #9");
    emitter.instruction("cmp w11, #4");
    emitter.instruction(&format!("b.hi {}_cb_ready", label));
    emitter.label(&format!("{}_cb_skip", label));
    emitter.instruction("add x6, x6, #1");
    emitter.instruction(&format!("b {}_load_cb", label));
    emitter.label(&format!("{}_cb_ready", label));

    // -- two digit fields compare as numbers --
    emitter.instruction("sub w11, w9, #48");
    emitter.instruction("cmp w11, #9");
    emitter.instruction(&format!("b.hi {}_nondigit", label));
    emitter.instruction("sub w12, w10, #48");
    emitter.instruction("cmp w12, #9");
    emitter.instruction(&format!("b.hi {}_nondigit", label));
    emitter.instruction("cmp w9, #48");                                         // a leading zero makes the field fractional
    emitter.instruction(&format!("b.eq {}_cl_loop", label));
    emitter.instruction("cmp w10, #48");
    emitter.instruction(&format!("b.eq {}_cl_loop", label));

    // -- compare_right: longer run wins, first difference remembered as the bias --
    emitter.instruction("mov w13, wzr");                                        // bias = 0
    emitter.label(&format!("{}_cr_loop", label));
    emitter.instruction("mov w9, wzr");
    emitter.instruction("cmp x5, x2");
    emitter.instruction(&format!("b.hs {}_cr_a_nd", label));
    emitter.instruction("ldrb w9, [x1, x5]");
    emitter.instruction("sub w11, w9, #48");
    emitter.instruction("cmp w11, #9");
    emitter.instruction(&format!("b.ls {}_cr_a_digit", label));
    emitter.label(&format!("{}_cr_a_nd", label));
    // a's run ended; if b still has digits, b's number is larger
    emitter.instruction("cmp x6, x4");
    emitter.instruction(&format!("b.hs {}_cr_both_nd", label));
    emitter.instruction("ldrb w10, [x3, x6]");
    emitter.instruction("sub w12, w10, #48");
    emitter.instruction("cmp w12, #9");
    emitter.instruction(&format!("b.ls {}_ret_neg", label));
    emitter.label(&format!("{}_cr_both_nd", label));
    emitter.instruction(&format!("cbnz w13, {}_ret_bias", label));               // equal-length runs: the first difference decides
    emitter.instruction(&format!("b {}_after_run", label));
    emitter.label(&format!("{}_cr_a_digit", label));
    // a still has digits; if b's run ended, a's number is larger
    emitter.instruction("cmp x6, x4");
    emitter.instruction(&format!("b.hs {}_ret_pos", label));
    emitter.instruction("ldrb w10, [x3, x6]");
    emitter.instruction("sub w12, w10, #48");
    emitter.instruction("cmp w12, #9");
    emitter.instruction(&format!("b.hi {}_ret_pos", label));
    emitter.instruction("cmp w9, w10");
    emitter.instruction(&format!("b.eq {}_cr_next", label));
    emitter.instruction(&format!("cbnz w13, {}_cr_next", label));                // only the FIRST difference sets the bias
    emitter.instruction("mov w13, #1");
    emitter.instruction("mov w11, #-1");
    emitter.instruction("csel w13, w11, w13, lo");                              // bias = (ca < cb) ? -1 : +1
    emitter.label(&format!("{}_cr_next", label));
    emitter.instruction("add x5, x5, #1");
    emitter.instruction("add x6, x6, #1");
    emitter.instruction(&format!("b {}_cr_loop", label));
    emitter.label(&format!("{}_ret_bias", label));
    emitter.instruction("sxtw x0, w13");
    emitter.instruction("ret");

    // -- compare_left: fractional fields decide on the first differing digit --
    emitter.label(&format!("{}_cl_loop", label));
    emitter.instruction("mov w9, wzr");
    emitter.instruction("cmp x5, x2");
    emitter.instruction(&format!("b.hs {}_cl_a_nd", label));
    emitter.instruction("ldrb w9, [x1, x5]");
    emitter.instruction("sub w11, w9, #48");
    emitter.instruction("cmp w11, #9");
    emitter.instruction(&format!("b.ls {}_cl_a_digit", label));
    emitter.label(&format!("{}_cl_a_nd", label));
    emitter.instruction("cmp x6, x4");
    emitter.instruction(&format!("b.hs {}_after_run", label));                   // both runs ended together: equal fields
    emitter.instruction("ldrb w10, [x3, x6]");
    emitter.instruction("sub w12, w10, #48");
    emitter.instruction("cmp w12, #9");
    emitter.instruction(&format!("b.ls {}_ret_neg", label));                     // b's fraction has more digits
    emitter.instruction(&format!("b {}_after_run", label));
    emitter.label(&format!("{}_cl_a_digit", label));
    emitter.instruction("cmp x6, x4");
    emitter.instruction(&format!("b.hs {}_ret_pos", label));
    emitter.instruction("ldrb w10, [x3, x6]");
    emitter.instruction("sub w12, w10, #48");
    emitter.instruction("cmp w12, #9");
    emitter.instruction(&format!("b.hi {}_ret_pos", label));
    emitter.instruction("cmp w9, w10");
    emitter.instruction(&format!("b.lo {}_ret_neg", label));                     // the first differing digit decides
    emitter.instruction(&format!("b.hi {}_ret_pos", label));
    emitter.instruction("add x5, x5, #1");
    emitter.instruction("add x6, x6, #1");
    emitter.instruction(&format!("b {}_cl_loop", label));

    // -- a digit field compared equal; whoever ran out of string loses --
    emitter.label(&format!("{}_after_run", label));
    emitter.instruction("cmp x5, x2");
    emitter.instruction(&format!("b.lo {}_ar_a_more", label));
    emitter.instruction("cmp x6, x4");
    emitter.instruction(&format!("b.hs {}_ret_zero", label));
    emitter.instruction(&format!("b {}_ret_neg", label));
    emitter.label(&format!("{}_ar_a_more", label));
    emitter.instruction("cmp x6, x4");
    emitter.instruction(&format!("b.hs {}_ret_pos", label));
    emitter.instruction(&format!("b {}_main", label));

    // -- ordinary characters compare bytewise, optionally folded UP --
    emitter.label(&format!("{}_nondigit", label));
    if fold_case {
        emitter.instruction("sub w11, w9, #97");                                // 'a'..'z' fold to 'A'..'Z': php compares through toupper
        emitter.instruction("cmp w11, #25");
        emitter.instruction(&format!("b.hi {}_fold_b", label));
        emitter.instruction("sub w9, w9, #32");
        emitter.label(&format!("{}_fold_b", label));
        emitter.instruction("sub w11, w10, #97");
        emitter.instruction("cmp w11, #25");
        emitter.instruction(&format!("b.hi {}_folded", label));
        emitter.instruction("sub w10, w10, #32");
        emitter.label(&format!("{}_folded", label));
    }
    emitter.instruction("cmp w9, w10");
    emitter.instruction(&format!("b.lo {}_ret_neg", label));
    emitter.instruction(&format!("b.hi {}_ret_pos", label));
    emitter.instruction("add x5, x5, #1");
    emitter.instruction("add x6, x6, #1");
    emitter.instruction("cmp x5, x2");
    emitter.instruction(&format!("b.lo {}_nd_a_more", label));
    emitter.instruction("cmp x6, x4");
    emitter.instruction(&format!("b.hs {}_ret_zero", label));
    emitter.instruction(&format!("b {}_ret_neg", label));
    emitter.label(&format!("{}_nd_a_more", label));
    emitter.instruction("cmp x6, x4");
    emitter.instruction(&format!("b.hs {}_ret_pos", label));
    emitter.instruction(&format!("b {}_main", label));

    emitter.label(&format!("{}_ret_neg", label));
    emitter.instruction("mov x0, #-1");
    emitter.instruction("ret");
    emitter.label(&format!("{}_ret_pos", label));
    emitter.instruction("mov x0, #1");
    emitter.instruction("ret");
    emitter.label(&format!("{}_ret_zero", label));
    emitter.instruction("mov x0, #0");
    emitter.instruction("ret");
}

/// Emits the x86_64 form of [`emit_natcmp`]. rbx carries the compare_right bias and is
/// callee-saved, so every exit funnels through one epilogue that restores it.
fn emit_natcmp_linux_x86_64(emitter: &mut Emitter, label: &str, fold_case: bool) {
    emitter.blank();
    emitter.comment(&format!("--- runtime: {} ---", label));
    emitter.label_global(label);

    emitter.instruction("push rbx");                                            // rbx carries the digit-run bias below
    // -- an empty side decides on lengths alone, before any whitespace skipping --
    emitter.instruction("test rsi, rsi");
    emitter.instruction(&format!("jnz {}_a_nonempty", label));
    emitter.instruction("test rcx, rcx");
    emitter.instruction(&format!("jnz {}_ret_neg", label));                     // a empty, b not: a first
    emitter.instruction("xor eax, eax");
    emitter.instruction(&format!("jmp {}_ret", label));
    emitter.label(&format!("{}_a_nonempty", label));
    emitter.instruction("test rcx, rcx");
    emitter.instruction(&format!("jz {}_ret_pos", label));                      // b empty, a not: b first
    emitter.instruction("xor r8d, r8d");                                        // ai = 0
    emitter.instruction("xor r9d, r9d");                                        // bi = 0

    emitter.label(&format!("{}_main", label));
    // -- load ca, skipping whitespace (32, 9..13); past-the-end reads as 0 --
    emitter.label(&format!("{}_load_ca", label));
    emitter.instruction("xor r10d, r10d");
    emitter.instruction("cmp r8, rsi");
    emitter.instruction(&format!("jae {}_ca_ready", label));                    // past the end: ca = 0
    emitter.instruction("movzx r10d, BYTE PTR [rdi + r8]");
    emitter.instruction("cmp r10d, 32");
    emitter.instruction(&format!("je {}_ca_skip", label));
    emitter.instruction("lea eax, [r10 - 9]");
    emitter.instruction("cmp eax, 4");                                          // 9..13 are the other whitespace bytes
    emitter.instruction(&format!("ja {}_ca_ready", label));
    emitter.label(&format!("{}_ca_skip", label));
    emitter.instruction("add r8, 1");
    emitter.instruction(&format!("jmp {}_load_ca", label));
    emitter.label(&format!("{}_ca_ready", label));
    // -- load cb the same way --
    emitter.label(&format!("{}_load_cb", label));
    emitter.instruction("xor r11d, r11d");
    emitter.instruction("cmp r9, rcx");
    emitter.instruction(&format!("jae {}_cb_ready", label));
    emitter.instruction("movzx r11d, BYTE PTR [rdx + r9]");
    emitter.instruction("cmp r11d, 32");
    emitter.instruction(&format!("je {}_cb_skip", label));
    emitter.instruction("lea eax, [r11 - 9]");
    emitter.instruction("cmp eax, 4");
    emitter.instruction(&format!("ja {}_cb_ready", label));
    emitter.label(&format!("{}_cb_skip", label));
    emitter.instruction("add r9, 1");
    emitter.instruction(&format!("jmp {}_load_cb", label));
    emitter.label(&format!("{}_cb_ready", label));

    // -- two digit fields compare as numbers --
    emitter.instruction("lea eax, [r10 - 48]");
    emitter.instruction("cmp eax, 9");
    emitter.instruction(&format!("ja {}_nondigit", label));
    emitter.instruction("lea eax, [r11 - 48]");
    emitter.instruction("cmp eax, 9");
    emitter.instruction(&format!("ja {}_nondigit", label));
    emitter.instruction("cmp r10d, 48");                                        // a leading zero makes the field fractional
    emitter.instruction(&format!("je {}_cl_loop", label));
    emitter.instruction("cmp r11d, 48");
    emitter.instruction(&format!("je {}_cl_loop", label));

    // -- compare_right: longer run wins, first difference remembered as the bias --
    emitter.instruction("xor ebx, ebx");                                        // bias = 0
    emitter.label(&format!("{}_cr_loop", label));
    emitter.instruction("xor r10d, r10d");
    emitter.instruction("cmp r8, rsi");
    emitter.instruction(&format!("jae {}_cr_a_nd", label));
    emitter.instruction("movzx r10d, BYTE PTR [rdi + r8]");
    emitter.instruction("lea eax, [r10 - 48]");
    emitter.instruction("cmp eax, 9");
    emitter.instruction(&format!("jbe {}_cr_a_digit", label));
    emitter.label(&format!("{}_cr_a_nd", label));
    // a's run ended; if b still has digits, b's number is larger
    emitter.instruction("cmp r9, rcx");
    emitter.instruction(&format!("jae {}_cr_both_nd", label));
    emitter.instruction("movzx r11d, BYTE PTR [rdx + r9]");
    emitter.instruction("lea eax, [r11 - 48]");
    emitter.instruction("cmp eax, 9");
    emitter.instruction(&format!("jbe {}_ret_neg", label));
    emitter.label(&format!("{}_cr_both_nd", label));
    emitter.instruction("test ebx, ebx");
    emitter.instruction(&format!("jnz {}_ret_bias", label));                    // equal-length runs: the first difference decides
    emitter.instruction(&format!("jmp {}_after_run", label));
    emitter.label(&format!("{}_cr_a_digit", label));
    // a still has digits; if b's run ended, a's number is larger
    emitter.instruction("cmp r9, rcx");
    emitter.instruction(&format!("jae {}_ret_pos", label));
    emitter.instruction("movzx r11d, BYTE PTR [rdx + r9]");
    emitter.instruction("lea eax, [r11 - 48]");
    emitter.instruction("cmp eax, 9");
    emitter.instruction(&format!("ja {}_ret_pos", label));
    emitter.instruction("cmp r10d, r11d");
    emitter.instruction(&format!("je {}_cr_next", label));
    emitter.instruction("test ebx, ebx");
    emitter.instruction(&format!("jnz {}_cr_next", label));                     // only the FIRST difference sets the bias
    emitter.instruction("mov ebx, 1");
    emitter.instruction("mov eax, -1");
    emitter.instruction("cmp r10d, r11d");                                      // regenerate CF: the test above clobbered the first compare's flags
    emitter.instruction("cmovb ebx, eax");                                      // bias = (ca < cb) ? -1 : +1
    emitter.label(&format!("{}_cr_next", label));
    emitter.instruction("add r8, 1");
    emitter.instruction("add r9, 1");
    emitter.instruction(&format!("jmp {}_cr_loop", label));
    emitter.label(&format!("{}_ret_bias", label));
    emitter.instruction("movsxd rax, ebx");
    emitter.instruction(&format!("jmp {}_ret", label));

    // -- compare_left: fractional fields decide on the first differing digit --
    emitter.label(&format!("{}_cl_loop", label));
    emitter.instruction("xor r10d, r10d");
    emitter.instruction("cmp r8, rsi");
    emitter.instruction(&format!("jae {}_cl_a_nd", label));
    emitter.instruction("movzx r10d, BYTE PTR [rdi + r8]");
    emitter.instruction("lea eax, [r10 - 48]");
    emitter.instruction("cmp eax, 9");
    emitter.instruction(&format!("jbe {}_cl_a_digit", label));
    emitter.label(&format!("{}_cl_a_nd", label));
    emitter.instruction("cmp r9, rcx");
    emitter.instruction(&format!("jae {}_after_run", label));                   // both runs ended together: equal fields
    emitter.instruction("movzx r11d, BYTE PTR [rdx + r9]");
    emitter.instruction("lea eax, [r11 - 48]");
    emitter.instruction("cmp eax, 9");
    emitter.instruction(&format!("jbe {}_ret_neg", label));                     // b's fraction has more digits
    emitter.instruction(&format!("jmp {}_after_run", label));
    emitter.label(&format!("{}_cl_a_digit", label));
    emitter.instruction("cmp r9, rcx");
    emitter.instruction(&format!("jae {}_ret_pos", label));
    emitter.instruction("movzx r11d, BYTE PTR [rdx + r9]");
    emitter.instruction("lea eax, [r11 - 48]");
    emitter.instruction("cmp eax, 9");
    emitter.instruction(&format!("ja {}_ret_pos", label));
    emitter.instruction("cmp r10d, r11d");
    emitter.instruction(&format!("jb {}_ret_neg", label));                      // the first differing digit decides
    emitter.instruction(&format!("ja {}_ret_pos", label));
    emitter.instruction("add r8, 1");
    emitter.instruction("add r9, 1");
    emitter.instruction(&format!("jmp {}_cl_loop", label));

    // -- a digit field compared equal; whoever ran out of string loses --
    emitter.label(&format!("{}_after_run", label));
    emitter.instruction("cmp r8, rsi");
    emitter.instruction(&format!("jb {}_ar_a_more", label));
    emitter.instruction("cmp r9, rcx");
    emitter.instruction(&format!("jae {}_ret_zero", label));
    emitter.instruction(&format!("jmp {}_ret_neg", label));
    emitter.label(&format!("{}_ar_a_more", label));
    emitter.instruction("cmp r9, rcx");
    emitter.instruction(&format!("jae {}_ret_pos", label));
    emitter.instruction(&format!("jmp {}_main", label));

    // -- ordinary characters compare bytewise, optionally folded UP --
    emitter.label(&format!("{}_nondigit", label));
    if fold_case {
        emitter.instruction("lea eax, [r10 - 97]");                             // 'a'..'z' fold to 'A'..'Z': php compares through toupper
        emitter.instruction("cmp eax, 25");
        emitter.instruction(&format!("ja {}_fold_b", label));
        emitter.instruction("sub r10d, 32");
        emitter.label(&format!("{}_fold_b", label));
        emitter.instruction("lea eax, [r11 - 97]");
        emitter.instruction("cmp eax, 25");
        emitter.instruction(&format!("ja {}_folded", label));
        emitter.instruction("sub r11d, 32");
        emitter.label(&format!("{}_folded", label));
    }
    emitter.instruction("cmp r10d, r11d");
    emitter.instruction(&format!("jb {}_ret_neg", label));
    emitter.instruction(&format!("ja {}_ret_pos", label));
    emitter.instruction("add r8, 1");
    emitter.instruction("add r9, 1");
    emitter.instruction("cmp r8, rsi");
    emitter.instruction(&format!("jb {}_nd_a_more", label));
    emitter.instruction("cmp r9, rcx");
    emitter.instruction(&format!("jae {}_ret_zero", label));
    emitter.instruction(&format!("jmp {}_ret_neg", label));
    emitter.label(&format!("{}_nd_a_more", label));
    emitter.instruction("cmp r9, rcx");
    emitter.instruction(&format!("jae {}_ret_pos", label));
    emitter.instruction(&format!("jmp {}_main", label));

    emitter.label(&format!("{}_ret_neg", label));
    emitter.instruction("mov rax, -1");
    emitter.instruction(&format!("jmp {}_ret", label));
    emitter.label(&format!("{}_ret_pos", label));
    emitter.instruction("mov rax, 1");
    emitter.instruction(&format!("jmp {}_ret", label));
    emitter.label(&format!("{}_ret_zero", label));
    emitter.instruction("xor eax, eax");
    emitter.label(&format!("{}_ret", label));
    emitter.instruction("pop rbx");                                             // restore the caller's bias register
    emitter.instruction("ret");
}
