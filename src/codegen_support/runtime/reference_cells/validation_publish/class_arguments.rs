//! Purpose:
//! Retires converted ReflectionClass arguments before their static value is published.
//!
//! Called from:
//! - The checked static publisher's explicit class-call entry.
//!
//! Key details:
//! - The converted candidate stays rooted while original argument destructors run.
//! - Caller parking and snapshot roots are cleared before protected retirement.
//! - A retirement Throwable stays pending while the successful conversion is published.

use super::*;

const FRAME: usize = 80;
const CONTEXT: usize = 8;
const SNAPSHOT: usize = 16;
const ARGUMENT: usize = 24;
const CANDIDATE: usize = 32;
const PIN: usize = 64;

/// Emits retire(context, snapshot_record, caller_owner_slot, owned_candidate) with owned result.
pub(super) fn emit(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    emitter.label_global("__rt_reference_retire_static_argument");
    abi::emit_frame_prologue(emitter, FRAME);
    for (index, offset) in [CONTEXT, SNAPSHOT, ARGUMENT, CANDIDATE].into_iter().enumerate() {
        abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    super::enter_pin(emitter, PIN, CANDIDATE);
    abi::load_at_offset(emitter, result, CANDIDATE);
    abi::emit_call_label(emitter, "__rt_decref_mixed");

    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), SNAPSHOT);
    abi::emit_call_label(emitter, "__rt_reference_validation_pin_take");
    abi::emit_reg_move(emitter, abi::int_arg_reg_name(emitter.target, 1), result);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), CONTEXT);
    abi::emit_call_label(emitter, "__rt_reference_validation_release");

    abi::load_at_offset(emitter, scratch, ARGUMENT);
    abi::emit_load_from_address(emitter, abi::int_arg_reg_name(emitter.target, 1), scratch, 0);
    abi::emit_store_zero_to_address(emitter, scratch, 0);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), CONTEXT);
    abi::emit_call_label(emitter, "__rt_reference_validation_release");

    abi::emit_frame_slot_address(emitter, abi::int_arg_reg_name(emitter.target, 0), PIN);
    abi::emit_call_label(emitter, "__rt_reference_validation_pin_take");
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every ABI roots the converted value while source retirement can invoke and throw from PHP.
    #[test]
    fn native_static_argument_retirement_roots_and_order() {
        assert!(PIN + 16 <= FRAME);
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(target).unwrap());
            emit(&mut emitter);
            let assembly = emitter.output();
            assert!(assembly.find("__rt_reference_validation_pin_enter").unwrap()
                < assembly.find("__rt_decref_mixed").unwrap(), "{target}: retain the candidate before source cleanup");
            assert_eq!(assembly.matches("__rt_reference_validation_pin_take").count(), 2, "{target}");
            assert_eq!(assembly.matches("__rt_reference_validation_release").count(), 2, "{target}");
            assert!(assembly.rfind("__rt_reference_validation_release").unwrap()
                < assembly.rfind("__rt_reference_validation_pin_take").unwrap(), "{target}: root through both source releases");
            assert!(!assembly.contains("__rt_throw_current"), "{target}: preserve cleanup errors for the static commit");
        }
    }
}
