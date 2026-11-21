//! Purpose:
//! Gives a native staging pin ownership of a managed cell over an existing reference.
//!
//! Called from:
//! - The compiler's versioned eval reference adapter.
//!
//! Key details:
//! - The cell retains the original reference without copying its payload.
//! - Pin publication precedes retirement of the cell's allocation owner.

use crate::{Emitter, abi};

/// Retains C argument zero as the new cell payload and publishes the pin in argument one.
pub fn emit_staged_reference(emitter: &mut impl Emitter, descriptor: i64) {
    abi::emit_frame_prologue(emitter, 48);
    let result = abi::int_result_reg(emitter);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter, 1), 16);
    abi::emit_reg_move(emitter, result, abi::int_arg_reg_name(emitter, 0));
    abi::emit_call_label(emitter, "__rt_incref");
    abi::store_at_offset(emitter, result, 8);
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter, 0), descriptor);
    abi::emit_call_label(emitter, "__rt_reference_cell_new");
    let reference = abi::secondary_scratch_reg(emitter);
    abi::load_at_offset(emitter, reference, 8);
    abi::emit_store_to_address(emitter, reference, result, 0);
    abi::store_at_offset(emitter, result, 24);
    abi::emit_reg_move(emitter, abi::int_arg_reg_name(emitter, 1), result);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter, 0), 16);
    abi::emit_call_label(emitter, "__rt_reference_validation_pin_enter");
    // Transfer the fresh allocation's owner to its pin, which now keeps the cell alive.
    abi::load_at_offset(emitter, result, 24);
    abi::emit_call_label(emitter, "__rt_decref_any");
    abi::load_at_offset(emitter, result, 24);
    abi::emit_frame_restore(emitter, 48);
    abi::emit_return(emitter);
}
