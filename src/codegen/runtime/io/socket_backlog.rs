//! Purpose:
//! Emits the `__rt_socket_backlog` runtime helper, which resolves the
//! `listen()` backlog for `stream_socket_server` from the
//! `_stream_context_options['socket']['backlog']` stream-context option.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` — the runtime surface
//!   is shared across AArch64 and Linux x86_64.
//! - The TCP / IPv6 / Unix-domain `stream_socket_server` emitters call it right
//!   before the `listen()` syscall, replacing the previously hardcoded 128.
//!
//! Key details:
//! - The 4-argument `stream_context_set_option` stores option values with the
//!   STRING tag, so the backlog is read as a string via
//!   `__rt_get_string_context_option` and parsed with `__rt_atoi` (the same
//!   pattern as `ftp.resume_pos`). A miss, an empty value, or a parsed value
//!   below 1 falls back to the default backlog of 128.
//! - Output: backlog in `x0` (AArch64) / `rax` (x86_64). The helper preserves
//!   no caller state beyond the ABI return register; callers must reload the
//!   socket fd into the syscall's first-argument register AFTER calling this
//!   (the call clobbers x0/rax).
//! - Touches no global state and references only the pure runtime helpers
//!   `__rt_get_string_context_option` / `__rt_atoi`, so it needs no `-l` linkage
//!   and no indirect-fn-pointer slots.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Default `listen()` backlog when no `socket.backlog` option is set (PHP/libc
/// convention).
const DEFAULT_BACKLOG: i64 = 128;

/// Emits the `__rt_socket_backlog` runtime helper. Output: x0 / rax = backlog.
pub fn emit_socket_backlog(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_socket_backlog_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: socket_backlog ---");
    emitter.label_global("__rt_socket_backlog");
    emitter.instruction("sub sp, sp, #32");                                     // frame: [0]=value ptr [8]=value len [16]=x29 [24]=x30
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish the helper frame pointer

    // -- look up _stream_context_options['socket']['backlog'] as a string --
    emitter.instruction("str xzr, [sp, #0]");                                   // value pointer default = null
    emitter.instruction("str xzr, [sp, #8]");                                   // value length default = 0
    crate::codegen::abi::emit_symbol_address(emitter, "x0", "_socket_key_str");
    emitter.instruction("mov x1, #6");                                          // strlen("socket")
    crate::codegen::abi::emit_symbol_address(emitter, "x2", "_socket_backlog_key_str");
    emitter.instruction("mov x3, #7");                                          // strlen("backlog")
    emitter.instruction("add x4, sp, #0");                                      // out_ptr_addr
    emitter.instruction("add x5, sp, #8");                                      // out_len_addr
    emitter.instruction("bl __rt_get_string_context_option");                   // x0 = 1 on hit, 0 on miss
    emitter.instruction("cbz x0, __rt_socket_backlog_default");                 // option absent → default backlog
    emitter.instruction("ldr x1, [sp, #0]");                                    // value pointer for atoi
    emitter.instruction("ldr x2, [sp, #8]");                                    // value length for atoi
    emitter.instruction("cbz x2, __rt_socket_backlog_default");                 // empty value → default backlog
    emitter.instruction("bl __rt_atoi");                                        // x0 = parsed backlog integer
    emitter.instruction("cmp x0, #1");                                          // is the parsed backlog at least 1?
    emitter.instruction("b.lt __rt_socket_backlog_default");                    // zero/negative → default backlog
    emitter.instruction("b __rt_socket_backlog_done");                          // use the parsed backlog
    emitter.label("__rt_socket_backlog_default");
    emitter.instruction(&format!("mov x0, #{}", DEFAULT_BACKLOG));              // default listen() backlog
    emitter.label("__rt_socket_backlog_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the backlog in x0
}

/// x86_64 Linux implementation of `__rt_socket_backlog`. Output: rax = backlog.
fn emit_socket_backlog_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: socket_backlog ---");
    emitter.label_global("__rt_socket_backlog");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 16");                                         // frame: [-8]=value ptr [-16]=value len

    // -- look up _stream_context_options['socket']['backlog'] as a string --
    emitter.instruction("mov QWORD PTR [rbp - 8], 0");                          // value pointer default = null
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // value length default = 0
    emitter.instruction("lea rdi, [rip + _socket_key_str]");                    // wrapper key = "socket"
    emitter.instruction("mov rsi, 6");                                          // strlen("socket")
    emitter.instruction("lea rdx, [rip + _socket_backlog_key_str]");            // option key = "backlog"
    emitter.instruction("mov rcx, 7");                                          // strlen("backlog")
    emitter.instruction("lea r8, [rbp - 8]");                                   // out_ptr_addr
    emitter.instruction("lea r9, [rbp - 16]");                                  // out_len_addr
    emitter.instruction("call __rt_get_string_context_option");                 // rax = 1 on hit, 0 on miss
    emitter.instruction("test rax, rax");                                       // option present?
    emitter.instruction("jz __rt_socket_backlog_default_x");                    // option absent → default backlog
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // value pointer (__rt_atoi reads the string ptr from rax)
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // value length (__rt_atoi reads the length from rdx)
    emitter.instruction("test rdx, rdx");                                       // empty value?
    emitter.instruction("jz __rt_socket_backlog_default_x");                    // empty value → default backlog
    emitter.instruction("call __rt_atoi");                                      // rax = parsed backlog integer
    emitter.instruction("cmp rax, 1");                                          // is the parsed backlog at least 1?
    emitter.instruction("jl __rt_socket_backlog_default_x");                    // zero/negative → default backlog
    emitter.instruction("jmp __rt_socket_backlog_done_x");                      // use the parsed backlog
    emitter.label("__rt_socket_backlog_default_x");
    emitter.instruction(&format!("mov eax, {}", DEFAULT_BACKLOG));              // default listen() backlog
    emitter.label("__rt_socket_backlog_done_x");
    emitter.instruction("add rsp, 16");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the backlog in rax
}
