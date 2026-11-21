//! Purpose:
//! Preserves compiler integration tests and compatibility imports for Memoir emitters.
//!
//! Called from:
//! - The elephc compiler memory subsystem.
//!
//! Key details:
//! - This code runs in the compiler and emits assembly; it is never a target-program library.

pub use elephc_memoir::gc_frame_slots::*;

#[cfg(test)]
use crate::codegen_support::{emit::Emitter, platform::Arch};

#[cfg(test)]
mod native_frame_tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Large descriptor signatures can place an owner beyond AArch64's add-immediate range.
    #[test]
    fn native_frame_owners_address_large_offsets_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_native_frame_owners(&mut emitter, "large_native_owners", &[8, 5008]);
            let assembly = emitter.output();
            assert_eq!(assembly.matches("__rt_gc_frame_owner_slot").count(), 2, "{name}: {assembly}");
            if target.arch == Arch::AArch64 {
                assert!(!assembly.contains("add x0, x0, #5008"), "{name}: invalid immediate");
                assert!(assembly.contains("add x0, x0, x9"), "{name}: {assembly}");
            } else {
                assert!(assembly.contains("add rdi, 5008"), "{name}: {assembly}");
            }
        }
    }
}
