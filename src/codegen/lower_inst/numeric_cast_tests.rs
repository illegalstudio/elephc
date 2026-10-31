//! Purpose:
//! Guards the branch range needed by deferred numeric float conversions.
//!
//! Called from:
//! - The library test harness through `lower_inst::numeric_cast`.
//!
//! Key details:
//! - The cold tail can be more than one MiB away, beyond AArch64 conditional branch range.
//! - Small-function peepholes may shorten the wide form after establishing its range.

use super::*;
use crate::codegen::{emit::Emitter, platform::Target};

/// ARM cold branches must use a local conditional skip followed by a wide unconditional jump.
#[test]
fn numeric_integer_cast_far_arm_branch_uses_wide_form() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64"] {
        let mut emitter = Emitter::new(Target::parse(name).unwrap());
        emit_float_branch(&mut emitter, "x9", "numeric_float_cold_tail");
        let assembly = emitter.output();
        assert!(assembly.contains("cbz x9, 1f"), "{name}: {assembly}");
        assert!(assembly.contains("b numeric_float_cold_tail"), "{name}: {assembly}");
        assert!(assembly.contains("1:"), "{name}: {assembly}");
        assert!(!assembly.contains("cbnz x9, numeric_float_cold_tail"), "{name}: {assembly}");
    }
}
