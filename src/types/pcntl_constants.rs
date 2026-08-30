//! Purpose:
//! Adapts the shared PCNTL integer-constant catalog to compiler target selection.
//!
//! Called from:
//! - `crate::types::checker::driver::init` and `crate::codegen_support::prescan`.
//!
//! Key details:
//! - The bridge crate is the single source for target-aware libc values used by AOT and eval.

use crate::codegen_support::platform::{Platform, Target};

/// Returns the exact PCNTL integer constants exposed for `target`.
pub(crate) fn pcntl_int_constants(target: Target) -> &'static [(&'static str, i64)] {
    if target.is_ios() {
        return &[];
    }
    match target.platform {
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
    fn value(target: Target, name: &str) -> Option<i64> {
        pcntl_int_constants(target)
            .iter()
            .find_map(|(candidate, value)| (*candidate == name).then_some(*value))
    }

    /// Verifies the compiler selects the shared target table without changing values.
    #[test]
    fn compiler_platform_selection_uses_shared_catalog() {
        use crate::codegen_support::platform::{AppleVariant, Arch};

        let macos = Target::new(Platform::MacOS, Arch::AArch64);
        let linux = Target::new(Platform::Linux, Arch::AArch64);
        let ios = Target::new_apple(Arch::AArch64, AppleVariant::IOS);
        let ios_sim = Target::new_apple(Arch::AArch64, AppleVariant::IOSSimulator);
        assert_eq!(value(macos, "SIGCHLD"), Some(20));
        assert_eq!(value(linux, "SIGCHLD"), Some(17));
        assert_eq!(value(macos, "PCNTL_EAGAIN"), Some(35));
        assert_eq!(value(linux, "PCNTL_EAGAIN"), Some(11));
        assert_eq!(value(linux, "CLONE_NEWNS"), Some(131_072));
        assert_eq!(value(macos, "CLONE_NEWNS"), None);
        assert!(pcntl_int_constants(ios).is_empty());
        assert!(pcntl_int_constants(ios_sim).is_empty());
        assert!(is_pcntl_int_constant("PRIO_DARWIN_BG"));
        assert!(!is_pcntl_int_constant("NOT_A_PCNTL_CONSTANT"));
    }
}
