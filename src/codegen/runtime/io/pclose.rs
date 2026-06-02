//! Purpose:
//! Emits the `__rt_pclose` runtime helper, which closes a process pipe opened
//! by `popen()` and waits for the child process.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::io`.
//!
//! Key details:
//! - The `FILE*` recorded by `__rt_popen` in `_popen_files` is handed back to
//!   libc `pclose`, which closes the stream and reaps the child.
//! - A descriptor with no recorded `FILE*` is closed directly as a fallback.

use crate::codegen::{abi, emit::Emitter, platform::Arch};

/// pclose: close a process pipe and return the child termination status.
/// Input:  AArch64 x0 = pipe descriptor / x86_64 rdi = pipe descriptor
/// Output: the child process termination status
pub fn emit_pclose(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_pclose_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: pclose ---");
    emitter.label_global("__rt_pclose");

    emitter.instruction("sub sp, sp, #16");                                     // minimal frame for the libc call
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer

    // -- look up the FILE* recorded for this descriptor --
    abi::emit_symbol_address(emitter, "x9", "_popen_files");
    emitter.instruction("ldr x10, [x9, x0, lsl #3]");                           // FILE* = _popen_files[fd]
    emitter.instruction("cbz x10, __rt_pclose_plain");                          // no FILE* recorded: close directly
    emitter.instruction("str xzr, [x9, x0, lsl #3]");                           // clear the fd->FILE* table entry
    emitter.instruction("mov x0, x10");                                         // FILE* argument for pclose
    emitter.bl_c("pclose");
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the frame
    emitter.instruction("ret");                                                 // return the termination status

    emitter.label("__rt_pclose_plain");
    emitter.syscall(6);
    emitter.instruction("mov x0, #0");                                          // report a zero status for a plain close
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the frame
    emitter.instruction("ret");                                                 // return the zero status
}

fn emit_pclose_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: pclose ---");
    emitter.label_global("__rt_pclose");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer

    // -- look up the FILE* recorded for this descriptor --
    emitter.instruction("lea r9, [rip + _popen_files]");                        // base of the fd->FILE* table
    emitter.instruction("mov r10, QWORD PTR [r9 + rdi * 8]");                   // FILE* = _popen_files[fd]
    emitter.instruction("test r10, r10");                                       // was a FILE* recorded?
    emitter.instruction("jz __rt_pclose_plain_x86");                            // no FILE* recorded: close directly
    emitter.instruction("mov QWORD PTR [r9 + rdi * 8], 0");                     // clear the fd->FILE* table entry
    emitter.instruction("mov rdi, r10");                                        // FILE* argument for pclose
    emitter.bl_c("pclose");
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the termination status

    emitter.label("__rt_pclose_plain_x86");
    emitter.instruction("mov eax, 3");                                          // Linux x86_64 syscall 3 = close
    emitter.instruction("syscall");                                             // close the descriptor directly
    emitter.instruction("xor eax, eax");                                        // report a zero status for a plain close
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the zero status
}
