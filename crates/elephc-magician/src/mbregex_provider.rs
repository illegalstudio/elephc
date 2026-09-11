//! Purpose:
//! Queries the shared mbstring bridge for managed Oniguruma availability.
//!
//! Called from:
//! - Eval builtin lookup before exposing a matching operation.
//!
//! Key details:
//! - Magician owns no second regex provider or request state.
//! - Generated setup installs the provider before creating an enabled eval context.

unsafe extern "C" {
    /// Reports successful installation of the complete process-wide mbregex provider.
    fn elephc_mbstring_regex_available_v1() -> i32;
}

/// Exposes matching operations only after the shared bridge has initialized Oniguruma.
pub(crate) fn available() -> bool { unsafe { elephc_mbstring_regex_available_v1() == 1 } }
