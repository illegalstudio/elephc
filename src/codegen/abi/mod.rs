mod bootstrap;
mod calls;
mod frame;
mod registers;
mod symbols;
#[cfg(test)]
mod tests;
mod values;

pub use bootstrap::{
    emit_copy_frame_pointer, emit_enable_heap_debug_flag, emit_exit,
    emit_store_process_args_to_globals,
};
pub use calls::{
    build_outgoing_arg_assignments_for_target, emit_call_label, emit_call_reg, emit_pop_reg,
    emit_pop_float_reg, emit_pop_reg_pair, emit_push_float_reg, emit_push_reg,
    emit_push_reg_pair, emit_push_result_value, emit_release_temporary_stack,
    emit_reserve_temporary_stack, emit_store_incoming_param, emit_temporary_stack_address,
    emit_load_temporary_stack_slot, materialize_outgoing_args,
};
pub use frame::{
    emit_cleanup_callback_epilogue, emit_cleanup_callback_prologue, emit_frame_prologue,
    emit_frame_restore, emit_frame_slot_address, emit_load_from_address,
    emit_preserve_return_value, emit_restore_return_value, emit_return,
    emit_store_to_address, emit_store_zero_to_address, emit_store_zero_to_local_slot, load_at_offset,
    load_at_offset_scratch, store_at_offset, store_at_offset_scratch,
};
pub use registers::{
    nested_call_reg, process_argc_reg, process_argv_reg, temp_int_reg, IncomingArgCursor,
    OutgoingArgAssignment,
};
pub use symbols::{
    emit_load_extern_symbol_to_reg, emit_load_symbol_to_local_slot, emit_load_symbol_to_reg,
    emit_load_symbol_to_result, emit_store_local_slot_to_symbol,
    emit_store_reg_to_extern_symbol, emit_store_reg_to_symbol, emit_store_result_to_symbol,
    emit_store_zero_to_symbol, emit_symbol_address,
};
pub use values::{
    emit_branch_if_int_result_nonzero, emit_branch_if_int_result_zero, emit_load_int_immediate,
    emit_decref_if_refcounted, emit_float_result_to_int_result, emit_incref_if_refcounted,
    emit_int_result_to_float_result, emit_jump, emit_load, emit_store, emit_write_stdout,
};
pub(crate) use registers::{float_result_reg, int_result_reg, string_result_regs, symbol_scratch_reg};
