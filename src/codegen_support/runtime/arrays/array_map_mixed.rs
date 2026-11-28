//! Purpose:
//! Emits descriptor-backed maps whose callback results are independently owned Mixed cells.
//!
//! Called from:
//! - Shared runtime array helper emission.
//!
//! Key details:
//! - Reuses the guarded indexed-map frame, callback ABI and exceptional cleanup.
//! - Mixed element stamps and incremental lengths make partial-result cleanup exact.

use crate::codegen_support::emit::Emitter;

/// Emits a Mixed-slot map with the shared resumable callback cleanup boundary.
pub fn emit_array_map_mixed(emitter: &mut Emitter) {
    super::array_map::emit_guarded_map(emitter, "__rt_array_map_mixed", true);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Arch, Target};

    /// Both map representations guard callback exceptions and publish the initialized owner prefix.
    #[test]
    fn strict_numeric_aggregates_mixed_map_exception_boundary_covers_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            super::super::array_map::emit_array_map(&mut emitter);
            emit_array_map_mixed(&mut emitter);
            let asm = emitter.output();
            for prefix in ["__rt_array_map", "__rt_array_map_mixed"] {
                assert!(asm.contains(&format!("{prefix}:\n")), "{name}");
                assert!(asm.contains(&format!("{prefix}_throw:\n")), "{name}");
                assert!(asm.contains(&format!("{prefix}_body:\n")), "{name}");
            }
            assert_eq!(asm.matches("__rt_cleanup_invoke").count(), 4, "{name}");
            let publish = if target.arch == Arch::AArch64 { "str x20, [x1]" } else { "mov QWORD PTR [r10], r13" };
            assert_eq!(asm.matches(publish).count(), 2, "{name}");
        }
    }
}
