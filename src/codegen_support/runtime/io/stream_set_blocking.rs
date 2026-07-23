//! Purpose:
//! Emits the `__rt_stream_set_blocking` runtime helper assembly for the
//! stream_set_blocking builtin.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - Reads the descriptor flags with `fcntl(F_GETFL)`, toggles the
//!   target-specific `O_NONBLOCK` bit, and writes them back with
//!   `fcntl(F_SETFL)`; returns 1 on success and 0 on failure.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// stream_set_blocking: toggle the O_NONBLOCK flag on a descriptor.
/// Input:  x0 = fd, x1 = blocking flag (non-zero = blocking)
/// Output: x0 = 1 on success, 0 on failure
pub fn emit_stream_set_blocking(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stream_set_blocking_x86_64(emitter);
        return;
    }

    let plat = emitter.platform;
    emitter.blank();
    emitter.comment("--- runtime: stream_set_blocking ---");
    emitter.label_global("__rt_stream_set_blocking");

    emitter.instruction("sub sp, sp, #16");                                     // scratch for the descriptor and blocking flag
    emitter.instruction("str x0, [sp, #0]");                                    // save the file descriptor
    emitter.instruction("str x1, [sp, #8]");                                    // save the requested blocking flag

    // -- fcntl(fd, F_GETFL, 0) --
    emitter.instruction("mov x1, #3");                                          // F_GETFL
    emitter.instruction("mov x2, #0");                                          // unused third argument
    emitter.syscall(92);
    if plat.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: a negative result means failure
    }
    emitter.instruction(&plat.branch_on_syscall_success("__rt_stream_set_blocking_getfl_ok")); // continue when F_GETFL succeeded
    emitter.instruction("b __rt_stream_set_blocking_fail");                     // F_GETFL failed

    emitter.label("__rt_stream_set_blocking_getfl_ok");
    emitter.instruction(&format!("mov x9, #{}", plat.o_nonblock()));            // the O_NONBLOCK flag bit
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the requested blocking flag
    emitter.instruction("cbz x10, __rt_stream_set_blocking_nonblock");          // zero means non-blocking
    emitter.instruction("bic x0, x0, x9");                                      // blocking: clear the O_NONBLOCK bit
    emitter.instruction("b __rt_stream_set_blocking_setfl");                    // apply the updated flags
    emitter.label("__rt_stream_set_blocking_nonblock");
    emitter.instruction("orr x0, x0, x9");                                      // non-blocking: set the O_NONBLOCK bit

    // -- fcntl(fd, F_SETFL, flags) --
    emitter.label("__rt_stream_set_blocking_setfl");
    emitter.instruction("mov x2, x0");                                          // updated flags become the third argument
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the file descriptor
    emitter.instruction("mov x1, #4");                                          // F_SETFL
    emitter.syscall(92);
    if plat.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: a negative result means failure
    }
    emitter.instruction(&plat.branch_on_syscall_success("__rt_stream_set_blocking_ok")); // continue when F_SETFL succeeded
    emitter.instruction("b __rt_stream_set_blocking_fail");                     // F_SETFL failed

    emitter.label("__rt_stream_set_blocking_ok");
    emitter.instruction("mov x0, #1");                                          // success: report true
    emitter.instruction("add sp, sp, #16");                                     // release the scratch
    emitter.instruction("ret");                                                 // return the success result

    emitter.label("__rt_stream_set_blocking_fail");
    emitter.instruction("mov x0, #0");                                          // failure: report false
    emitter.instruction("add sp, sp, #16");                                     // release the scratch
    emitter.instruction("ret");                                                 // return the failure result
}

/// Emits the x86_64 stream runtime helper for toggling descriptor blocking mode.
///
/// Windows post-processing rewrites both fcntl syscalls to `__rt_sys_fcntl`,
/// which applies `FIONBIO` to sockets and retains cache-only semantics for CRT
/// descriptors such as the standard streams.
fn emit_stream_set_blocking_x86_64(emitter: &mut Emitter) {
    let plat = emitter.platform;
    emitter.blank();
    emitter.comment("--- runtime: stream_set_blocking ---");
    emitter.label_global("__rt_stream_set_blocking");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 16");                                         // scratch for the descriptor and blocking flag
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the file descriptor
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the requested blocking flag

    // -- fcntl(fd, F_GETFL, 0) --
    emitter.instruction("mov esi, 3");                                          // F_GETFL
    emitter.instruction("xor edx, edx");                                        // unused third argument
    emitter.instruction("mov eax, 72");                                         // Linux x86_64 syscall 72 = fcntl
    emitter.instruction("syscall");                                             // read the descriptor flags
    emitter.instruction("test rax, rax");                                       // did F_GETFL fail?
    emitter.instruction("js __rt_stream_set_blocking_fail_x86");                // F_GETFL failed

    emitter.instruction(&format!("mov r9d, {}", plat.o_nonblock()));            // the O_NONBLOCK flag bit
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the requested blocking flag
    emitter.instruction("test rcx, rcx");                                       // zero means non-blocking
    emitter.instruction("jz __rt_stream_set_blocking_nonblock_x86");            // branch to the non-blocking path
    emitter.instruction("not r9");                                              // invert the mask to clear O_NONBLOCK
    emitter.instruction("and rax, r9");                                         // blocking: clear the O_NONBLOCK bit
    emitter.instruction("jmp __rt_stream_set_blocking_setfl_x86");              // apply the updated flags
    emitter.label("__rt_stream_set_blocking_nonblock_x86");
    emitter.instruction("or rax, r9");                                          // non-blocking: set the O_NONBLOCK bit

    // -- fcntl(fd, F_SETFL, flags) --
    emitter.label("__rt_stream_set_blocking_setfl_x86");
    emitter.instruction("mov rdx, rax");                                        // updated flags become the third argument
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the file descriptor
    emitter.instruction("mov esi, 4");                                          // F_SETFL
    emitter.instruction("mov eax, 72");                                         // Linux x86_64 syscall 72 = fcntl
    emitter.instruction("syscall");                                             // write the descriptor flags
    emitter.instruction("test rax, rax");                                       // did F_SETFL fail?
    emitter.instruction("js __rt_stream_set_blocking_fail_x86");                // F_SETFL failed

    emitter.instruction("mov rax, 1");                                          // success: report true
    emitter.instruction("add rsp, 16");                                         // release the scratch
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the success result

    emitter.label("__rt_stream_set_blocking_fail_x86");
    emitter.instruction("mov rax, 0");                                          // failure: report false
    emitter.instruction("add rsp, 16");                                         // release the scratch
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the failure result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target, transform_for_windows};

    /// Verifies Windows rewrites both x86_64 fcntl operations through the shim
    /// that applies `FIONBIO` to sockets while preserving CRT descriptor state.
    #[test]
    fn test_stream_set_blocking_windows_uses_fcntl_shim() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_stream_set_blocking(&mut emitter);
        let asm = transform_for_windows(&emitter.output());
        assert_eq!(asm.matches("call __rt_sys_fcntl").count(), 2);
        assert!(asm.contains("mov r9d, 2048"), "missing Windows O_NONBLOCK mask");
        assert!(!asm.contains("syscall"), "Windows output must not retain raw fcntl syscalls");
    }

    /// Verifies that the Linux x86_64 dispatch is unaffected by the Windows
    /// branch and still routes through the `fcntl(F_GETFL/F_SETFL)` path.
    #[test]
    fn test_stream_set_blocking_linux_still_uses_fcntl() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_stream_set_blocking(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains(".globl __rt_stream_set_blocking\n"), "missing global label");
        assert!(asm.contains("mov eax, 72"), "Linux helper must still use the fcntl syscall number");
    }
}
