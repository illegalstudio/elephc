//! Purpose:
//! Emits the `__rt_popen` runtime helper, which opens a process pipe through
//! the libc `popen` call and exposes its underlying descriptor.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::io`.
//!
//! Key details:
//! - libc `popen` yields a `FILE*`; `fileno` recovers the raw descriptor so
//!   elephc's fd-based `fread`/`fwrite` work on the pipe.
//! - The `FILE*` is recorded in the `_popen_files` table keyed by descriptor
//!   so `pclose()` can hand it back to libc `pclose`.

use crate::codegen::{abi, emit::Emitter, platform::Arch};

/// popen: open a process pipe and return its descriptor.
/// Input:  AArch64 x1/x2 = command string, x3/x4 = mode string
///         x86_64  rdi/rsi = command string, rdx/rcx = mode string
/// Output: the pipe descriptor, or -1 on failure
pub fn emit_popen(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_popen_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: popen ---");
    emitter.label_global("__rt_popen");

    // Frame: [0..16) saved regs, [16) cmd cstr, [24) FILE*, [32..40) mode cstr,
    //        [40) mode ptr, [48) mode len.
    emitter.instruction("sub sp, sp, #64");                                     // frame for the popen state
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("str x3, [sp, #40]");                                   // save the mode string pointer
    emitter.instruction("str x4, [sp, #48]");                                   // save the mode string length
    emitter.instruction("bl __rt_cstr");                                        // null-terminate the command, x0 = C string
    emitter.instruction("str x0, [sp, #16]");                                   // save the command C-string pointer

    // -- build the mode C-string on the stack (clamped to 7 bytes) --
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload the mode string pointer
    emitter.instruction("ldr x10, [sp, #48]");                                  // reload the mode string length
    emitter.instruction("cmp x10, #7");                                         // clamp the mode length to the 7-byte buffer
    emitter.instruction("b.ls __rt_popen_mode_ok");                             // keep short mode strings as-is
    emitter.instruction("mov x10, #7");                                         // truncate an over-long mode string
    emitter.label("__rt_popen_mode_ok");
    emitter.instruction("add x12, sp, #32");                                    // mode C-string buffer base
    emitter.instruction("mov x11, #0");                                         // mode copy index
    emitter.label("__rt_popen_mode_copy");
    emitter.instruction("cmp x11, x10");                                        // copied every mode byte?
    emitter.instruction("b.hs __rt_popen_mode_done");                           // mode copy complete
    emitter.instruction("ldrb w13, [x9, x11]");                                 // load a mode byte
    emitter.instruction("strb w13, [x12, x11]");                                // store it into the buffer
    emitter.instruction("add x11, x11, #1");                                    // advance the copy index
    emitter.instruction("b __rt_popen_mode_copy");                              // keep copying the mode
    emitter.label("__rt_popen_mode_done");
    emitter.instruction("strb wzr, [x12, x11]");                                // NUL-terminate the mode string

    // -- popen(command, mode) --
    emitter.instruction("ldr x0, [sp, #16]");                                   // command C-string argument
    emitter.instruction("add x1, sp, #32");                                     // mode C-string argument
    emitter.bl_c("popen");
    emitter.instruction("cbz x0, __rt_popen_fail");                             // a NULL FILE* means popen failed
    emitter.instruction("str x0, [sp, #24]");                                   // save the FILE* across the fileno call

    // -- fileno(FILE*) recovers the raw descriptor --
    emitter.bl_c("fileno");
    emitter.instruction("mov w9, w0");                                          // x9 = the pipe descriptor

    // -- record the FILE* in the fd->FILE* table for pclose() --
    abi::emit_symbol_address(emitter, "x10", "_popen_files");
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload the FILE*
    emitter.instruction("str x11, [x10, x9, lsl #3]");                          // _popen_files[fd] = FILE*
    emitter.instruction("mov x0, x9");                                          // return the pipe descriptor
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the frame
    emitter.instruction("ret");                                                 // return the descriptor

    emitter.label("__rt_popen_fail");
    emitter.instruction("mov x0, #-1");                                         // -1 reports a popen failure
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the frame
    emitter.instruction("ret");                                                 // return the failure result
}

/// Emits the Linux x86_64 stream runtime helper for popen.
fn emit_popen_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: popen ---");
    emitter.label_global("__rt_popen");

    // Frame: [rbp-8) mode ptr, [rbp-16) mode len, [rbp-24) cmd cstr,
    //        [rbp-32) FILE*, [rbp-40..rbp-32) mode cstr buffer.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 48");                                         // frame for the popen state
    emitter.instruction("mov QWORD PTR [rbp - 8], rdx");                        // save the mode string pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rcx");                       // save the mode string length
    emitter.instruction("mov rax, rdi");                                        // command pointer into the cstr input register
    emitter.instruction("mov rdx, rsi");                                        // command length into the cstr input register
    emitter.instruction("call __rt_cstr");                                      // null-terminate the command, rax = C string
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the command C-string pointer

    // -- build the mode C-string on the stack (clamped to 7 bytes) --
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload the mode string pointer
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload the mode string length
    emitter.instruction("cmp r9, 7");                                           // clamp the mode length to the 7-byte buffer
    emitter.instruction("jbe __rt_popen_mode_ok_x86");                          // keep short mode strings as-is
    emitter.instruction("mov r9, 7");                                           // truncate an over-long mode string
    emitter.label("__rt_popen_mode_ok_x86");
    emitter.instruction("lea r10, [rbp - 40]");                                 // mode C-string buffer base
    emitter.instruction("xor rcx, rcx");                                        // mode copy index
    emitter.label("__rt_popen_mode_copy_x86");
    emitter.instruction("cmp rcx, r9");                                         // copied every mode byte?
    emitter.instruction("jae __rt_popen_mode_done_x86");                        // mode copy complete
    emitter.instruction("movzx eax, BYTE PTR [r8 + rcx]");                      // load a mode byte
    emitter.instruction("mov BYTE PTR [r10 + rcx], al");                        // store it into the buffer
    emitter.instruction("inc rcx");                                             // advance the copy index
    emitter.instruction("jmp __rt_popen_mode_copy_x86");                        // keep copying the mode
    emitter.label("__rt_popen_mode_done_x86");
    emitter.instruction("mov BYTE PTR [r10 + rcx], 0");                         // NUL-terminate the mode string

    // -- popen(command, mode) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // command C-string argument
    emitter.instruction("lea rsi, [rbp - 40]");                                 // mode C-string argument
    emitter.bl_c("popen");
    emitter.instruction("test rax, rax");                                       // a NULL FILE* means popen failed
    emitter.instruction("jz __rt_popen_fail_x86");                              // bail out on a popen failure
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the FILE* across the fileno call

    // -- fileno(FILE*) recovers the raw descriptor --
    emitter.instruction("mov rdi, rax");                                        // FILE* argument for fileno
    emitter.bl_c("fileno");
    emitter.instruction("mov r9d, eax");                                        // r9 = the pipe descriptor

    // -- record the FILE* in the fd->FILE* table for pclose() --
    abi::emit_symbol_address(emitter, "r10", "_popen_files");                   // base of the fd->FILE* table
    emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                       // reload the FILE*
    emitter.instruction("mov QWORD PTR [r10 + r9 * 8], r11");                   // _popen_files[fd] = FILE*
    emitter.instruction("mov rax, r9");                                         // return the pipe descriptor
    emitter.instruction("add rsp, 48");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the descriptor

    emitter.label("__rt_popen_fail_x86");
    emitter.instruction("mov rax, -1");                                         // -1 reports a popen failure
    emitter.instruction("add rsp, 48");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the failure result
}
