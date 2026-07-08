//! Purpose:
//! Emits the `__rt_stream_set_timeout` runtime helper, which applies a read
//! timeout to a socket descriptor through `setsockopt(SO_RCVTIMEO)`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - Builds a 16-byte `timeval` on the stack; the microseconds field is
//!   stored as a full 8-byte word, which is correct for Linux and leaves the
//!   macOS `timeval` padding zeroed.
//! - Returns 1 when `setsockopt` succeeds (a socket descriptor) and 0
//!   otherwise (for example a plain file descriptor).

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// stream_set_timeout: set the receive timeout on a socket descriptor.
/// Input:  AArch64 x0 = fd, x1 = seconds, x2 = microseconds
///         x86_64  rdi = fd, rsi = seconds, rdx = microseconds
/// Output: 1 on success, 0 on failure
pub fn emit_stream_set_timeout(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stream_set_timeout_linux_x86_64(emitter);
        return;
    }

    let plat = emitter.platform;
    emitter.blank();
    emitter.comment("--- runtime: stream_set_timeout ---");
    emitter.label_global("__rt_stream_set_timeout");

    // -- build the timeval on the stack --
    emitter.instruction("sub sp, sp, #16");                                     // reserve a 16-byte timeval
    emitter.instruction("str x1, [sp, #0]");                                    // tv_sec = seconds
    emitter.instruction("str x2, [sp, #8]");                                    // tv_usec = microseconds

    // -- setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &tv, 16) --
    emitter.instruction(&format!("mov x1, #{}", plat.sol_socket()));            // SOL_SOCKET option level
    emitter.instruction(&format!("mov x2, #{}", plat.so_rcvtimeo()));           // SO_RCVTIMEO option name
    emitter.instruction("mov x3, sp");                                          // pointer to the timeval
    emitter.instruction("mov x4, #16");                                         // option length = sizeof(timeval)
    emitter.syscall(105);
    if plat.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: a negative result means failure
    }
    emitter.instruction(&plat.branch_on_syscall_success("__rt_stream_set_timeout_ok")); // continue when setsockopt succeeded
    emitter.instruction("mov x0, #0");                                          // setsockopt failed: report false
    emitter.instruction("b __rt_stream_set_timeout_ret");                       // return the failure result
    emitter.label("__rt_stream_set_timeout_ok");
    emitter.instruction("mov x0, #1");                                          // setsockopt succeeded: report true
    emitter.label("__rt_stream_set_timeout_ret");
    emitter.instruction("add sp, sp, #16");                                     // release the timeval
    emitter.instruction("ret");                                                 // return the boolean result
}

/// Emits the Linux x86_64 stream runtime helper for stream set timeout.
fn emit_stream_set_timeout_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_set_timeout ---");
    emitter.label_global("__rt_stream_set_timeout");

    // -- build the timeval on the stack --
    emitter.instruction("sub rsp, 16");                                         // reserve a 16-byte timeval
    emitter.instruction("mov QWORD PTR [rsp], rsi");                            // tv_sec = seconds
    emitter.instruction("mov QWORD PTR [rsp + 8], rdx");                        // tv_usec = microseconds

    // -- setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &tv, 16) --
    emitter.instruction("mov esi, 1");                                          // SOL_SOCKET option level
    emitter.instruction("mov edx, 20");                                         // SO_RCVTIMEO option name
    emitter.instruction("mov r10, rsp");                                        // pointer to the timeval
    emitter.instruction("mov r8d, 16");                                         // option length = sizeof(timeval)
    emitter.instruction("mov eax, 54");                                         // Linux x86_64 syscall 54 = setsockopt
    emitter.instruction("syscall");                                             // apply the socket receive timeout
    emitter.instruction("cmp rax, 0");                                          // did setsockopt fail?
    emitter.instruction("jl __rt_stream_set_timeout_fail");                     // a negative result means failure
    emitter.instruction("mov eax, 1");                                          // setsockopt succeeded: report true
    emitter.instruction("add rsp, 16");                                         // release the timeval
    emitter.instruction("ret");                                                 // return the boolean result
    emitter.label("__rt_stream_set_timeout_fail");
    emitter.instruction("xor eax, eax");                                        // setsockopt failed: report false
    emitter.instruction("add rsp, 16");                                         // release the timeval
    emitter.instruction("ret");                                                 // return the boolean result
}
