//! Purpose:
//! Emits `__rt_fd_write`, a thin "write these bytes to a descriptor" dispatcher
//! that routes a synthetic userspace-wrapper fd (`>= 0x40000000`) into the
//! wrapper's `stream_write` and a normal fd straight to the `write` syscall.
//! Lets descriptor-writing builtins that emit raw `write` syscalls (e.g.
//! `fputcsv`) transparently support userspace wrappers.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via
//!   `crate::codegen::runtime::io`.
//! - `__rt_fputcsv`'s field/separator/quote/newline write sites.
//!
//! Key details:
//! - Same ABI as a bare `write(fd, buf, len)`: fd/buf/len in x0/x1/x2 (AArch64)
//!   or rdi/rsi/rdx (x86_64); returns the byte count in x0/rax. A wrapper fd
//!   tail-calls `__rt_user_wrapper_fwrite` (so its return / register contract is
//!   reused unchanged); a normal fd issues the platform `write` syscall.
//! - Leaf helper (no frame): the raw path syscalls then `ret`; the wrapper path
//!   tail-branches, preserving the caller's return address.

use crate::codegen::{emit::Emitter, platform::Arch};

/// Emits `__rt_fd_write(fd, buf, len) -> bytes_written`, dispatching synthetic
/// wrapper descriptors to `__rt_user_wrapper_fwrite` and normal descriptors to
/// the `write` syscall.
pub fn emit_fd_write(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_fd_write_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: fd_write ---");
    emitter.label_global("__rt_fd_write");
    emitter.instruction("mov w9, #0x4000");                                     // load the high half of USER_WRAPPER_FD_BASE = 0x40000000
    emitter.instruction("lsl w9, w9, #16");                                     // shift into bits 30..16 to form 0x40000000
    emitter.instruction("cmp x0, x9");                                          // is this a synthetic user-wrapper fd?
    emitter.instruction("b.ge __rt_user_wrapper_fwrite");                       // wrapper: tail-call stream_write (x0=fd, x1=ptr, x2=len)
    emitter.syscall(4);                                                         // normal fd: write(fd, buf, len) → x0 = bytes written
    emitter.instruction("ret");                                                 // return the byte count
}

fn emit_fd_write_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fd_write ---");
    emitter.label_global("__rt_fd_write");
    emitter.instruction("mov r9d, 0x40000000");                                 // USER_WRAPPER_FD_BASE
    emitter.instruction("cmp rdi, r9");                                         // is this a synthetic user-wrapper fd?
    emitter.instruction("jge __rt_user_wrapper_fwrite");                        // wrapper: tail-call stream_write (rdi=fd, rsi=ptr, rdx=len)
    emitter.instruction("mov eax, 1");                                          // syscall 1 = write on Linux x86_64
    emitter.instruction("syscall");                                             // normal fd: write(fd, buf, len) → rax = bytes written
    emitter.instruction("ret");                                                 // return the byte count
}
