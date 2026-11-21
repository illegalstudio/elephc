//! Purpose:
//! Publishes completed decoded arrays for later references in the same wire stream.
//!
//! Called from:
//! - The target-specific recursive unserialize decoders.
//!
//! Key details:
//! - A context-owned retain keeps registry boxes valid even if hydration discards their data.
//! - Only in-bounds slots publish owners; parser completion retires them after restoring context.

use crate::codegen_support::emit::Emitter;

use elephc_memoir::values::reference_registry;

/// Registers a boxed array at the caller-reserved decode ordinal.
pub(super) fn emit_register_array(emitter: &mut Emitter) {
    reference_registry::emit_register_cell(
        emitter,
        "__rt_unserialize_register_array",
        "_unser_values",
        reference_registry::UNSERIALIZE_CAPACITY,
        "__rt_unserialize_defer_data",
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Arch;
    use crate::codegen_support::platform::Target;

    /// Recursive decoders publish completed arrays and use the shared payload-owning clone path.
    #[test]
    fn array_references_publish_and_retain_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            match emitter.target.arch {
                Arch::AArch64 => super::super::decoder_aarch64::emit_parser(&mut emitter),
                Arch::X86_64 => super::super::decoder_x86_64::emit_parser(&mut emitter),
            }
            let asm = emitter.output();
            let array_close = asm.split_once("__rt_unser_at_array_close:\n").unwrap().1
                .split_once("__rt_unser_at_array_fail:\n").unwrap().0;
            assert!(array_close.contains("__rt_unserialize_register_array"), "{name}");
            let reference = asm.split_once("__rt_unser_at_ref:\n").unwrap().1;
            assert!(reference.contains("__rt_mixed_clone"), "{name}");
            assert!(reference.contains("114"), "{name}: lowercase references stay object-only");
            assert!(!reference.contains("__rt_heap_alloc"), "{name}: no unretained shallow payload copies");
        }
    }

    /// Every ABI bounds publication and retains the registry owner before deferring its release.
    #[test]
    fn completed_array_registry_owners_are_bounded_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_register_array(&mut emitter);
            let asm = emitter.output();
            assert!(asm.find("65536").unwrap() < asm.find("_unser_values").unwrap(), "{name}");
            assert!(asm.find("_unser_values").unwrap() < asm.find("__rt_incref").unwrap(), "{name}");
            assert!(asm.find("__rt_incref").unwrap() < asm.find("__rt_unserialize_defer_data").unwrap(), "{name}");
        }
    }
}
