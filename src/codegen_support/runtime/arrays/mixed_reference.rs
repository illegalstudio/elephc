//! Purpose:
//! Preserves compiler imports and tests for Memoir boxed-value ownership.
//!
//! Called from:
//! - Compiler runtime emission and memory integration tests.
//!
//! Key details:
//! - Assembly implementation lives in the Memoir codegen feature.

pub use elephc_memoir::values::mixed_reference::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::{emit::Emitter, platform::Target};

    /// Reference-aware dereferencing follows both nested wrappers and managed hash cells.
    #[test]
    fn mixed_reference_deref_emits_every_supported_target() {
        for name in [
            "macos-aarch64",
            "ios-arm64",
            "ios-sim-arm64",
            "linux-aarch64",
            "linux-x86_64",
        ] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_mixed_reference(&mut emitter);
            let assembly = emitter.output();
            for symbol in [
                "__rt_mixed_deref_reference:",
                "__rt_mixed_deref_reference_nested",
                "__rt_mixed_deref_reference_done",
            ] {
                assert!(assembly.contains(symbol), "{name}: missing {symbol}");
            }
        }
    }
}
