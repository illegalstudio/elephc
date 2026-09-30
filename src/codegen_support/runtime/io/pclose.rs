//! Purpose:
//! Emits the `__rt_pclose` runtime helper, which closes a process pipe opened
//! by `popen()` and waits for the child process.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - The owning `FILE*` comes from `StreamState.backend_aux`, never from an
//!   fd-indexed side table.
//! - libc returns a wait status; the helper decodes normal child exits to the
//!   PHP-visible exit code returned by `pclose()`.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// pclose: close a process pipe and return the child termination status.
/// Input:  AArch64 x0 = owning FILE* / x86_64 rdi = owning FILE*
/// Output: the PHP child exit status, or -1 when libc pclose fails.
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

    emitter.instruction("cbz x0, __rt_pclose_fail");                            // a missing FILE* cannot own a child process
    emitter.bl_c("pclose");
    emitter.instruction("cmp w0, #-1");                                         // did libc fail to close or wait?
    emitter.instruction("b.eq __rt_pclose_done");                               // preserve the -1 failure sentinel
    emitter.instruction("and w9, w0, #0x7f");                                   // isolate the POSIX terminating-signal field
    emitter.instruction("cbnz w9, __rt_pclose_done");                           // preserve signalled-child status as returned by libc
    emitter.instruction("lsr w0, w0, #8");                                      // extract the normal child exit code
    emitter.instruction("and w0, w0, #0xff");                                   // constrain the PHP-visible exit code to one byte
    emitter.instruction("b __rt_pclose_done");                                  // join the helper epilogue
    emitter.label("__rt_pclose_fail");
    emitter.instruction("mov x0, #-1");                                         // report an invalid process-pipe owner
    emitter.label("__rt_pclose_done");
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the frame
    emitter.instruction("ret");                                                 // return the termination status
}

/// Emits the Linux x86_64 stream runtime helper for pclose.
fn emit_pclose_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: pclose ---");
    emitter.label_global("__rt_pclose");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer

    emitter.instruction("test rdi, rdi");                                       // is an owning FILE* available?
    emitter.instruction("jz __rt_pclose_fail_x86");                             // reject missing process-pipe state
    emitter.bl_c("pclose");
    emitter.instruction("cmp eax, -1");                                         // did libc fail to close or wait?
    emitter.instruction("je __rt_pclose_done_x86");                             // preserve the -1 failure sentinel
    emitter.instruction("mov ecx, eax");                                        // preserve the raw wait status for signal inspection
    emitter.instruction("and ecx, 0x7f");                                       // isolate the POSIX terminating-signal field
    emitter.instruction("jnz __rt_pclose_done_x86");                            // preserve signalled-child status as returned by libc
    emitter.instruction("shr eax, 8");                                          // extract the normal child exit code
    emitter.instruction("and eax, 0xff");                                       // constrain the PHP-visible exit code to one byte
    emitter.instruction("jmp __rt_pclose_done_x86");                            // join the helper epilogue
    emitter.label("__rt_pclose_fail_x86");
    emitter.instruction("mov eax, -1");                                         // report an invalid process-pipe owner
    emitter.label("__rt_pclose_done_x86");
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the termination status
}
