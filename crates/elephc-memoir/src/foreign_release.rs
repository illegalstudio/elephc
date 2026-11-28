//! Purpose:
//! Retires one external owner while returning destructor exceptions through a boxed accumulator.
//!
//! Called from:
//! - Compiler C ABI adapters for foreign-owned values and staging pins.
//!
//! Key details:
//! - The host selects transfer and exception-boxing hooks; no native unwind escapes this body.
//! - Taking an owner precedes protected cleanup, and the new-throw status survives boxing.

use crate::{abi, Emitter};

const FRAME: usize = 64;
const VALUE: usize = 8;
const OUTPUT: usize = 16;
const THROWN: usize = 24;
const CAUGHT: usize = 32;

/// Host hooks for contained retirement of a foreign-owned value or staging record.
#[derive(Clone, Copy)]
pub struct Context<'a> {
    /// Caller-unique prefix for the native control-flow labels.
    pub prefix: &'a str,
    /// Optional transfer entry that consumes a staging record and returns its owned payload.
    pub take_owner: Option<&'a str>,
    /// Entry consuming an existing boxed exception into a native pending owner.
    pub take_pending: &'a str,
    /// Entry consuming a native pending owner into the caller's boxed accumulator.
    pub box_pending: &'a str,
}

/// Emits a complete release body below the host's exported entry label.
pub fn emit_body(emitter: &mut impl Emitter, context: Context<'_>) {
    let begin = format!("{}_begin", context.prefix);
    let done = format!("{}_return", context.prefix);
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    abi::emit_frame_prologue(emitter, FRAME);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter, 0), VALUE);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter, 1), OUTPUT);
    abi::load_at_offset(emitter, scratch, OUTPUT);
    abi::emit_load_from_address(emitter, result, scratch, 0);
    abi::emit_store_zero_to_address(emitter, scratch, 0);
    abi::emit_branch_if_int_result_zero(emitter, &begin);
    abi::emit_call_label(emitter, context.take_pending);
    emitter.label(&begin);
    abi::store_at_offset(emitter, result, THROWN);
    abi::load_at_offset(emitter, result, VALUE);
    if let Some(take_owner) = context.take_owner {
        abi::emit_reg_move(emitter, abi::int_arg_reg_name(emitter, 0), result);
        abi::emit_call_label(emitter, take_owner);
    }
    // The generic release suppresses child decrefs for graph members swept by the collector.
    crate::cleanup::invoke::emit_guarded_cleanup_call(emitter, "__rt_decref_any", result, THROWN);
    abi::store_at_offset(emitter, result, CAUGHT);
    abi::load_at_offset(emitter, result, THROWN);
    abi::emit_branch_if_int_result_zero(emitter, &done);
    abi::emit_call_label(emitter, context.box_pending);
    abi::load_at_offset(emitter, scratch, OUTPUT);
    abi::emit_store_to_address(emitter, result, scratch, 0);
    emitter.label(&done);
    abi::load_at_offset(emitter, result, CAUGHT);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
}
