//! Purpose:
//! Emits the `__rt_stream_set_timeout` runtime helper, which applies read and
//! write timeouts to a socket descriptor through `setsockopt`.
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

/// Emits the native stream timeout setter for the active target ABI.
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
    if emitter.target.platform == crate::codegen_support::platform::Platform::Windows {
        emit_stream_set_timeout_windows_x86_64(emitter);
        return;
    }
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

/// Emits the WinSock receive/send timeout setter and clears stale timeout metadata.
fn emit_stream_set_timeout_windows_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_set_timeout ---");
    emitter.label_global("__rt_stream_set_timeout");
    emitter.instruction("sub rsp, 40");                                         // preserve a POSIX timeval and opaque descriptor across two shim calls
    emitter.instruction("mov QWORD PTR [rsp + 24], rdi");                       // preserve the full-width SOCKET across both option updates
    emitter.instruction("mov QWORD PTR [rsp], rsi");                            // timeval.tv_sec = PHP seconds
    emitter.instruction("mov QWORD PTR [rsp + 8], rdx");                        // timeval.tv_usec = PHP microseconds
    emitter.instruction("mov rsi, 1");                                          // POSIX SOL_SOCKET, translated by the WinSock shim
    emitter.instruction("mov rdx, 20");                                         // POSIX SO_RCVTIMEO, translated by the WinSock shim
    emitter.instruction("mov r10, rsp");                                        // pass the POSIX timeval to the shim for millisecond conversion
    emitter.instruction("mov r8, 16");                                          // sizeof(timeval)
    emitter.instruction("call __rt_sys_setsockopt");                            // configure receive timeout through the audited WinSock ABI shim
    emitter.instruction("test rax, rax");                                       // shim returns zero on success and negative on failure
    emitter.instruction("jnz __rt_stream_set_timeout_fail_win");                // receiving timeout must succeed before reporting true
    emitter.instruction("mov rdi, QWORD PTR [rsp + 24]");                       // restore the opaque SOCKET for the send timeout update
    emitter.instruction("mov rsi, 1");                                          // POSIX SOL_SOCKET, translated by the WinSock shim
    emitter.instruction("mov rdx, 21");                                         // POSIX SO_SNDTIMEO, translated by the WinSock shim
    emitter.instruction("mov r10, rsp");                                        // reuse the POSIX timeval for millisecond conversion
    emitter.instruction("mov r8, 16");                                          // sizeof(timeval)
    emitter.instruction("call __rt_sys_setsockopt");                            // configure send timeout through the audited WinSock ABI shim
    emitter.instruction("test rax, rax");                                       // both socket options must succeed
    emitter.instruction("jnz __rt_stream_set_timeout_fail_win");                // partial configuration reports false
    emitter.instruction("mov rdi, QWORD PTR [rsp + 24]");                       // resolve this stream's compact metadata slot
    emitter.instruction("call __rt_win_stream_slot");                           // opaque SOCKETs must never index metadata directly
    crate::codegen_support::abi::emit_symbol_address(emitter, "r10", "_win_stream_timed_out"); // timeout state table base
    emitter.instruction("mov BYTE PTR [r10 + rax], 0");                         // a new timeout configuration clears stale timeout state
    emitter.instruction("mov eax, 1");                                          // both socket options succeeded
    emitter.instruction("add rsp, 40");                                         // release the timeout frame
    emitter.instruction("ret");                                                 // return PHP true
    emitter.label("__rt_stream_set_timeout_fail_win");
    emitter.instruction("xor eax, eax");                                        // either socket-option failure reports PHP false
    emitter.instruction("add rsp, 40");                                         // release the timeout frame
    emitter.instruction("ret");                                                 // return PHP false
}
