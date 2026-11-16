//! Purpose:
//! Emits `__rt_strstr_len`, the allocation-free `strlen(strstr(...))` runtime helper.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - Calls the existing `__rt_strpos` candidate search, then computes either the prefix
//!   length (`$before_needle` truthy) or the suffix length without allocating or boxing a
//!   `string|false` cell.
//! - A missing needle maps to PHP's `strlen(false)` result, zero.
//! - ABI: AArch64 x1/x2 = haystack, x3/x4 = needle, x5 = before flag; x86_64
//!   rdi/rsi = haystack, rdx/rcx = needle, r8 = before flag. The result is the integer
//!   length in x0/rax.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits the `__rt_strstr_len` helper for the active target.
pub fn emit_strstr_len(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_strstr_len_x86_64(emitter);
        return;
    }
    emit_strstr_len_aarch64(emitter);
}

/// Emits the AArch64 `__rt_strstr_len` helper.
fn emit_strstr_len_aarch64(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: strstr_len ---");
    e.label_global("__rt_strstr_len");
    e.instruction("sub sp, sp, #64");                                           // reserve the haystack, needle, and flag slots
    e.instruction("stp x29, x30, [sp, #48]");                                   // preserve the caller frame and return address
    e.instruction("add x29, sp, #48");                                          // establish a stable frame base
    e.instruction("stp x1, x2, [sp, #0]");                                      // save the haystack pointer and length
    e.instruction("stp x3, x4, [sp, #16]");                                     // save the needle pointer and length
    e.instruction("str x5, [sp, #32]");                                         // save the before-needle flag
    abi::emit_call_label(e, "__rt_strpos");                                      // find the first needle occurrence
    e.instruction("ldp x1, x2, [sp, #0]");                                      // restore the haystack pointer and length
    e.instruction("ldr x5, [sp, #32]");                                         // restore the before-needle flag
    e.instruction("cmp x0, #0");                                                // did the search find the needle?
    e.instruction("b.lt __rt_strstr_len_miss");                                 // a missing needle makes strlen(false) zero
    e.instruction("cbz x5, __rt_strstr_len_suffix");                            // the default form returns the suffix length
    e.instruction("b __rt_strstr_len_done");                                    // the prefix length is already the match offset
    e.label("__rt_strstr_len_suffix");
    e.instruction("sub x0, x2, x0");                                            // suffix length = haystack length - match offset
    e.instruction("b __rt_strstr_len_done");                                    // restore the frame and return
    e.label("__rt_strstr_len_miss");
    e.instruction("mov x0, xzr");                                               // strlen(false) is zero
    e.label("__rt_strstr_len_done");
    e.instruction("ldp x29, x30, [sp, #48]");                                   // restore the caller frame and return address
    e.instruction("add sp, sp, #64");                                           // release the helper frame
    e.instruction("ret");                                                       // return the selected substring length
}

/// Emits the x86_64 SysV `__rt_strstr_len` helper.
fn emit_strstr_len_x86_64(e: &mut Emitter) {
    e.blank();
    e.comment("--- runtime: strstr_len ---");
    e.label_global("__rt_strstr_len");
    e.instruction("push rbp");                                                  // preserve the caller frame pointer
    e.instruction("mov rbp, rsp");                                              // establish a stable frame base
    e.instruction("sub rsp, 32");                                               // reserve the haystack and flag slots
    e.instruction("mov QWORD PTR [rbp - 8], rdi");                              // save the haystack pointer
    e.instruction("mov QWORD PTR [rbp - 16], rsi");                             // save the haystack length
    e.instruction("mov QWORD PTR [rbp - 24], r8");                              // save the before-needle flag
    abi::emit_call_label(e, "__rt_strpos");                                      // find the first needle occurrence
    e.instruction("mov rdx, QWORD PTR [rbp - 16]");                             // restore the haystack length
    e.instruction("cmp rax, 0");                                                // did the search find the needle?
    e.instruction("jl __rt_strstr_len_miss_x86_64");                            // a missing needle makes strlen(false) zero
    e.instruction("cmp QWORD PTR [rbp - 24], 0");                               // was the before-needle flag truthy?
    e.instruction("jz __rt_strstr_len_suffix_x86_64");                          // the default form returns the suffix length
    e.instruction("jmp __rt_strstr_len_done_x86_64");                           // the prefix length is already the match offset
    e.label("__rt_strstr_len_suffix_x86_64");
    e.instruction("sub rdx, rax");                                              // suffix length = haystack length - match offset
    e.instruction("mov rax, rdx");                                              // carry the suffix length in the result register
    e.instruction("jmp __rt_strstr_len_done_x86_64");                           // restore the frame and return
    e.label("__rt_strstr_len_miss_x86_64");
    e.instruction("xor eax, eax");                                              // strlen(false) is zero
    e.label("__rt_strstr_len_done_x86_64");
    e.instruction("leave");                                                     // restore the caller frame pointer
    e.instruction("ret");                                                       // return the selected substring length
}
