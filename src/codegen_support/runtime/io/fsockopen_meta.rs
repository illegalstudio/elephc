//! Purpose:
//! Emits `__rt_stream_record_fsockopen_meta`, which gives an `fsockopen()` descriptor the same
//! transport and `uri` metadata the other socket openers record.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`, after the descriptor has been boxed.
//!
//! Key details:
//! - The address is not an operand here. `fsockopen()` composes it from a hostname and a port, and
//!   php composes the SAME string (`host` plus `:port` when the port is positive) before handing it
//!   to `php_stream_xport_create` — so `__rt_fsockopen` publishes that exact slice in
//!   `_fsockopen_uri_ptr`/`_len` and this helper forwards it. Measured on `php -n` 8.5.6: a
//!   `fsockopen("127.0.0.1", 8080)` reports `uri` as `127.0.0.1:8080`, with no `tcp://` even though
//!   that is the transport it opened.
//! - Without this, an `fsockopen()` stream had no transport recorded at all, so it reported
//!   `wrapper_type` (which php omits for every socket), no `uri` (which php provides) and
//!   `stream_type` `tcp_socket` even for a `unix://` connection.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits `__rt_stream_record_fsockopen_meta(handle) -> handle`.
///
/// Delegates to `__rt_stream_record_transport`, which reads the transport out of the address text
/// and publishes the address as the stream's `uri`.
pub fn emit_stream_record_fsockopen_meta(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: record fsockopen transport and uri ---");
    emitter.label_global("__rt_stream_record_fsockopen_meta");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #32");                             // frame for the handle and the saved linkage
            emitter.instruction("stp x29, x30, [sp, #16]");                     // save frame pointer and return address
            emitter.instruction("add x29, sp, #16");                            // establish the helper frame pointer
            emitter.instruction("str x0, [sp, #0]");                            // the handle is also the answer
            abi::emit_symbol_address(emitter, "x9", "_fsockopen_uri_ptr");
            emitter.instruction("ldr x1, [x9]");                                // the address php would have used
            abi::emit_symbol_address(emitter, "x9", "_fsockopen_uri_len");
            emitter.instruction("ldr x2, [x9]");                                // and its byte length
            emitter.instruction("mov x3, #0");                                  // the address itself names the transport
            emitter.instruction("bl __rt_stream_record_transport");
            emitter.instruction("ldr x0, [sp, #0]");                            // hand the handle back unchanged
            emitter.instruction("ldp x29, x30, [sp, #16]");                     // restore frame pointer and return address
            emitter.instruction("add sp, sp, #32");                             // release the helper frame
            emitter.instruction("ret");
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // preserve the caller frame pointer
            emitter.instruction("mov rbp, rsp");                                // establish the helper frame
            emitter.instruction("sub rsp, 16");                                 // reserve the handle slot
            emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                // the handle is also the answer
            abi::emit_symbol_address(emitter, "r10", "_fsockopen_uri_ptr");
            emitter.instruction("mov rsi, QWORD PTR [r10]");                    // the address php would have used
            abi::emit_symbol_address(emitter, "r10", "_fsockopen_uri_len");
            emitter.instruction("mov rdx, QWORD PTR [r10]");                    // and its byte length
            emitter.instruction("xor ecx, ecx");                                // the address itself names the transport
            emitter.instruction("call __rt_stream_record_transport");
            emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                // hand the handle back unchanged
            emitter.instruction("add rsp, 16");                                 // release the helper frame
            emitter.instruction("pop rbp");                                     // restore the caller frame pointer
            emitter.instruction("ret");
        }
    }
}
