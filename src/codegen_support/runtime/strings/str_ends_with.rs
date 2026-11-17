//! Purpose:
//! Emits the `__rt_str_ends_with` runtime helper assembly for PHP byte strings.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - The helper compares the whole suffix with one libc `memcmp`; libc owns the
//!   vectorized byte comparison on every supported target.
//! - A suffix longer than the haystack answers false without touching memory.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the `__rt_str_ends_with` helper for the active target.
///
/// Input (AArch64): x1/x2 = haystack pointer/length, x3/x4 = needle pointer/length.
/// Output: x0 = 1 when the haystack ends with the needle, 0 otherwise.
pub fn emit_str_ends_with(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_str_ends_with_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: str_ends_with ---");
    emitter.label_global("__rt_str_ends_with");
    emitter.instruction("cmp x4, x2");                                          // reject suffixes longer than the haystack
    emitter.instruction("b.gt __rt_str_ends_with_no");                          // a long suffix cannot match
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // preserve the caller frame across libc
    emitter.instruction("sub x0, x2, x4");                                      // compute the suffix start offset
    emitter.instruction("add x0, x1, x0");                                      // first memcmp argument = suffix bytes
    emitter.instruction("mov x1, x3");                                          // second memcmp argument = needle bytes
    emitter.instruction("mov x2, x4");                                          // memcmp length = suffix length
    emitter.bl_c("memcmp");                                                     // vectorized whole-suffix comparison
    emitter.instruction("cmp w0, #0");                                          // did the suffix compare equal?
    emitter.instruction("cset w0, eq");                                         // return 1 for equality, 0 otherwise
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore the caller frame
    emitter.instruction("ret");                                                 // return the boolean result
    emitter.label("__rt_str_ends_with_no");
    emitter.instruction("mov x0, #0");                                          // a long suffix cannot match
    emitter.instruction("ret");                                                 // return false
}

/// Emits the x86_64 `str_ends_with` helper.
///
/// Input: rdi = haystack pointer, rsi = haystack length, rdx = needle pointer,
/// rcx = needle length. Output: eax = 1 when the suffix matches, 0 otherwise.
fn emit_str_ends_with_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_ends_with ---");
    emitter.label_global("__rt_str_ends_with");
    emitter.instruction("cmp rcx, rsi");                                        // reject suffixes longer than the haystack
    emitter.instruction("jg __rt_str_ends_with_no_linux_x86_64");               // a long suffix cannot match
    emitter.instruction("sub rsp, 8");                                          // align the stack before the libc call
    emitter.instruction("mov rax, rsi");                                        // copy the haystack length
    emitter.instruction("sub rax, rcx");                                        // compute the suffix start offset
    emitter.instruction("add rdi, rax");                                        // first memcmp argument = suffix bytes
    emitter.instruction("mov rsi, rdx");                                        // second memcmp argument = needle bytes
    emitter.instruction("mov rdx, rcx");                                        // memcmp length = suffix length
    emitter.bl_c("memcmp");                                                     // vectorized whole-suffix comparison
    emitter.instruction("add rsp, 8");                                          // restore the caller stack pointer
    emitter.instruction("test eax, eax");                                       // did the suffix compare equal?
    emitter.instruction("sete al");                                             // normalize equality into the low byte
    emitter.instruction("movzx eax, al");                                       // widen the boolean result
    emitter.instruction("ret");                                                 // return the boolean result
    emitter.label("__rt_str_ends_with_no_linux_x86_64");
    emitter.instruction("xor eax, eax");                                        // a long suffix cannot match
    emitter.instruction("ret");                                                 // return false
}
