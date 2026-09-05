//! Purpose:
//! Emits `__rt_mb_strtoupper`, the runtime helper for PHP's `mb_strtoupper()`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()`.
//!
//! Key details:
//! - Unicode full-case tables are shared across targets; each architecture owns
//!   encoding dispatch, UTF-8 walking, ASCII byte folding, and the iconv round-trip.

use crate::codegen_support::{emit::Emitter, platform::Arch};

mod aarch64;
mod case_map;
mod x86_64;

/// Emits `__rt_mb_strtoupper` and the shared Unicode uppercase helper.
pub fn emit_mb_strtoupper(emitter: &mut Emitter) {
    case_map::emit_case_map_data(emitter);
    emitter.raw(".text");
    case_map::emit_case_upper(emitter);
    if emitter.target.arch == Arch::X86_64 {
        x86_64::emit_mb_strtoupper_x86_64(emitter);
    } else {
        aarch64::emit_mb_strtoupper_aarch64(emitter);
    }
}
