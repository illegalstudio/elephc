//! Purpose:
//! Emits `__rt_socket_connect_warning`, the diagnostic PHP prints when a socket builtin cannot
//! open its endpoint.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - The `stream_socket_client()`, `stream_socket_server()` and `fsockopen()` lowerings, on the
//!   failing branch.
//!
//! Key details:
//! - PHP warns on every failed open, whether or not the caller passed `&$errno`/`&$errstr`; only
//!   `@` suppresses it. elephc filled the out-parameters but printed nothing at all, so a script
//!   that relied on the warning to notice a dead endpoint saw a silent `false`.
//! - The line is written in fragments through `__rt_diag_warning`, which is what honours `@`,
//!   because the address and the reason are only known at run time.
//! - `fsockopen()` passes a bare host and a port, since PHP spells its address `host:port`; the
//!   other two pass the address the caller wrote, which already carries any port.
//! - Which wording carries a port is decided by the KIND, not by the port VALUE. php's fsockopen
//!   template is `Unable to connect to %s:%d`, so the port is printed whatever it is — including
//!   the -1 that `fsockopen("unix:///tmp/s.sock")` leaves it at, which php spells `...s.sock:-1`.
//!   Testing the value instead dropped the suffix for exactly that call.

use super::socket_errno::SOCKET_ERRNO_SYMBOL;
use crate::codegen_support::runtime::data::{
    SOCKET_FAILED_CLIENT_PREFIX, SOCKET_FAILED_FSOCKOPEN_PREFIX, SOCKET_FAILED_REASON_CLOSE,
    SOCKET_FAILED_REASON_OPEN, SOCKET_FAILED_REASON_UNKNOWN, SOCKET_FAILED_SERVER_PREFIX,
    SOCKET_FAILED_UNABLE,
};
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// The `kind` selector for the `stream_socket_client()` wording.
pub(crate) const SOCKET_WARNING_CLIENT: i64 = 0;

/// The `kind` selector for the `stream_socket_server()` wording.
pub(crate) const SOCKET_WARNING_SERVER: i64 = 1;

/// The `kind` selector for the `fsockopen()` wording.
pub(crate) const SOCKET_WARNING_FSOCKOPEN: i64 = 2;

/// Emits `__rt_socket_connect_warning(kind, addr_ptr, addr_len, port)`.
///
/// AArch64 receives `x0`/`x1`/`x2`/`x3`; x86_64 receives `rdi`/`rsi`/`rdx`/`rcx`. Only the
/// `fsockopen()` wording carries a `:port` suffix. The reason comes from the error number the socket helpers publish at
/// the syscall that failed, described by libc `strerror` — the same source as `&$errstr`.
pub fn emit_socket_connect_warning(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_socket_connect_warning_aarch64(emitter),
        Arch::X86_64 => emit_socket_connect_warning_x86_64(emitter),
    }
}

/// Emits the AArch64 socket-failure warning.
fn emit_socket_connect_warning_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: socket open failure warning ---");
    emitter.label_global("__rt_socket_connect_warning");
    emitter.instruction("sub sp, sp, #64");                                     // frame for the address pair, the port, the head and the saved linkage
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // preserve the address pointer
    emitter.instruction("str x2, [sp, #8]");                                    // preserve the address byte length
    emitter.instruction("str x3, [sp, #16]");                                   // preserve the port, if any

    emitter.instruction(&format!("cmp x0, #{SOCKET_WARNING_SERVER}"));          // which builtin is reporting?
    emitter.instruction("b.eq __rt_scw_server");
    emitter.instruction(&format!("cmp x0, #{SOCKET_WARNING_FSOCKOPEN}"));
    emitter.instruction("b.eq __rt_scw_fsockopen");
    emitter.instruction("str xzr, [sp, #40]");                                  // this wording names no port
    abi::emit_symbol_address(emitter, "x1", "_sock_fail_client");
    emitter.instruction(&format!("mov x2, #{}", SOCKET_FAILED_CLIENT_PREFIX.len()));
    emitter.instruction("b __rt_scw_prefix_ready");
    emitter.label("__rt_scw_server");
    emitter.instruction("str xzr, [sp, #40]");                                  // nor does this one
    abi::emit_symbol_address(emitter, "x1", "_sock_fail_server");
    emitter.instruction(&format!("mov x2, #{}", SOCKET_FAILED_SERVER_PREFIX.len()));
    emitter.instruction("b __rt_scw_prefix_ready");
    emitter.label("__rt_scw_fsockopen");
    emitter.instruction("mov x9, #1");                                          // php's fsockopen template is `%s:%d`
    emitter.instruction("str x9, [sp, #40]");                                   // so the port is printed whatever it is
    abi::emit_symbol_address(emitter, "x1", "_sock_fail_fsockopen");
    emitter.instruction(&format!("mov x2, #{}", SOCKET_FAILED_FSOCKOPEN_PREFIX.len()));
    emitter.label("__rt_scw_prefix_ready");
    emitter.instruction("str x1, [sp, #24]");                                   // remember the head, both lines carry it
    emitter.instruction("str x2, [sp, #32]");                                   // and its length

    // A resolution failure is two warnings in PHP: the resolver's own message under the same head,
    // then the connect line that repeats it as the reason.
    abi::emit_symbol_address(emitter, "x9", "_socket_gai_msg_len");
    emitter.instruction("ldr x9, [x9]");                                        // did a resolution fail?
    emitter.instruction("cbz x9, __rt_scw_no_resolver_line");
    emitter.instruction("bl __rt_diag_warning");                                // warnings honour the @ suppression depth
    abi::emit_symbol_address(emitter, "x1", "_socket_gai_msg");
    abi::emit_symbol_address(emitter, "x2", "_socket_gai_msg_len");
    emitter.instruction("ldr x2, [x2]");                                        // the composed resolver message
    emitter.instruction("bl __rt_diag_warning");
    abi::emit_symbol_address(emitter, "x1", "_sock_fail_newline");
    emitter.instruction("mov x2, #1");                                          // close the resolver's own line
    emitter.instruction("bl __rt_diag_warning");
    emitter.instruction("ldr x1, [sp, #24]");                                   // reload the head for the connect line
    emitter.instruction("ldr x2, [sp, #32]");
    emitter.label("__rt_scw_no_resolver_line");
    emitter.instruction("bl __rt_diag_warning");                                // warnings honour the @ suppression depth
    abi::emit_symbol_address(emitter, "x1", "_sock_fail_unable");
    emitter.instruction(&format!("mov x2, #{}", SOCKET_FAILED_UNABLE.len()));
    emitter.instruction("bl __rt_diag_warning");

    emitter.instruction("ldr x1, [sp, #0]");                                    // the address the caller wrote
    emitter.instruction("ldr x2, [sp, #8]");                                    // and its byte length
    emitter.instruction("bl __rt_diag_warning");

    emitter.instruction("ldr x9, [sp, #40]");                                   // does this wording name a port?
    emitter.instruction("cbz x9, __rt_scw_no_port");                            // the other two put it in the address instead
    abi::emit_symbol_address(emitter, "x1", "_sock_fail_colon");
    emitter.instruction("mov x2, #1");
    emitter.instruction("bl __rt_diag_warning");
    emitter.instruction("ldr x0, [sp, #16]");                                   // convert the port to decimal
    emitter.instruction("bl __rt_itoa");                                        // x1 = digits, x2 = length
    emitter.instruction("bl __rt_diag_warning");
    emitter.label("__rt_scw_no_port");

    abi::emit_symbol_address(emitter, "x1", "_sock_fail_open");
    emitter.instruction(&format!("mov x2, #{}", SOCKET_FAILED_REASON_OPEN.len()));
    emitter.instruction("bl __rt_diag_warning");
    abi::emit_symbol_address(emitter, "x9", SOCKET_ERRNO_SYMBOL);
    emitter.instruction("ldr x0, [x9]");                                        // the error number the failing syscall published
    emitter.instruction("bl __rt_socket_strerror");                             // x0 = message, x1 = length
    emitter.instruction("mov x2, x1");                                          // the diagnostic helper takes the length in x2
    emitter.instruction("mov x1, x0");                                          // and the pointer in x1
    emitter.instruction("cbnz x2, __rt_scw_reason_ready");                      // a described failure prints its own reason
    abi::emit_symbol_address(emitter, "x1", "_sock_fail_unknown");              // nothing described this one
    emitter.instruction(&format!("mov x2, #{}", SOCKET_FAILED_REASON_UNKNOWN.len()));
    emitter.label("__rt_scw_reason_ready");
    emitter.instruction("bl __rt_diag_warning");
    abi::emit_symbol_address(emitter, "x1", "_sock_fail_close");
    emitter.instruction(&format!("mov x2, #{}", SOCKET_FAILED_REASON_CLOSE.len()));
    emitter.instruction("bl __rt_diag_warning");

    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the helper frame
    emitter.instruction("ret");
}

/// Emits the x86_64 socket-failure warning.
fn emit_socket_connect_warning_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: socket open failure warning ---");
    emitter.label_global("__rt_socket_connect_warning");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame
    emitter.instruction("sub rsp, 64");                                         // reserve the address, length, port and head slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // preserve the address pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // preserve the address byte length
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // preserve the port, if any

    emitter.instruction(&format!("cmp rdi, {SOCKET_WARNING_SERVER}"));          // which builtin is reporting?
    emitter.instruction("je __rt_scw_server_x86");
    emitter.instruction(&format!("cmp rdi, {SOCKET_WARNING_FSOCKOPEN}"));
    emitter.instruction("je __rt_scw_fsockopen_x86");
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // this wording names no port
    abi::emit_symbol_address(emitter, "rdi", "_sock_fail_client");
    emitter.instruction(&format!("mov rsi, {}", SOCKET_FAILED_CLIENT_PREFIX.len()));
    emitter.instruction("jmp __rt_scw_prefix_ready_x86");
    emitter.label("__rt_scw_server_x86");
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // nor does this one
    abi::emit_symbol_address(emitter, "rdi", "_sock_fail_server");
    emitter.instruction(&format!("mov rsi, {}", SOCKET_FAILED_SERVER_PREFIX.len()));
    emitter.instruction("jmp __rt_scw_prefix_ready_x86");
    emitter.label("__rt_scw_fsockopen_x86");
    emitter.instruction("mov QWORD PTR [rbp - 48], 1");                         // php's fsockopen template is `%s:%d`
    abi::emit_symbol_address(emitter, "rdi", "_sock_fail_fsockopen");
    emitter.instruction(&format!("mov rsi, {}", SOCKET_FAILED_FSOCKOPEN_PREFIX.len()));
    emitter.label("__rt_scw_prefix_ready_x86");
    emitter.instruction("mov QWORD PTR [rbp - 32], rdi");                       // remember the head, both lines carry it
    emitter.instruction("mov QWORD PTR [rbp - 40], rsi");                       // and its length

    // See the AArch64 counterpart: a resolution failure is two warnings in PHP.
    abi::emit_symbol_address(emitter, "r10", "_socket_gai_msg_len");
    emitter.instruction("mov r10, QWORD PTR [r10]");                            // did a resolution fail?
    emitter.instruction("test r10, r10");
    emitter.instruction("jz __rt_scw_no_resolver_line_x86");
    emitter.instruction("call __rt_diag_warning");                              // warnings honour the @ suppression depth
    abi::emit_symbol_address(emitter, "rdi", "_socket_gai_msg");
    abi::emit_symbol_address(emitter, "rsi", "_socket_gai_msg_len");
    emitter.instruction("mov rsi, QWORD PTR [rsi]");                            // the composed resolver message
    emitter.instruction("call __rt_diag_warning");
    abi::emit_symbol_address(emitter, "rdi", "_sock_fail_newline");
    emitter.instruction("mov rsi, 1");                                          // close the resolver's own line
    emitter.instruction("call __rt_diag_warning");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the head for the connect line
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");
    emitter.label("__rt_scw_no_resolver_line_x86");
    emitter.instruction("call __rt_diag_warning");                              // warnings honour the @ suppression depth
    abi::emit_symbol_address(emitter, "rdi", "_sock_fail_unable");
    emitter.instruction(&format!("mov rsi, {}", SOCKET_FAILED_UNABLE.len()));
    emitter.instruction("call __rt_diag_warning");

    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the address the caller wrote
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // and its byte length
    emitter.instruction("call __rt_diag_warning");

    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // does this wording name a port?
    emitter.instruction("test r10, r10");
    emitter.instruction("jz __rt_scw_no_port_x86");                             // the other two put it in the address instead
    abi::emit_symbol_address(emitter, "rdi", "_sock_fail_colon");
    emitter.instruction("mov rsi, 1");
    emitter.instruction("call __rt_diag_warning");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // convert the port to decimal
    emitter.instruction("call __rt_itoa");                                      // rax = digits, rdx = length
    emitter.instruction("mov rdi, rax");                                        // the diagnostic helper takes the pointer in rdi
    emitter.instruction("mov rsi, rdx");                                        // and the length in rsi
    emitter.instruction("call __rt_diag_warning");
    emitter.label("__rt_scw_no_port_x86");

    abi::emit_symbol_address(emitter, "rdi", "_sock_fail_open");
    emitter.instruction(&format!("mov rsi, {}", SOCKET_FAILED_REASON_OPEN.len()));
    emitter.instruction("call __rt_diag_warning");
    abi::emit_symbol_address(emitter, "r10", SOCKET_ERRNO_SYMBOL);
    emitter.instruction("mov rdi, QWORD PTR [r10]");                            // the error number the failing syscall published
    emitter.instruction("call __rt_socket_strerror");                           // rax = message, rdx = length
    emitter.instruction("mov rdi, rax");                                        // the diagnostic helper takes the pointer in rdi
    emitter.instruction("mov rsi, rdx");                                        // and the length in rsi
    emitter.instruction("test rsi, rsi");
    emitter.instruction("jnz __rt_scw_reason_ready_x86");                       // a described failure prints its own reason
    abi::emit_symbol_address(emitter, "rdi", "_sock_fail_unknown");             // nothing described this one
    emitter.instruction(&format!("mov rsi, {}", SOCKET_FAILED_REASON_UNKNOWN.len()));
    emitter.label("__rt_scw_reason_ready_x86");
    emitter.instruction("call __rt_diag_warning");
    abi::emit_symbol_address(emitter, "rdi", "_sock_fail_close");
    emitter.instruction(&format!("mov rsi, {}", SOCKET_FAILED_REASON_CLOSE.len()));
    emitter.instruction("call __rt_diag_warning");

    emitter.instruction("add rsp, 64");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");
}
