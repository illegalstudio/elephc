//! Purpose:
//! Dispatches the date() runtime emitter to the active target implementation.
//! The module keeps the public `__rt_date` emission entry stable across target ABIs.
//!
//! Called from:
//! - `crate::codegen_support::runtime::system::emit_date()`.
//!
//! Key details:
//! - Architecture wrappers delegate every timestamp to the same php-src timelib bridge.

mod arm64;
mod linux_x86_64;

use crate::codegen_support::{
    emit::Emitter,
    platform::{Arch, Platform},
};

/// Emits `__rt_date` and `__rt_gmdate` for the active target ABI.
///
/// Both entry points format through the vendored timelib bridge. `date()` selects the
/// active PHP timezone and `gmdate()` selects UTC; omitted timestamps still query libc
/// `time()` before entering the common bridge.
pub(crate) fn emit_date(emitter: &mut Emitter) {
    match emitter.platform {
        Platform::MacOS => emitter.raw(".weak_reference _elephc_tz_format"),
        Platform::Linux => emitter.raw(".weak elephc_tz_format"),
        Platform::Windows => {}
    }

    if emitter.target.arch == Arch::X86_64 {
        linux_x86_64::emit_date_linux_x86_64(emitter);
        return;
    }

    arm64::emit_date_arm64(emitter);
}
