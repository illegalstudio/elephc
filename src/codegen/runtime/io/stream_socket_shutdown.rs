//! Purpose:
//! Emits the `__rt_stream_socket_shutdown` runtime helper assembly for the
//! stream_socket_shutdown builtin.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::io`.
//!
//! Key details:
//! - Calls `shutdown(fd, how)` and normalizes the result to 1 (success) or 0.

use crate::codegen::{emit::Emitter, platform::Arch};

/// stream_socket_shutdown: disable reception and/or transmission on a socket.
/// Input:  x0 = fd, x1 = how (0 = read, 1 = write, 2 = both)
/// Output: x0 = 1 on success, 0 on failure
pub fn emit_stream_socket_shutdown(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stream_socket_shutdown_linux_x86_64(emitter);
        return;
    }

    let plat = emitter.platform;
    emitter.blank();
    emitter.comment("--- runtime: stream_socket_shutdown ---");
    emitter.label_global("__rt_stream_socket_shutdown");

    emitter.syscall(134);
    if plat.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: a negative result means failure
    }
    emitter.instruction(&plat.branch_on_syscall_success("__rt_stream_socket_shutdown_ok")); // branch on success
    emitter.instruction("mov x0, #0");                                          // shutdown() failed: report false
    emitter.instruction("ret");                                                 // return the failure result
    emitter.label("__rt_stream_socket_shutdown_ok");
    emitter.instruction("mov x0, #1");                                          // shutdown() succeeded: report true
    emitter.instruction("ret");                                                 // return the success result
}

fn emit_stream_socket_shutdown_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_socket_shutdown ---");
    emitter.label_global("__rt_stream_socket_shutdown");

    emitter.instruction("mov eax, 48");                                         // Linux x86_64 syscall 48 = shutdown
    emitter.instruction("syscall");                                             // shut down the socket
    emitter.instruction("test rax, rax");                                       // did shutdown() fail?
    emitter.instruction("js __rt_stream_socket_shutdown_fail_x86");             // a negative result means failure
    emitter.instruction("mov eax, 1");                                          // shutdown() succeeded: report true
    emitter.instruction("ret");                                                 // return the success result
    emitter.label("__rt_stream_socket_shutdown_fail_x86");
    emitter.instruction("xor eax, eax");                                        // shutdown() failed: report false
    emitter.instruction("ret");                                                 // return the failure result
}
