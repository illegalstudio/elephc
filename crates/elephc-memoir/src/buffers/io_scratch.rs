//! Purpose:
//! Declares the fixed byte buffers used by native protocol and system adapters.
//!
//! Called from:
//! - Fixed runtime data emission and the runtime's I/O adapters.
//!
//! Key details:
//! - Each caller selects the byte budget, copy order and lifetime of its window.

use crate::{Arch, Emitter, data::StaticStorage};

pub const HTTP_REQUEST: StaticStorage = StaticStorage { symbol: "_http_req_scratch", bytes: 8_192 };
pub const HTTP_RESPONSE: StaticStorage = StaticStorage { symbol: "_http_resp_buf", bytes: 1_048_576 };
pub const HTTPS_RESPONSE: StaticStorage = StaticStorage { symbol: "_https_resp_buf", bytes: 1_048_576 };
pub const HTTP_REDIRECT: StaticStorage = StaticStorage { symbol: "_http_redirect_path_buf", bytes: 2_048 };
pub const FTP_RESPONSE: StaticStorage = StaticStorage { symbol: "_ftp_resp_buf", bytes: 4_096 };
pub const FTP_COMMAND: StaticStorage = StaticStorage { symbol: "_ftp_cmd_scratch", bytes: 64 };
pub const PHAR_OUTPUT: StaticStorage = StaticStorage { symbol: "_phar_write_out", bytes: 1_048_576 };
/// Owned runtime-built URL for the legacy single-stream fallback.
pub const PHAR_URL_POINTER: StaticStorage = StaticStorage { symbol: "_phar_write_url_ptr", bytes: 8 };
/// The fallback's URL length doubles as the dynamic-stream marker.
pub const PHAR_URL_LENGTH: StaticStorage = StaticStorage { symbol: "_phar_write_url_len", bytes: 8 };
pub const OPENED_PATH: StaticStorage = StaticStorage { symbol: "_stream_open_opened_path_scratch", bytes: 16 };
pub const RUNTIME_WRAPPER_CLASS: StaticStorage = StaticStorage { symbol: "_runtime_wrapper_class_buf", bytes: 1_024 };
/// Maximum class-name bytes copied before the terminating NUL.
pub const RUNTIME_WRAPPER_CLASS_MAX_BYTES: usize = RUNTIME_WRAPPER_CLASS.bytes - 1;
pub const WRAPPER_DRAIN: StaticStorage = StaticStorage { symbol: "_user_wrapper_drain_buf", bytes: 1_048_576 };
pub const PROTOCOLS: StaticStorage = StaticStorage { symbol: "_protoent_buf", bytes: 32_768 };
pub const SERVICES: StaticStorage = StaticStorage { symbol: "_servent_buf", bytes: 1_048_576 };
pub const PRINCIPAL: StaticStorage = StaticStorage { symbol: "_principal_lookup_buf", bytes: 4_096 };

/// Stores one byte at the caller's cursor in a previously materialized wrapper scratch span.
pub fn emit_store_wrapper_byte(emitter: &mut impl Emitter, base: &str, cursor: &str, byte: &str) {
    match emitter.arch() {
        Arch::AArch64 => emitter.instruction(&format!("strb {byte}, [{base}, {cursor}]")), // append one byte to the wrapper line
        Arch::X86_64 => emitter.instruction(&format!("mov BYTE PTR [{base} + {cursor}], {byte}")), // append one byte to the wrapper line
    }
}

/// Installs the newly persisted fallback URL and releases the independently displaced owner.
pub fn emit_replace_phar_url(emitter: &mut impl crate::Emitter) {
    let (pointer, length, previous) = match emitter.arch() {
        crate::Arch::AArch64 => ("x1", "x2", "x0"),
        crate::Arch::X86_64 => ("rax", "rdx", "r10"),
    };
    emitter.emit_load_symbol_to_reg(previous, PHAR_URL_POINTER.symbol, 0);
    emitter.emit_store_reg_to_symbol(pointer, PHAR_URL_POINTER.symbol, 0);
    emitter.emit_store_reg_to_symbol(length, PHAR_URL_LENGTH.symbol, 0);
    emitter.emit_reg_move(emitter.int_result_reg(), previous);
    emitter.emit_call_label("__rt_heap_free_safe");
}

/// Clears a fallback URL before retiring its owner while preserving the caller's result.
///
/// The leaf x86-64 open path is unaligned on entry. Finalization already has an aligned frame.
pub fn emit_clear_phar_url(emitter: &mut impl crate::Emitter, leaf: bool) {
    match emitter.arch() {
        crate::Arch::AArch64 => emitter.instruction("stp x0, x30, [sp, #-16]!"), // retain the result and return address across retirement
        crate::Arch::X86_64 if leaf => emitter.instruction("push rax"),           // preserve the result and align this leaf call
        crate::Arch::X86_64 => {
            emitter.instruction("sub rsp, 16");                                // reserve a spill while keeping the call aligned
            emitter.instruction("mov QWORD PTR [rsp], rax");                   // preserve the finalize status
        }
    }
    let result = emitter.int_result_reg();
    emitter.emit_load_symbol_to_reg(result, PHAR_URL_POINTER.symbol, 0);
    let scratch = emitter.secondary_scratch_reg();
    for symbol in [PHAR_URL_POINTER.symbol, PHAR_URL_LENGTH.symbol] {
        emitter.emit_symbol_address(scratch, symbol);
        emitter.emit_store_zero_to_address(scratch, 0);
    }
    emitter.emit_call_label("__rt_heap_free_safe");
    match emitter.arch() {
        crate::Arch::AArch64 => emitter.instruction("ldp x0, x30, [sp], #16"),   // restore the caller's result and linkage
        crate::Arch::X86_64 if leaf => emitter.instruction("pop rax"),            // recover the leaf return value
        crate::Arch::X86_64 => {
            emitter.instruction("mov rax, QWORD PTR [rsp]");                   // recover the finalize status
            emitter.instruction("add rsp, 16");                                // release the aligned spill
        }
    }
}
