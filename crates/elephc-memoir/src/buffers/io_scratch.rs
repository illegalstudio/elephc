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
pub const OPENED_PATH: StaticStorage = StaticStorage { symbol: "_stream_open_opened_path_scratch", bytes: 16 };
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
