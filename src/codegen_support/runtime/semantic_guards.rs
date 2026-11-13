//! Purpose:
//! Emits shared native strict guards for boxed runtime consumers.
//!
//! Called from:
//! - Runtime emission and callback predicate consumers.
//!
//! Key details:
//! - Borrowed and consumed cells have distinct entries; failures use the ordinary unwinder.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use elephc_semantics::{SemanticProfile, StrictFeature};

const OWNED_FRAME: usize = 96;
const OWNED_RECORD: usize = 80;

const BOOLEAN_MESSAGE: &str = "--strict-bools requires a boolean callback result";

/// Emits data only for enabled guards, preserving the default runtime data image.
pub(crate) fn data(profile: SemanticProfile) -> String {
    if !profile.contains(StrictFeature::Bools) { return String::new(); }
    format!("\n.data\n_strict_boolean_message:\n.ascii \"{BOOLEAN_MESSAGE}\"\n")
}

/// Emits bool guards and a consumed-result adapter on every supported architecture.
pub(in crate::codegen_support::runtime) fn emit(emitter: &mut Emitter) {
    if !emitter.semantic_profile.contains(StrictFeature::Bools) { return; }
    let result = abi::int_result_reg(emitter);
    let tag = abi::secondary_scratch_reg(emitter);
    emitter.label_global("__rt_strict_boolean");
    abi::emit_branch_if_int_result_zero(emitter, "__rt_strict_boolean_fail");
    abi::emit_load_from_address(emitter, tag, result, 0);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("cmp {tag}, #3")),        // require the PHP boolean runtime tag
        Arch::X86_64 => emitter.instruction(&format!("cmp {tag}, 3")),          // require the PHP boolean runtime tag
    }
    abi::emit_branch_if_not_equal_wide(emitter, "__rt_strict_boolean_fail");
    abi::emit_jump(emitter, "__rt_mixed_cast_bool");

    emitter.label_global("__rt_strict_boolean_owned");
    abi::emit_frame_prologue(emitter, OWNED_FRAME);
    abi::store_at_offset(emitter, result, 8);
    let owner = abi::tertiary_scratch_reg(emitter);
    abi::emit_frame_slot_address(emitter, owner, 8);
    abi::emit_link_call_operand_owner_in_frame(emitter, owner, OWNED_RECORD);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_strict_boolean_owned_fail");
    abi::emit_load_from_address(emitter, tag, result, 0);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("cmp {tag}, #3")),        // inspect the consumed callback result kind
        Arch::X86_64 => emitter.instruction(&format!("cmp {tag}, 3")),          // inspect the consumed callback result kind
    }
    abi::emit_branch_if_not_equal_wide(emitter, "__rt_strict_boolean_owned_fail");
    abi::emit_call_label(emitter, "__rt_mixed_cast_bool");
    abi::store_at_offset(emitter, result, 16);
    abi::emit_unlink_call_operand_owner_in_frame(emitter, OWNED_RECORD);
    abi::load_at_offset(emitter, result, 8);
    abi::emit_call_label(emitter, "__rt_decref_mixed");
    abi::load_at_offset(emitter, result, 16);
    abi::emit_frame_restore(emitter, OWNED_FRAME);
    abi::emit_return(emitter);
    emitter.label("__rt_strict_boolean_owned_fail");
    // Leave the input published so cleanup can chain a throwing destructor to the TypeError.
    // A separate global entry is a separate dead-stripping section, so fallthrough cannot retain it.
    abi::emit_jump(emitter, "__rt_strict_boolean_fail");
    emitter.label_global("__rt_strict_boolean_fail");
    match emitter.target.arch {
        Arch::AArch64 => super::arrays::value_error::emit_throw_static_message_aarch64(
            emitter, "_spl_type_error_class_id", "_strict_boolean_message", BOOLEAN_MESSAGE.len()),
        Arch::X86_64 => super::arrays::value_error::emit_throw_static_message_x86_64(
            emitter, "_spl_type_error_class_id", "_strict_boolean_message", BOOLEAN_MESSAGE.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Both owned and borrowed callback guards exist for every supported target and stay opt-in.
    #[test]
    fn strict_profile_bools_runtime_helpers_cover_all_supported_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit(&mut emitter);
            assert!(emitter.output().is_empty(), "{name}");
            let mut emitter = Emitter::new(target);
            emitter.semantic_profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Bools, true);
            emit(&mut emitter);
            let asm = emitter.output();
            for symbol in ["__rt_strict_boolean", "__rt_strict_boolean_owned", "_spl_type_error_class_id", "__rt_throw_current"] {
                assert!(asm.contains(symbol), "{name}: {symbol}");
            }
            assert!(asm.contains(".globl __rt_strict_boolean_fail"), "{name}");
            let branch = if target.arch == Arch::AArch64 { "b __rt_strict_boolean_fail" } else { "jmp __rt_strict_boolean_fail" };
            assert!(asm.contains(branch), "{name}: global failure entries require explicit section edges");
        }
    }
}
