//! Purpose:
//! Adapts the shared PCNTL integer-constant catalog to compiler target selection.
//!
//! Called from:
//! - `crate::types::checker::driver::init` and `crate::codegen_support::prescan`.
//!
//! Key details:
//! - The bridge crate is the single source for target-aware libc values used by AOT and eval.

use crate::codegen_support::platform::Platform;

/// Returns the exact PCNTL integer constants exposed for `platform`.
pub(crate) fn pcntl_int_constants(platform: Platform) -> &'static [(&'static str, i64)] {
    match platform {
        Platform::MacOS => elephc_pcntl::MACOS_PCNTL_INT_CONSTANTS,
        Platform::Linux => elephc_pcntl::LINUX_PCNTL_INT_CONSTANTS,
        Platform::Windows => panic!("Windows target is not yet supported (see issue #379)"),
    }
}

/// Reports whether `name` is a PCNTL constant on any supported target.
pub(crate) fn is_pcntl_int_constant(name: &str) -> bool {
    elephc_pcntl::is_pcntl_int_constant(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Looks up one named constant in a target-specific table.
    fn value(platform: Platform, name: &str) -> Option<i64> {
        pcntl_int_constants(platform)
            .iter()
            .find_map(|(candidate, value)| (*candidate == name).then_some(*value))
    }

    /// Verifies the compiler selects the shared target table without changing values.
    #[test]
    fn compiler_platform_selection_uses_shared_catalog() {
        assert_eq!(value(Platform::MacOS, "SIGCHLD"), Some(20));
        assert_eq!(value(Platform::Linux, "SIGCHLD"), Some(17));
        assert_eq!(value(Platform::MacOS, "PCNTL_EAGAIN"), Some(35));
        assert_eq!(value(Platform::Linux, "PCNTL_EAGAIN"), Some(11));
        assert_eq!(value(Platform::Linux, "CLONE_NEWNS"), Some(131_072));
        assert_eq!(value(Platform::MacOS, "CLONE_NEWNS"), None);
        assert!(is_pcntl_int_constant("PRIO_DARWIN_BG"));
        assert!(!is_pcntl_int_constant("NOT_A_PCNTL_CONSTANT"));
    }
}
