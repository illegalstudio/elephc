//! Purpose:
//! Defines the instruction-set identity used by memory emitters.
//!
//! Called from:
//! - The elephc compiler memory subsystem.
//!
//! Key details:
//! - This code runs in the compiler and emits assembly; it is never a target-program library.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Target architecture.
pub enum Arch {
    AArch64,
    X86_64,
}


impl Arch {
    /// Detects the host architecture from the Rust compile-time target architecture.
    ///
    /// Returns `Arch::AArch64` on ARM64 hosts and `Arch::X86_64` on x86_64 hosts.
    /// Panics if running on an unsupported architecture.
    pub fn detect_host() -> Self {
        if cfg!(target_arch = "aarch64") {
            Arch::AArch64
        } else if cfg!(target_arch = "x86_64") {
            Arch::X86_64
        } else {
            panic!("unsupported host architecture for elephc")
        }
    }
}
