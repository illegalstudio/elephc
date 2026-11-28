//! Purpose:
//! Preserves the runtime emitter path for pointer-array storage conversion.
//!
//! Called from:
//! - Compiler runtime orchestration.
//!
//! Key details:
//! - Memoir emits the conversion for both native architectures.

pub use elephc_memoir::values::array_to_pointer_words::emit_array_to_pointer_words;

#[cfg(test)]
mod tests {
    use super::emit_array_to_pointer_words;
    use crate::codegen_support::{emit::Emitter, platform::Target};

    /// Preserves the ownership transfer order of an indexed pointer-array conversion.
    #[test]
    fn pointer_array_conversion_releases_cells_before_publishing_pointer_words() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_array_to_pointer_words(&mut emitter);
            let assembly = emitter.output();
            let position = |needle| {
                assembly.find(needle).unwrap_or_else(|| panic!("{name}: missing {needle}"))
            };
            let separate = position("__rt_array_ensure_unique");
            let unbox = position("__rt_mixed_unbox");
            let release = position("__rt_decref_mixed");
            let publish = position("__rt_array_to_pointer_words_store:");
            let stamp = position("__rt_array_to_pointer_words_stamp:");
            assert!(
                separate < unbox && unbox < release && release < publish && publish < stamp,
                "{name}: {assembly}",
            );
        }
    }
}
