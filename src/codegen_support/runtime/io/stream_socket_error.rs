//! Purpose:
//! Maps the normalized socket errno to a stable PHP-visible error message.
//!
//! Called from:
//! - `__rt_stream_socket_server` failure paths after native errno capture.
//!
//! Key details:
//! - Windows callers arrive through the Winsock-to-POSIX translator first.
//! - The pointer/length pair is always valid, including the generic fallback.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits the socket-server errno-to-message helper.
pub fn emit_stream_socket_error_message(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86(emitter);
    } else {
        emit_aarch64(emitter);
    }
}

fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream socket error message ---");
    emitter.label_global("__rt_stream_socket_error_message");
    abi::emit_load_symbol_to_reg(emitter, "x0", "_stream_socket_errno", 0);       // load normalized errno
    emitter.instruction("cmp x0, #98");                                         // EADDRINUSE
    emitter.instruction("b.eq __rt_ssem_inuse");                                // select address-in-use text
    emitter.instruction("cmp x0, #13");                                         // EACCES
    emitter.instruction("b.eq __rt_ssem_access");                               // select permission text
    emitter.instruction("cmp x0, #111");                                        // ECONNREFUSED
    emitter.instruction("b.eq __rt_ssem_refused");                              // select refusal text
    abi::emit_symbol_address(emitter, "x9", "_stream_error_generic");            // generic diagnostic pointer
    emitter.instruction("mov x10, #23");                                        // generic diagnostic length
    emitter.instruction("b __rt_ssem_store");                                   // store fallback
    emitter.label("__rt_ssem_inuse");
    abi::emit_symbol_address(emitter, "x9", "_stream_error_inuse");              // address-in-use pointer
    emitter.instruction("mov x10, #22");                                        // address-in-use length
    emitter.instruction("b __rt_ssem_store");                                   // store selected text
    emitter.label("__rt_ssem_access");
    abi::emit_symbol_address(emitter, "x9", "_stream_error_access");             // permission pointer
    emitter.instruction("mov x10, #17");                                        // permission length
    emitter.instruction("b __rt_ssem_store");                                   // store selected text
    emitter.label("__rt_ssem_refused");
    abi::emit_symbol_address(emitter, "x9", "_stream_error_refused");            // refusal pointer
    emitter.instruction("mov x10, #18");                                        // refusal length
    emitter.label("__rt_ssem_store");
    abi::emit_store_reg_to_symbol(emitter, "x9", "_stream_socket_error_ptr", 0); // publish message pointer
    abi::emit_store_reg_to_symbol(emitter, "x10", "_stream_socket_error_len", 0); // publish message length
    emitter.instruction("ret");                                                 // return to failure caller
}

fn emit_x86(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream socket error message ---");
    emitter.label_global("__rt_stream_socket_error_message");
    abi::emit_load_symbol_to_reg(emitter, "rax", "_stream_socket_errno", 0);    // load normalized errno
    emitter.instruction("cmp rax, 98");                                         // EADDRINUSE
    emitter.instruction("je __rt_ssem_inuse_x86");                              // select address-in-use text
    emitter.instruction("cmp rax, 13");                                         // EACCES
    emitter.instruction("je __rt_ssem_access_x86");                             // select permission text
    emitter.instruction("cmp rax, 111");                                        // ECONNREFUSED
    emitter.instruction("je __rt_ssem_refused_x86");                            // select refusal text
    abi::emit_symbol_address(emitter, "r10", "_stream_error_generic");          // generic diagnostic pointer
    emitter.instruction("mov r11, 23");                                         // generic diagnostic length
    emitter.instruction("jmp __rt_ssem_store_x86");                             // store fallback
    emitter.label("__rt_ssem_inuse_x86");
    abi::emit_symbol_address(emitter, "r10", "_stream_error_inuse");            // address-in-use pointer
    emitter.instruction("mov r11, 22");                                         // address-in-use length
    emitter.instruction("jmp __rt_ssem_store_x86");                             // store selected text
    emitter.label("__rt_ssem_access_x86");
    abi::emit_symbol_address(emitter, "r10", "_stream_error_access");           // permission pointer
    emitter.instruction("mov r11, 17");                                         // permission length
    emitter.instruction("jmp __rt_ssem_store_x86");                             // store selected text
    emitter.label("__rt_ssem_refused_x86");
    abi::emit_symbol_address(emitter, "r10", "_stream_error_refused");          // refusal pointer
    emitter.instruction("mov r11, 18");                                         // refusal length
    emitter.label("__rt_ssem_store_x86");
    abi::emit_store_reg_to_symbol(emitter, "r10", "_stream_socket_error_ptr", 0); // publish message pointer
    abi::emit_store_reg_to_symbol(emitter, "r11", "_stream_socket_error_len", 0); // publish message length
    emitter.instruction("ret");                                                 // return to failure caller
}
