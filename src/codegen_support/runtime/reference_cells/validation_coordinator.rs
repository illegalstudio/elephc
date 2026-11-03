//! Purpose:
//! Calls the native Rust validation coordinator with a complete protected operation table.
//!
//! Called from:
//! - Native reference publishers when the native_reference_validation capability is enabled.
//!
//! Key details:
//! - Arguments are borrowed reference, original RHS, context and writable outcome.
//! - The publisher owns context, outcome and invisible roots through every callback and commit.
//! - All PHP exits are contained by operation callbacks before crossing Rust stack frames.

use super::*;

const FRAME: usize = 112;
const OPS: usize = 88;

/// Emits a C-ABI bridge preserving the coordinator's success/failure/Throwable/conflict statuses.
pub(in crate::codegen_support::runtime) fn emit(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    emitter.label_global("__rt_reference_validate_native");
    abi::emit_frame_prologue(emitter, FRAME);
    for (index, offset) in [8, 16, 24, 32].into_iter().enumerate() {
        abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    for (offset, value) in [(OPS, 1), (OPS - 8, 48)] {
        abi::emit_load_int_immediate(emitter, result, value);
        abi::store_at_offset(emitter, result, offset);
    }
    for (offset, entry) in [
        (OPS - 16, "__rt_reference_validation_is_current"),
        (OPS - 24, "__rt_reference_validation_coerce"),
        (OPS - 32, "__rt_reference_validation_identical"),
        (OPS - 40, "__rt_reference_validation_release"),
    ] {
        abi::emit_symbol_address(emitter, result, entry);
        abi::store_at_offset(emitter, result, offset);
    }
    for (index, offset) in [8, 16, 24].into_iter().enumerate() {
        abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    abi::emit_frame_slot_address(emitter, abi::int_arg_reg_name(emitter.target, 3), OPS);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 4), 32);
    let entry = emitter.target.extern_symbol("__elephc_reference_validate_v1");
    abi::emit_call_label(emitter, &entry);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every supported ABI supplies the versioned table without selecting PHP call conventions.
    #[test]
    fn native_reference_coordinator_has_all_protected_callbacks_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit(&mut emitter);
            let assembly = emitter.output();
            for entry in ["is_current", "coerce", "identical", "release"] {
                assert!(assembly.contains(&format!("__rt_reference_validation_{entry}")), "{name}: {entry}");
            }
            assert!(assembly.contains(&target.extern_symbol("__elephc_reference_validate_v1")), "{name}: native Rust ABI");
            assert!(!assembly.contains("__rt_throw_current"), "{name}: only contained statuses cross Rust");
        }
    }
}
