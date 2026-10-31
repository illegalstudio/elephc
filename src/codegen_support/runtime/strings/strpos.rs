//! Purpose:
//! Emits the `__rt_strpos` runtime helper for PHP byte-string searches.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - Uses libc `memchr` for the next candidate first byte and `memcmp` to compare
//!   the full needle; libc owns the vectorized scans on every supported target.
//! - Empty needles match at offset zero, longer needles report the not-found
//!   sentinel, and a window that ends before the needle returns -1.
//! - The helper keeps a small stack frame because the libc calls clobber the
//!   caller-saved registers and the link register.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the `__rt_strpos` helper for the active target.
///
/// AArch64 input: x1/x2 = haystack pointer/length, x3/x4 = needle pointer/length.
/// Output: x0 = match offset, or -1 when the needle is absent.
pub fn emit_strpos(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_strpos_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: strpos ---");
    emitter.label_global("__rt_strpos");
    emitter.instruction("cbz x4, __rt_strpos_empty");                           // an empty needle matches at offset zero
    emitter.instruction("cmp x4, x2");                                          // reject needles longer than the haystack
    emitter.instruction("b.gt __rt_strpos_notfound");                           // a long needle cannot fit
    emitter.instruction("sub sp, sp, #80");                                     // reserve the libc-preserved search state
    emitter.instruction("stp x29, x30, [sp, #64]");                             // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #64");                                    // establish a stable frame base
    emitter.instruction("stp x1, x2, [sp, #16]");                               // save the haystack pointer and length
    emitter.instruction("stp x3, x4, [sp, #32]");                               // save the needle pointer and length
    emitter.instruction("sub x9, x2, x4");                                      // last valid candidate start = haystack length - needle length
    emitter.instruction("str x9, [sp, #0]");                                    // publish the last valid candidate
    emitter.instruction("mov x9, #0");                                          // start scanning at the window start
    emitter.instruction("str x9, [sp, #8]");                                    // publish the current candidate offset
    emitter.instruction("ldrb w9, [x3]");                                       // load the needle's first byte
    emitter.instruction("str x9, [sp, #48]");                                   // save the first-byte search key
    emitter.label("__rt_strpos_loop");
    emitter.instruction("ldr x1, [sp, #16]");                                   // reload the haystack pointer
    emitter.instruction("ldr x2, [sp, #24]");                                   // reload the haystack length
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the current candidate offset
    emitter.instruction("add x0, x1, x9");                                      // search from the next candidate byte
    emitter.instruction("sub x2, x2, x9");                                      // remaining haystack bytes from the candidate
    emitter.instruction("ldr w1, [sp, #48]");                                   // pass the cached first needle byte
    emitter.bl_c("memchr");                                                    // find the next first-byte candidate
    emitter.instruction("cbz x0, __rt_strpos_frame_notfound");                  // no first-byte candidate remains
    emitter.instruction("ldr x1, [sp, #16]");                                   // reload the haystack pointer for the offset
    emitter.instruction("sub x9, x0, x1");                                      // convert the candidate pointer into an offset
    emitter.instruction("str x9, [sp, #8]");                                    // publish the candidate offset
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the last valid candidate start
    emitter.instruction("cmp x9, x10");                                         // does the full needle still fit here?
    emitter.instruction("b.gt __rt_strpos_frame_notfound");                     // a candidate past the window cannot match
    emitter.instruction("add x0, x1, x9");                                      // compare the full needle at this candidate
    emitter.instruction("ldr x1, [sp, #32]");                                   // pass the needle pointer
    emitter.instruction("ldr x2, [sp, #40]");                                   // pass the needle length
    emitter.bl_c("memcmp");                                                    // compare the remaining needle bytes
    emitter.instruction("cbz w0, __rt_strpos_found");                           // a zero result is a full match
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the failed candidate offset
    emitter.instruction("add x9, x9, #1");                                      // advance to the next candidate
    emitter.instruction("str x9, [sp, #8]");                                    // publish the advanced candidate
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the last valid candidate start
    emitter.instruction("cmp x9, x10");                                         // is another candidate still in range?
    emitter.instruction("b.le __rt_strpos_loop");                               // continue while candidates remain
    emitter.label("__rt_strpos_frame_notfound");
    emitter.instruction("mov x0, #-1");                                         // return the not-found sentinel
    emitter.instruction("b __rt_strpos_frame_done");                            // restore the frame before returning
    emitter.label("__rt_strpos_found");
    emitter.instruction("ldr x0, [sp, #8]");                                    // return the matching candidate offset
    emitter.label("__rt_strpos_frame_done");
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #80");                                     // release the search frame
    emitter.instruction("ret");                                                 // return the match offset or -1
    emitter.label("__rt_strpos_empty");
    emitter.instruction("mov x0, #0");                                          // empty needles match at offset zero
    emitter.instruction("ret");                                                 // return the empty-needle offset
    emitter.label("__rt_strpos_notfound");
    emitter.instruction("mov x0, #-1");                                         // return the not-found sentinel
    emitter.instruction("ret");                                                 // return the not-found sentinel
}

/// Emits the x86_64 `__rt_strpos` helper.
///
/// SysV input: rdi/rsi = haystack pointer/length, rdx/rcx = needle pointer/length.
/// Output: rax = match offset, or -1 when the needle is absent.
fn emit_strpos_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strpos ---");
    emitter.label_global("__rt_strpos");
    emitter.instruction("test rcx, rcx");                                       // an empty needle matches at offset zero
    emitter.instruction("jz __rt_strpos_empty_linux_x86_64");                   // return zero for an empty needle
    emitter.instruction("cmp rcx, rsi");                                        // reject needles longer than the haystack
    emitter.instruction("jg __rt_strpos_notfound_linux_x86_64");                // a long needle cannot fit
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 64");                                         // reserve the libc-preserved search state
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the haystack pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the haystack length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the needle pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the needle length
    emitter.instruction("mov rax, rsi");                                        // copy the haystack length
    emitter.instruction("sub rax, rcx");                                        // last valid candidate start = haystack length - needle length
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // publish the last valid candidate
    emitter.instruction("xor eax, eax");                                        // start scanning at the window start
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // publish the current candidate offset
    emitter.instruction("movzx eax, BYTE PTR [rdx]");                           // load the needle's first byte
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save the first-byte search key
    emitter.label("__rt_strpos_loop_linux_x86_64");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the haystack pointer
    emitter.instruction("add rdi, QWORD PTR [rbp - 48]");                       // search from the current candidate byte
    emitter.instruction("mov esi, DWORD PTR [rbp - 56]");                       // pass the cached first needle byte
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the haystack length
    emitter.instruction("sub rdx, QWORD PTR [rbp - 48]");                       // remaining haystack bytes from the candidate
    emitter.bl_c("memchr");                                                    // find the next first-byte candidate
    emitter.instruction("test rax, rax");                                       // did memchr find another candidate?
    emitter.instruction("jz __rt_strpos_frame_notfound_linux_x86_64");          // no first-byte candidate remains
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the haystack pointer for the offset
    emitter.instruction("mov r11, rax");                                        // copy the candidate pointer
    emitter.instruction("sub r11, r10");                                        // convert the candidate pointer into an offset
    emitter.instruction("mov QWORD PTR [rbp - 48], r11");                       // publish the candidate offset
    emitter.instruction("cmp r11, QWORD PTR [rbp - 40]");                       // does the full needle still fit here?
    emitter.instruction("jg __rt_strpos_frame_notfound_linux_x86_64");          // a candidate past the window cannot match
    emitter.instruction("mov rdi, r10");                                        // compare the full needle at this candidate
    emitter.instruction("add rdi, r11");                                        // first memcmp argument = candidate bytes
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // pass the needle pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // pass the needle length
    emitter.bl_c("memcmp");                                                    // compare the remaining needle bytes
    emitter.instruction("test eax, eax");                                       // did the full needle compare equal?
    emitter.instruction("jz __rt_strpos_found_linux_x86_64");                   // a zero result is a full match
    emitter.instruction("add QWORD PTR [rbp - 48], 1");                         // advance to the next candidate
    emitter.instruction("mov r11, QWORD PTR [rbp - 48]");                       // reload the advanced candidate
    emitter.instruction("cmp r11, QWORD PTR [rbp - 40]");                       // is another candidate still in range?
    emitter.instruction("jle __rt_strpos_loop_linux_x86_64");                   // continue while candidates remain
    emitter.label("__rt_strpos_frame_notfound_linux_x86_64");
    emitter.instruction("mov rax, -1");                                         // return the not-found sentinel
    emitter.instruction("leave");                                               // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the not-found sentinel
    emitter.label("__rt_strpos_found_linux_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // return the matching candidate offset
    emitter.instruction("leave");                                               // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the match offset
    emitter.label("__rt_strpos_empty_linux_x86_64");
    emitter.instruction("xor eax, eax");                                        // empty needles match at offset zero
    emitter.instruction("ret");                                                 // return the empty-needle offset
    emitter.label("__rt_strpos_notfound_linux_x86_64");
    emitter.instruction("mov rax, -1");                                         // return the not-found sentinel
    emitter.instruction("ret");                                                 // return the not-found sentinel
}
