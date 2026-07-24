//! Purpose:
//! Collects target ABI helpers for frames, registers, calls, symbols, bootstrap, and values.
//! Provides the stable API used by higher-level codegen without exposing architecture details.
//!
//! Called from:
//! - `crate::codegen_support::*` expression, statement, function, and runtime-facing emitters
//!
//! Key details:
//! - Shared lowering should go through this module instead of hardcoding platform registers or stack rules.

mod bootstrap;
mod callbacks;
mod calls;
mod frame;
mod registers;
mod symbols;
#[cfg(test)]
mod tests;
mod values;

#[cfg(test)]
pub use bootstrap::emit_copy_frame_pointer;
pub use bootstrap::{
    emit_enable_heap_debug_flag, emit_exit, emit_exit_with_result_reg,
    emit_store_process_args_to_globals,
};
pub use callbacks::{
    c_callback_internal_symbol, c_callback_stack_arg_offset, emit_c_callback_entry,
    emit_windows_c_abi_registers_for_runtime_helper,
};
pub use calls::{
    build_c_abi_outgoing_arg_assignments_for_target, build_outgoing_arg_assignments_for_target,
    compact_windows_c_abi_stack_args, emit_call_label, emit_call_reg,
    emit_load_temporary_stack_slot, emit_pop_float_reg, emit_pop_reg, emit_pop_reg_pair,
    emit_push_float_reg, emit_push_reg, emit_push_reg_pair, emit_push_result_value,
    emit_release_temporary_stack, emit_reserve_temporary_stack, emit_store_incoming_param,
    emit_store_to_sp, emit_temporary_stack_address, materialize_outgoing_args,
    outgoing_call_stack_pad_bytes,
};
pub use frame::{
    emit_frame_prologue, emit_frame_restore, emit_frame_slot_address, emit_load_from_address,
    emit_reg_move, emit_return, emit_store_to_address, emit_store_zero_to_address,
    emit_store_zero_to_local_slot, load_at_offset, load_at_offset_scratch, load_from_caller_stack,
    store_at_offset, store_at_offset_scratch,
};
#[cfg(test)]
pub use frame::{emit_preserve_return_value, emit_restore_return_value};
pub(crate) use registers::{
    float_arg_reg_name, float_result_reg, float_spill_scratch_reg,
    int_arg_reg_name, int_result_reg, runtime_helper_int_arg_reg, secondary_scratch_reg,
    string_result_regs, symbol_scratch_reg, tertiary_scratch_reg,
};
pub use registers::{
    nested_call_reg, process_argc_reg, process_argv_reg, temp_int_reg, IncomingArgCursor,
    OutgoingArgAssignment,
};
pub use symbols::{
    emit_cmp_reg_to_symbol, emit_dec_symbol, emit_extern_symbol_address,
    emit_load_extern_symbol_to_reg, emit_load_symbol_to_reg, emit_load_symbol_to_result,
    emit_store_imm_to_symbol, emit_store_reg_to_extern_symbol, emit_store_reg_to_symbol,
    emit_store_result_to_symbol, emit_store_zero_to_symbol, emit_symbol_address,
};
#[cfg(test)]
pub use symbols::{emit_load_symbol_to_local_slot, emit_store_local_slot_to_symbol};
pub use values::{
    emit_branch_if_int_result_nonzero, emit_branch_if_int_result_zero, emit_decref_if_refcounted,
    emit_float_result_to_int_result, emit_incref_if_refcounted, emit_int_result_to_float_result,
    emit_jump, emit_load, emit_load_int_immediate, emit_release_local_ref_cell, emit_store,
    emit_write_stdout,
};
