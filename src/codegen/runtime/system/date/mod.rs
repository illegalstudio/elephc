//! Purpose:
//! Dispatches the date() runtime emitter to the active target implementation.
//! The module keeps the public __rt_date emission entry stable while target formatters diverge.
//!
//! Called from:
//! - `crate::codegen::runtime::system::emit_date()`.
//!
//! Key details:
//! - The public label stays __rt_date while architecture-specific emitters own register and libc details.

mod arm64;
mod linux_x86_64;

use crate::codegen::{emit::Emitter, platform::Arch};

/// __rt_date: format a Unix timestamp according to a PHP date format string.
/// Input:  x0=timestamp (-1 = use current time), x1=format_ptr, x2=format_len
/// Output: x1=result ptr (in concat_buf), x2=result len
///
/// `__rt_gmdate` is co-emitted here and shares the whole body; it differs only in
/// using libc `gmtime` (UTC) instead of `localtime` to decompose the timestamp.
///
/// Supports format characters: Y, y, m, n, d, j, D, l, N, w, F, M, H, G, h, g,
/// i, s, A, a, U, S, z, t, L, W, o
///
/// Uses libc _time, _localtime/_gmtime to get struct tm components.
/// struct tm layout: tm_sec(+0), tm_min(+4), tm_hour(+8), tm_mday(+12),
///                   tm_mon(+16), tm_year(+20), tm_wday(+24), tm_yday(+28), tm_isdst(+32)
pub(crate) fn emit_date(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        linux_x86_64::emit_date_linux_x86_64(emitter);
        return;
    }

    arm64::emit_date_arm64(emitter);
}
