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
    emit_push_reg, emit_push_result_value, emit_release_temporary_stack,
    emit_store_incoming_param, materialize_outgoing_args,
};
pub use frame::{
    emit_cleanup_callback_epilogue, emit_cleanup_callback_prologue, emit_frame_prologue,
    emit_frame_restore, emit_frame_slot_address, emit_preserve_return_value,
    emit_restore_return_value, emit_return, emit_store_zero_to_local_slot, load_at_offset,
    load_at_offset_scratch, store_at_offset, store_at_offset_scratch,
};
pub use registers::{
    nested_call_reg, process_argc_reg, process_argv_reg, temp_int_reg, IncomingArgCursor,
    OutgoingArgAssignment,
};
pub use symbols::{
    emit_load_symbol_to_local_slot, emit_load_symbol_to_reg, emit_load_symbol_to_result,
    emit_store_local_slot_to_symbol, emit_store_reg_to_symbol, emit_store_result_to_symbol,
    emit_store_zero_to_symbol, emit_symbol_address,
};
pub use values::{
    emit_decref_if_refcounted, emit_incref_if_refcounted, emit_load, emit_store,
    emit_write_stdout,
};
pub(crate) use registers::{float_result_reg, int_result_reg, string_result_regs, symbol_scratch_reg};
