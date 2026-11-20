//! Purpose:
//! Authenticates native cells whose physical payload is a persistent boxed PHP reference.
//!
//! Called from:
//! - Managed reference runtime emission and native reference ABI adaptation.
//!
//! Key details:
//! - Raw frames and interior addresses are rejected before any header read.
//! - Descriptor seven alone is insufficient because raw Iterable cells share that tag.
//! - The result is borrowed and must be recomputed after PHP re-entry.

use super::*;

pub(super) use elephc_memoir::reference_cells::canonical::emit;

/// Resolves an authenticated scalar-property reference with the compiler's row flag.
pub(crate) fn resolve_scalar_property(
    emitter: &mut Emitter, address: &str, tag: &str, marker: &str, prefix: &str,
) {
    elephc_memoir::reference_cells::canonical::resolve_scalar_property(
        emitter, address, tag, marker, prefix,
        crate::codegen_support::sentinels::CANONICAL_SCALAR_PROPERTY_FLAG,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every target authenticates the cell and wrapper before inspecting their physical layouts.
    #[test]
    fn canonical_reference_shape_authentication_on_every_target() {
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(crate::codegen_support::platform::Target::parse(target).unwrap());
            emit(&mut emitter);
            let assembly = emitter.output();
            let owner = assembly.find("__rt_reference_cell_owner").unwrap();
            let payload = assembly.find("__rt_gc_candidate_live").unwrap();
            assert!(owner < payload, "{target}: raw cell addresses must be rejected first");
            assert_eq!(assembly.matches("__rt_reference_cell_owner").count(), 1,
                "{target}: shape lookup must use the allocator identity index once");
            assert!(!assembly.contains("__rt_incref"), "{target}: the probe returns a borrowed wrapper");
        }
    }

    /// Metadata walks stay leaf helpers and authenticate ownership before inspecting any header.
    #[test]
    fn canonical_scalar_property_resolution_preserves_leaf_abi_on_every_target() {
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = crate::codegen_support::platform::Target::parse(target).unwrap();
            let mut emitter = Emitter::new(target);
            let (address, tag, marker) = if target.arch == Arch::AArch64 { ("x14", "x17", "x15") }
                else { ("r10", "r9", "r11") };
            resolve_scalar_property(&mut emitter, address, tag, marker, "scalar_row_probe");
            let assembly = emitter.output();
            let metadata_guard = assembly.find("1024").unwrap();
            let header = assembly.find(if target.arch == Arch::AArch64 { "#-8]" } else { " - 8]" }).unwrap();
            assert!(metadata_guard < header, "{target:?}: borrowed rows must skip header reads");
            assert!(!assembly.lines().any(|line| matches!(line.split_whitespace().next(), Some("bl" | "call"))),
                "{target:?}: leaf renderers must not gain hidden calls or clobbers");
            assert!(assembly.contains("scalar_row_probe_scalar_raw"),
                "{target:?}: raw and uninitialized cells retain their original address and tag");
        }
    }
}
