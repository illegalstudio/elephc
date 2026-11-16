//! Purpose:
//! Completes one frame-local cleanup while preserving the exception already being unwound.
//!
//! Called from:
//! - Exceptional PHP frame callbacks through the shared typed ABI cleanup helpers.
//!
//! Key details:
//! - C arguments are a unary release entry and its owned payload.
//! - Destructor exceptions join the active chain and never escape this cleanup call.
//! - The integer result reports whether this cleanup caught a new throw.

use crate::{abi, Emitter};

const FRAME: usize = 64;
const ENTRY: usize = 8;
const PAYLOAD: usize = 16;
const PENDING: usize = 24;
const CAUGHT: usize = 32;

/// Emits a bounded release that republishes the accumulated Throwable and returns its caught flag.
pub fn emit_cleanup_preserve_exception(emitter: &mut impl Emitter) {
    let result = abi::int_result_reg(emitter);
    emitter.blank();
    emitter.label_global("__rt_cleanup_preserve_exception");
    abi::emit_frame_prologue(emitter, FRAME);
    for (index, offset) in [ENTRY, PAYLOAD].into_iter().enumerate() {
        abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter, index), offset);
    }
    abi::emit_load_symbol_to_reg(emitter, result, "_exc_value", 0);
    abi::store_at_offset(emitter, result, PENDING);
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter, 0), ENTRY);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter, 1), PAYLOAD);
    abi::emit_frame_slot_address(emitter, abi::int_arg_reg_name(emitter, 2), PENDING);
    abi::emit_call_label(emitter, "__rt_cleanup_invoke");
    abi::store_at_offset(emitter, result, CAUGHT);
    abi::load_at_offset(emitter, result, PENDING);
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_value", 0);
    abi::load_at_offset(emitter, result, CAUGHT);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
}
