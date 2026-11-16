//! Purpose:
//! Retires one short-lived call operand while exception cleanup walks its scope record.
//!
//! Called from:
//! - The activation-chain walker through the record's unary C callback.
//!
//! Key details:
//! - The argument points to an owning slot, not directly to the payload.
//! - Clearing the slot prevents later PHP-frame cleanup from releasing the owner twice.
//! - Destructor exceptions are chained without escaping the cleanup walker.

pub use elephc_memoir::temporary_owners::emit_cleanup_call_operand_owner;

#[cfg(test)]
use crate::codegen_support::emit::Emitter;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Arch, Target};

    /// Slot retirement precedes bounded release, including when the payload's destructor throws.
    #[test]
    fn call_operand_cleanup_clears_before_release_on_all_targets() {
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(target).unwrap());
            emit_cleanup_call_operand_owner(&mut emitter);
            let clear = if emitter.target.arch == Arch::AArch64 {
                "str xzr, [x10]"
            } else {
                "mov QWORD PTR [r10], 0"
            };
            let asm = emitter.output();
            assert!(asm.find(clear).unwrap() < asm.find("__rt_cleanup_preserve_exception").unwrap(), "{target}");
            assert!(!asm.contains("__rt_throw_current"), "{target}");
        }
    }
}
