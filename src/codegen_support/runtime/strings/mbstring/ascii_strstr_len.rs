//! Purpose:
//! Emits the allocation-free `strlen(mb_strstr(...))` fast path for explicit-UTF-8 ASCII
//! haystacks and needles.
//!
//! Called from:
//! - `crate::codegen_support::runtime::strings::mbstring::emit_mbstring()`.
//!
//! Key details:
//! - The helper proves both operands are ASCII, runs the matching byte search, and returns the
//!   prefix or suffix byte length. ASCII byte and codepoint positions are identical, so the
//!   length matches PHP's `strlen(mb_strstr(...))`.
//! - A non-ASCII operand returns `-1`; the caller then runs the Unicode bridge and measures the
//!   boxed result instead.
//! - AArch64 input: `x1`/`x2` = haystack, `x3`/`x4` = needle, `x5` = before flag,
//!   `x6` = mode (0 forward, 1 forward-insensitive, 2 reverse, 3 reverse-insensitive).
//!   x86_64 input: `rdi`/`rsi`, `rdx`/`rcx`, `r8`, `r9`. Result in `x0`/`rax`.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits the mbstring search-length helper for the active target.
pub(super) fn emit(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64(emitter);
    } else {
        emit_aarch64(emitter);
    }
}

/// Emits the AArch64 search-length helper.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mb_strstr_len_utf8 ---");
    emitter.label_global("__rt_mb_strstr_len_utf8");
    emitter.instruction("sub sp, sp, #96");                                     // reserve both string pairs, the flag, and the mode
    emitter.instruction("stp x29, x30, [sp, #80]");                             // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #80");                                    // establish a stable frame base
    emitter.instruction("stp x1, x2, [sp, #0]");                                // save the haystack pointer and length
    emitter.instruction("stp x3, x4, [sp, #16]");                               // save the needle pointer and length
    emitter.instruction("str x5, [sp, #32]");                                   // save the before-needle flag
    emitter.instruction("str x6, [sp, #40]");                                   // save the search mode
    emitter.instruction("ldr x1, [sp, #0]");                                    // load the haystack pointer for the ASCII check
    emitter.instruction("ldr x2, [sp, #8]");                                    // load the haystack length for the ASCII check
    abi::emit_call_label(emitter, "__rt_mb_is_ascii");
    emitter.instruction("cbz x0, __rt_mb_strstr_len_utf8_fallback");           // a non-ASCII haystack needs the Unicode bridge
    emitter.instruction("ldr x1, [sp, #16]");                                   // load the needle pointer for the ASCII check
    emitter.instruction("ldr x2, [sp, #24]");                                   // load the needle length for the ASCII check
    abi::emit_call_label(emitter, "__rt_mb_is_ascii");
    emitter.instruction("cbz x0, __rt_mb_strstr_len_utf8_fallback");           // a non-ASCII needle needs the Unicode bridge
    emitter.instruction("ldp x1, x2, [sp, #0]");                                // restore the haystack search pair
    emitter.instruction("ldp x3, x4, [sp, #16]");                               // restore the needle search pair
    emitter.instruction("ldr x6, [sp, #40]");                                   // reload the search mode
    emitter.instruction("cmp x6, #0");                                          // forward case-sensitive search?
    emitter.instruction("b.eq __rt_mb_strstr_len_utf8_forward");                // run the first-occurrence search
    emitter.instruction("cmp x6, #1");                                          // forward case-insensitive search?
    emitter.instruction("b.eq __rt_mb_strstr_len_utf8_forward_fold");           // run the folded first-occurrence search
    emitter.instruction("cmp x6, #2");                                          // reverse case-sensitive search?
    emitter.instruction("b.eq __rt_mb_strstr_len_utf8_reverse");                // run the last-occurrence search
    abi::emit_call_label(emitter, "__rt_strripos");                            // folded last-occurrence search
    emitter.instruction("b __rt_mb_strstr_len_utf8_searched");                  // measure the selected substring
    emitter.label("__rt_mb_strstr_len_utf8_forward");
    abi::emit_call_label(emitter, "__rt_strpos");                              // first-occurrence search
    emitter.instruction("b __rt_mb_strstr_len_utf8_searched");                  // measure the selected substring
    emitter.label("__rt_mb_strstr_len_utf8_forward_fold");
    abi::emit_call_label(emitter, "__rt_stripos");                             // folded first-occurrence search
    emitter.instruction("b __rt_mb_strstr_len_utf8_searched");                  // measure the selected substring
    emitter.label("__rt_mb_strstr_len_utf8_reverse");
    abi::emit_call_label(emitter, "__rt_strrpos");                             // last-occurrence search
    emitter.label("__rt_mb_strstr_len_utf8_searched");
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the haystack byte length
    emitter.instruction("ldr x5, [sp, #32]");                                   // reload the before-needle flag
    emitter.instruction("cmp x0, #0");                                          // did the search find the needle?
    emitter.instruction("b.lt __rt_mb_strstr_len_utf8_miss");                   // a missing needle makes strlen(false) zero
    emitter.instruction("cbnz x5, __rt_mb_strstr_len_utf8_done");               // a truthy flag returns the prefix length
    emitter.instruction("sub x0, x2, x0");                                      // suffix length = haystack length - match offset
    emitter.instruction("b __rt_mb_strstr_len_utf8_done");                      // return the selected length
    emitter.label("__rt_mb_strstr_len_utf8_miss");
    emitter.instruction("mov x0, xzr");                                         // strlen(false) is zero
    emitter.label("__rt_mb_strstr_len_utf8_done");
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #96");                                     // release the helper frame
    emitter.instruction("ret");                                                // return the selected substring length
    emitter.label("__rt_mb_strstr_len_utf8_fallback");
    emitter.instruction("mov x0, #-1");                                         // signal that the caller must use the bridge
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #96");                                     // release the helper frame
    emitter.instruction("ret");                                                // return the fallback sentinel
}

/// Emits the x86_64 search-length helper.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mb_strstr_len_utf8 ---");
    emitter.label_global("__rt_mb_strstr_len_utf8");
    emitter.instruction("push rbp");                                           // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                       // establish a stable frame base
    emitter.instruction("sub rsp, 64");                                        // reserve both string pairs, the flag, and the mode
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                       // save the haystack pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                      // save the haystack length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                      // save the needle pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                      // save the needle length
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                       // save the before-needle flag
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                       // save the search mode
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                       // load the haystack pointer for the ASCII check
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                      // load the haystack length for the ASCII check
    abi::emit_call_label(emitter, "__rt_mb_is_ascii");
    emitter.instruction("test ecx, ecx");                                      // is the haystack entirely ASCII?
    emitter.instruction("jz __rt_mb_strstr_len_utf8_fallback_x86");            // a non-ASCII haystack needs the Unicode bridge
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                      // load the needle pointer for the ASCII check
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                      // load the needle length for the ASCII check
    abi::emit_call_label(emitter, "__rt_mb_is_ascii");
    emitter.instruction("test ecx, ecx");                                      // is the needle entirely ASCII?
    emitter.instruction("jz __rt_mb_strstr_len_utf8_fallback_x86");            // a non-ASCII needle needs the Unicode bridge
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                       // restore the haystack pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                      // restore the haystack length
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                      // restore the needle pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                      // restore the needle length
    emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                       // reload the search mode
    emitter.instruction("cmp r9, 0");                                          // forward case-sensitive search?
    emitter.instruction("je __rt_mb_strstr_len_utf8_forward_x86");             // run the first-occurrence search
    emitter.instruction("cmp r9, 1");                                          // forward case-insensitive search?
    emitter.instruction("je __rt_mb_strstr_len_utf8_forward_fold_x86");        // run the folded first-occurrence search
    emitter.instruction("cmp r9, 2");                                          // reverse case-sensitive search?
    emitter.instruction("je __rt_mb_strstr_len_utf8_reverse_x86");             // run the last-occurrence search
    abi::emit_call_label(emitter, "__rt_strripos");                           // folded last-occurrence search
    emitter.instruction("jmp __rt_mb_strstr_len_utf8_searched_x86");           // measure the selected substring
    emitter.label("__rt_mb_strstr_len_utf8_forward_x86");
    abi::emit_call_label(emitter, "__rt_strpos");                             // first-occurrence search
    emitter.instruction("jmp __rt_mb_strstr_len_utf8_searched_x86");           // measure the selected substring
    emitter.label("__rt_mb_strstr_len_utf8_forward_fold_x86");
    abi::emit_call_label(emitter, "__rt_stripos");                            // folded first-occurrence search
    emitter.instruction("jmp __rt_mb_strstr_len_utf8_searched_x86");           // measure the selected substring
    emitter.label("__rt_mb_strstr_len_utf8_reverse_x86");
    abi::emit_call_label(emitter, "__rt_strrpos");                            // last-occurrence search
    emitter.label("__rt_mb_strstr_len_utf8_searched_x86");
    emitter.instruction("mov r8, QWORD PTR [rbp - 40]");                       // reload the before-needle flag
    emitter.instruction("cmp rax, 0");                                         // did the search find the needle?
    emitter.instruction("jl __rt_mb_strstr_len_utf8_miss_x86");                // a missing needle makes strlen(false) zero
    emitter.instruction("test r8, r8");                                        // was the before-needle flag truthy?
    emitter.instruction("jnz __rt_mb_strstr_len_utf8_done_x86");               // a truthy flag returns the prefix length
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                      // reload the haystack byte length
    emitter.instruction("sub rdx, rax");                                       // suffix length = haystack length - match offset
    emitter.instruction("mov rax, rdx");                                       // carry the suffix length in the result register
    emitter.instruction("jmp __rt_mb_strstr_len_utf8_done_x86");               // return the selected length
    emitter.label("__rt_mb_strstr_len_utf8_miss_x86");
    emitter.instruction("xor eax, eax");                                       // strlen(false) is zero
    emitter.label("__rt_mb_strstr_len_utf8_done_x86");
    emitter.instruction("leave");                                              // restore the caller frame pointer
    emitter.instruction("ret");                                               // return the selected substring length
    emitter.label("__rt_mb_strstr_len_utf8_fallback_x86");
    emitter.instruction("mov rax, -1");                                        // signal that the caller must use the bridge
    emitter.instruction("leave");                                              // restore the caller frame pointer
    emitter.instruction("ret");                                               // return the fallback sentinel
}
