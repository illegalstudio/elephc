//! Purpose:
//! Emits `__rt_mb_convert_case`, the runtime helper for PHP's `mb_convert_case()`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()`.
//! - `crate::codegen::lower_inst::builtins::strings::lower_mb_convert_case()`.
//!
//! Key details:
//! - Omitted/null/`UTF-8` encodings convert UTF-8; `8bit`/`binary`/`7bit` treat bytes as `U+00xx`.
//! - Other names decode through libc `iconv` into UTF-8, convert, and convert back.
//! - Case tables are generated from `crate::mb_case` and gated by `RuntimeFeatures`.

mod aarch64;
mod tables;
mod x86_64;

use crate::codegen_support::{
    emit::Emitter,
    platform::{Arch, Platform},
};

/// Maximum explicit encoding-name length copied into the runtime's stack buffer.
const MAX_ENCODING_NAME_LEN: usize = 63;

/// Worst-case UTF-8 expansion reserved for one converted result.
const RESERVE_MULTIPLIER: u64 = 4;

/// Emits `__rt_mb_convert_case(str_ptr, str_len, mode, encoding_ptr, encoding_len) -> string`.
pub fn emit_mb_convert_case(emitter: &mut Emitter) {
    tables::emit_case_tables(emitter);
    match emitter.platform {
        Platform::Linux => {}
        Platform::MacOS => emitter.raw(".section __TEXT,__text,regular,pure_instructions"),
        Platform::Windows => panic!("Windows target is not yet supported (see issue #379)"),
    }
    if emitter.target.arch == Arch::X86_64 {
        x86_64::emit_mb_convert_case_x86_64(emitter);
    } else {
        aarch64::emit_mb_convert_case_aarch64(emitter);
    }
}
