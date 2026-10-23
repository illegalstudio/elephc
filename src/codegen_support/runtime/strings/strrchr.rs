//! Purpose:
//! Emits the `__rt_strrchr` runtime helper assembly for PHP's `strrchr`: the byte offset of
//! the last occurrence of the needle's first byte.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - php-src searches only `needle[0]`; an empty needle searches the NUL byte.
//! - The helper returns an offset (or `-1`); the generated call site slices the haystack and
//!   boxes the `string|false` result, like `strstr()`.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits the `__rt_strrchr` runtime helper.
///
/// ABI (AArch64):
///   Input:  `x1`/`x2` = haystack, `x3`/`x4` = needle.
///   Output: `x0` = offset of the last matching byte, or `-1`.
///
/// ABI (x86_64 System V):
///   Input:  `rdi`/`rsi` = haystack, `rdx`/`rcx` = needle.
///   Output: `rax` = offset of the last matching byte, or `-1`.
///
/// The helper is a leaf and preserves every argument register except the scratch ones it
/// names (`x9`/`x10`, `r8`/`r9`).
pub fn emit_strrchr(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_strrchr_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: strrchr ---");
    emitter.label_global("__rt_strrchr");

    emitter.instruction("mov w9, #0");                                          // an empty needle searches the NUL byte
    emitter.instruction("cbz x4, __rt_strrchr_search");                         // keep NUL when the needle has no first byte
    emitter.instruction("ldrb w9, [x3]");                                       // php-src searches only the needle's first byte

    emitter.label("__rt_strrchr_search");
    emitter.instruction("mov x0, x2");                                          // scan backwards from one past the last haystack byte

    emitter.label("__rt_strrchr_loop");
    emitter.instruction("cbz x0, __rt_strrchr_miss");                           // the whole haystack has been scanned
    emitter.instruction("sub x0, x0, #1");                                      // step back to the previous haystack byte
    emitter.instruction("ldrb w10, [x1, x0]");                                  // load the haystack byte at the candidate offset
    emitter.instruction("cmp w10, w9");                                         // is it the searched byte?
    emitter.instruction("b.ne __rt_strrchr_loop");                              // keep scanning backwards on a mismatch
    emitter.instruction("ret");                                                 // return the offset of the last occurrence

    emitter.label("__rt_strrchr_miss");
    emitter.instruction("mov x0, #-1");                                         // the byte does not occur in the haystack
    emitter.instruction("ret");                                                 // return the miss sentinel
}

/// Emits `__rt_strrchr` for x86_64 Linux using the System V argument registers.
fn emit_strrchr_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strrchr ---");
    emitter.label_global("__rt_strrchr");

    emitter.instruction("xor r8d, r8d");                                        // an empty needle searches the NUL byte
    emitter.instruction("test rcx, rcx");                                       // does the needle have a first byte?
    emitter.instruction("je __rt_strrchr_search_linux_x86_64");                 // keep NUL when the needle is empty
    emitter.instruction("movzx r8d, BYTE PTR [rdx]");                           // php-src searches only the needle's first byte

    emitter.label("__rt_strrchr_search_linux_x86_64");
    emitter.instruction("mov rax, rsi");                                        // scan backwards from one past the last haystack byte

    emitter.label("__rt_strrchr_loop_linux_x86_64");
    emitter.instruction("test rax, rax");                                       // has the whole haystack been scanned?
    emitter.instruction("je __rt_strrchr_miss_linux_x86_64");                   // the byte does not occur in the haystack
    emitter.instruction("sub rax, 1");                                          // step back to the previous haystack byte
    emitter.instruction("movzx r9d, BYTE PTR [rdi + rax]");                     // load the haystack byte at the candidate offset
    emitter.instruction("cmp r9d, r8d");                                        // is it the searched byte?
    emitter.instruction("jne __rt_strrchr_loop_linux_x86_64");                  // keep scanning backwards on a mismatch
    emitter.instruction("ret");                                                 // return the offset of the last occurrence

    emitter.label("__rt_strrchr_miss_linux_x86_64");
    emitter.instruction("mov rax, -1");                                         // the byte does not occur in the haystack
    emitter.instruction("ret");                                                 // return the miss sentinel
}
