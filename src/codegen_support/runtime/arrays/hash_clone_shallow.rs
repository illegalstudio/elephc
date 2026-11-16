//! Purpose:
//! Preserves compiler imports and tests for Memoir value storage.
//!
//! Called from:
//! - Compiler memory emission and its integration tests.
//!
//! Key details:
//! - Runs only in the compiler and preserves the generated target assembly.

pub use elephc_memoir::values::hash_clone_shallow::*;

#[cfg(test)]
use crate::codegen_support::{emit::Emitter};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Hash COW delegates tag-11 ownership to the helper that separates singleton cells.
    #[test]
    fn hash_clone_uses_reference_aware_cell_cloning_on_every_target() {
        for name in [
            "macos-aarch64",
            "ios-arm64",
            "ios-sim-arm64",
            "linux-aarch64",
            "linux-x86_64",
        ] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_hash_clone_shallow(&mut emitter);
            let assembly = emitter.output();
            let (tag_branch, reference_path) = assembly
                .split_once("__rt_hash_clone_shallow_value_reference_cell:")
                .unwrap_or_else(|| panic!("{name}: missing reference-cell clone path"));
            assert!(
                tag_branch.contains("__rt_hash_clone_shallow_value_reference_cell"),
                "{name}: tag 11 must select the reference-aware path",
            );
            assert!(
                reference_path.contains("__rt_reference_cell_clone"),
                "{name}: reference entries must use singleton-aware cell cloning",
            );
            let reference_path = reference_path
                .split("__rt_hash_clone_shallow_insert:")
                .next()
                .expect("reference path precedes insertion");
            assert!(
                !reference_path.contains("__rt_incref"),
                "{name}: reference entries must not bypass singleton separation",
            );
        }
    }
}
