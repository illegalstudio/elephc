//! Purpose:
//! Emits the `__rt_str_starts_with` runtime helper assembly for PHP byte strings.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - The helper compares the whole prefix with one libc `memcmp`; libc owns the
//!   vectorized byte comparison on every supported target.
//! - A prefix longer than the haystack answers false without touching memory.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the `__rt_str_starts_with` helper for the active target.
///
/// Input (AArch64): x1/x2 = haystack pointer/length, x3/x4 = needle pointer/length.
/// Output: x0 = 1 when the haystack starts with the needle, 0 otherwise.
pub fn emit_str_starts_with(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_str_starts_with_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: str_starts_with ---");
    emitter.label_global("__rt_str_starts_with");
    emitter.instruction("cmp x4, x2");                                          // reject prefixes longer than the haystack
    emitter.instruction("b.gt __rt_str_starts_with_no");                        // a long prefix cannot match
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // preserve the caller frame across libc
    emitter.instruction("mov x0, x1");                                          // first memcmp argument = haystack bytes
    emitter.instruction("mov x1, x3");                                          // second memcmp argument = prefix bytes
    emitter.instruction("mov x2, x4");                                          // memcmp length = prefix length
    emitter.bl_c("memcmp");                                                     // vectorized whole-prefix comparison
    emitter.instruction("cmp w0, #0");                                          // did the prefix compare equal?
    emitter.instruction("cset w0, eq");                                         // return 1 for equality, 0 otherwise
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore the caller frame
    emitter.instruction("ret");                                                 // return the boolean result
    emitter.label("__rt_str_starts_with_no");
    emitter.instruction("mov x0, #0");                                          // a long prefix cannot match
    emitter.instruction("ret");                                                 // return false
}

/// Emits the x86_64 `str_starts_with` helper.
///
/// Input: rdi = haystack pointer, rsi = haystack length, rdx = needle pointer,
/// rcx = needle length. Output: eax = 1 when the prefix matches, 0 otherwise.
fn emit_str_starts_with_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_starts_with ---");
    emitter.label_global("__rt_str_starts_with");
    emitter.instruction("cmp rcx, rsi");                                        // reject prefixes longer than the haystack
    emitter.instruction("jg __rt_str_starts_with_no_linux_x86_64");             // a long prefix cannot match
    emitter.instruction("sub rsp, 8");                                          // align the stack before the libc call
    emitter.instruction("mov rsi, rdx");                                        // second memcmp argument = prefix bytes
    emitter.instruction("mov rdx, rcx");                                        // memcmp length = prefix length
    emitter.bl_c("memcmp");                                                     // vectorized whole-prefix comparison
    emitter.instruction("add rsp, 8");                                          // restore the caller stack pointer
    emitter.instruction("test eax, eax");                                       // did the prefix compare equal?
    emitter.instruction("sete al");                                             // normalize equality into the low byte
    emitter.instruction("movzx eax, al");                                       // widen the boolean result
    emitter.instruction("ret");                                                 // return the boolean result
    emitter.label("__rt_str_starts_with_no_linux_x86_64");
    emitter.instruction("xor eax, eax");                                        // a long prefix cannot match
    emitter.instruction("ret");                                                 // return false
}
