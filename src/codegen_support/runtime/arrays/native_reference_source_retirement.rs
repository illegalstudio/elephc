//! Purpose:
//! Preserves compiler imports and tests for Memoir value storage.
//!
//! Called from:
//! - Compiler memory emission and its integration tests.
//!
//! Key details:
//! - Runs only in the compiler and preserves the generated target assembly.


#[cfg(test)]
use crate::codegen_support::emit::Emitter;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::{platform::Target, RuntimeFeatures};

    /// Sources remain present through resurrection and retire before their first property release.
    #[test]
    fn native_reference_sources_retire_only_in_the_final_property_walk() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            super::super::object_free_deep::emit_object_free_deep(&mut emitter,
                RuntimeFeatures { native_reference_validation: true, ..RuntimeFeatures::none() });
            let assembly = emitter.output();
            let loop_at = assembly.find("__rt_object_free_deep_loop:").unwrap();
            let unbind = assembly.find("__elephc_reference_sources_unbind_v1").unwrap();
            let release = assembly.find("__rt_object_free_deep_release_runtime:").unwrap();
            assert!(loop_at < unbind && unbind < release, "{name}: retire one source before its owner");
            assert!(assembly[..loop_at].contains("resurrected"), "{name}: resurrection precedes the final walk");
            assert_eq!(assembly.matches("__elephc_reference_sources_unbind_v1").count(), 1, "{name}");
        }
    }
}
