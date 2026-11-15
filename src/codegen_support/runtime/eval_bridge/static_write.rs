//! Purpose:
//! Publishes a borrowed eval value through a checked native static declaration.
//!
//! Called from:
//! - Generated scoped static setters and eval bridge runtime emission.
//!
//! Key details:
//! - Preparation preserves uninitialized visibility until the shared static commit.
//! - The caller receives a boxed Throwable instead of a native exit through Rust.

use super::*;

const FRAME: usize = 176;
const BINDING: usize = 8;
const DECLARATION: usize = 16;
const VALUE: usize = 24;
const FLAGS: usize = 32;
const THROWABLE: usize = 40;
const WRAPPER: usize = 48;
const STATUS: usize = 56;
const CONTEXT: usize = 72;
const OUTCOME: usize = 152;

/// Emits store(binding, descriptor, borrowed_rhs, caller_flags, throwable_out).
pub(super) fn emit(emitter: &mut Emitter, native_validation: bool) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    let invalid = "__rt_eval_static_store_invalid";
    let done = "__rt_eval_static_store_done";
    label_c_global(emitter, "__elephc_eval_value_static_store_v1");
    abi::emit_frame_prologue(emitter, FRAME);
    for (index, offset) in [BINDING, DECLARATION, VALUE, FLAGS, THROWABLE].into_iter().enumerate() {
        abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    abi::load_at_offset(emitter, result, THROWABLE);
    abi::emit_branch_if_int_result_zero(emitter, invalid);
    abi::emit_store_zero_to_address(emitter, result, 0);
    if native_validation {
        abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), BINDING);
        abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 1), DECLARATION);
        abi::emit_call_label(emitter, "__rt_reference_static_mixed_prepare_store");
        abi::emit_load_from_address(emitter, result, result, 0);
        abi::store_at_offset(emitter, result, WRAPPER);
        abi::load_at_offset(emitter, result, FLAGS);
        abi::store_at_offset(emitter, result, CONTEXT);
        abi::emit_store_zero_to_local_slot(emitter, CONTEXT - 8);
        abi::load_at_offset(emitter, result, VALUE);
        abi::emit_call_label(emitter, "__rt_mixed_clone");
        abi::emit_reg_move(emitter, abi::int_arg_reg_name(emitter.target, 2), result);
        abi::emit_frame_slot_address(emitter, abi::int_arg_reg_name(emitter.target, 0), CONTEXT);
        abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 1), WRAPPER);
        abi::emit_frame_slot_address(emitter, abi::int_arg_reg_name(emitter.target, 3), OUTCOME);
        let initialized = abi::int_arg_reg_name(emitter.target, 4);
        abi::load_at_offset(emitter, initialized, BINDING);
        match emitter.target.arch {
            Arch::AArch64 => emitter.instruction(&format!("add {initialized}, {initialized}, #8")), // publish the accepted child and initialization at one commit point
            Arch::X86_64 => emitter.instruction(&format!("add {initialized}, 8")), // publish the accepted child and initialization at one commit point
        }
        abi::emit_call_label(emitter, "__rt_reference_publish_native_static");
        abi::store_at_offset(emitter, result, STATUS);
        abi::load_at_offset(emitter, result, CONTEXT - 8);
        let returned = "__rt_eval_static_store_return_status";
        abi::emit_branch_if_int_result_zero(emitter, returned);
        abi::emit_call_label(emitter, "__rt_throwable_box_owned");
        abi::load_at_offset(emitter, scratch, THROWABLE);
        abi::emit_store_to_address(emitter, result, scratch, 0);
        emitter.label(returned);
        abi::load_at_offset(emitter, result, STATUS);
        abi::emit_jump(emitter, done);
    }
    emitter.label(invalid);
    abi::emit_load_int_immediate(emitter, result, 1);
    emitter.label(done);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
}
