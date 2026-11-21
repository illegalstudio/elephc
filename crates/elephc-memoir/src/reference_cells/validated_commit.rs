//! Purpose:
//! Publishes validated reference candidates and contains displaced-owner retirement.
//!
//! Called from:
//! - Compiler native reference commit adapters.
//!
//! Key details:
//! - The wrapper adopts its candidate before retirement can invoke a destructor.

use crate::{abi, Arch, Emitter, RecordOffset};

const FRAME: usize = 64;

/// Host facts for a commit whose candidate has already passed declaration validation.
#[derive(Clone, Copy)]
pub struct Commit<'a> {
    /// Offset of the owned raw Throwable in the host validation context.
    pub pending: RecordOffset,
    /// Host entry for retiring one owner through the context.
    pub retire: &'a str,
    /// Optional entry consuming the publisher's snapshot lease after publication.
    pub take_snapshot: Option<&'a str>,
    /// Result for a candidate that cannot be published and retires without throwing.
    pub invalid: i64,
    /// Result when a pending Throwable must be reported to the coordinator.
    pub threw: i64,
}

/// Optionally consumes the successful publisher's snapshot lease at the ownership transfer point.
pub fn emit_commit(emitter: &mut impl Emitter, entry: &str, context: Commit<'_>) {
    let result = abi::int_result_reg(emitter);
    let wrapper = abi::secondary_scratch_reg(emitter);
    let pending = format!("{entry}_pending");
    let invalid = format!("{entry}_invalid");
    let done = format!("{entry}_done");
    emitter.label_global(entry);
    abi::emit_frame_prologue(emitter, FRAME);
    for (index, offset) in [8, 16, 24].into_iter().enumerate() {
        abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter, index), offset);
    }
    if context.take_snapshot.is_some() {
        abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter, 3), 40);
    }
    abi::load_at_offset(emitter, result, 8);
    abi::emit_load_from_address(emitter, result, result, context.pending.bytes());
    abi::emit_branch_if_int_result_nonzero(emitter, &pending);
    abi::load_at_offset(emitter, result, 24);
    abi::emit_branch_if_int_result_zero(emitter, &invalid);
    abi::load_at_offset(emitter, result, 16);
    abi::emit_call_label(emitter, "__rt_reference_is");
    abi::emit_branch_if_int_result_zero(emitter, &invalid);
    abi::load_at_offset(emitter, wrapper, 16);
    abi::emit_load_from_address(emitter, result, wrapper, 8);
    abi::store_at_offset(emitter, result, 32);
    abi::load_at_offset(emitter, result, 24);
    abi::emit_store_to_address(emitter, result, wrapper, 8);
    // The wrapper now owns the candidate. Never release that transferred owner again.
    if let Some(take_snapshot) = context.take_snapshot {
        abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter, 0), 40);
        abi::emit_call_label(emitter, take_snapshot);
        abi::store_at_offset(emitter, result, 24);
        emit_release_slot(emitter, 24, context.retire);
    }
    emit_release_slot(emitter, 32, context.retire);
    if context.take_snapshot.is_some() {
        // A snapshot destructor may throw even when the displaced child's release succeeds.
        abi::load_at_offset(emitter, result, 8);
        abi::emit_load_from_address(emitter, result, result, context.pending.bytes());
        let complete = format!("{entry}_complete");
        abi::emit_branch_if_int_result_zero(emitter, &complete);
        abi::emit_load_int_immediate(emitter, result, context.threw);
        abi::emit_jump(emitter, &done);
        emitter.label(&complete);
        abi::emit_load_int_immediate(emitter, result, 0);
    }
    abi::emit_jump(emitter, &done);
    emitter.label(&pending);
    emit_release_slot(emitter, 24, context.retire);
    abi::emit_load_int_immediate(emitter, result, context.threw);
    abi::emit_jump(emitter, &done);
    emitter.label(&invalid);
    emit_release_slot(emitter, 24, context.retire);
    abi::emit_branch_if_int_result_nonzero(emitter, &done);
    abi::emit_load_int_immediate(emitter, result, context.invalid);
    emitter.label(&done);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
}

/// Retires one owned frame value through the context's protected release operation.
fn emit_release_slot(emitter: &mut impl Emitter, offset: usize, retire: &str) {
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter, 0), 8);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter, 1), offset);
    abi::emit_call_label(emitter, retire);
}

/// Consumes a candidate and reports any new throw through the caller context.
pub fn emit_release(
    emitter: &mut impl Emitter,
    entry: &str,
    pending: RecordOffset,
    release: &str,
    throw_status: i64,
) {
    let result = abi::int_result_reg(emitter);
    let done = &format!("{entry}_done");
    emitter.label_global(entry);
    abi::emit_frame_prologue(emitter, 32);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter, 0), 8);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter, 1), 16);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter, 2), 8);
    match emitter.arch() {
        Arch::AArch64 => emitter.instruction(&format!("add x2, x2, #{}", pending.bytes())), // accumulate a contained Throwable in this validation's owned slot
        Arch::X86_64 => emitter.instruction(&format!("add rdx, {}", pending.bytes())), // accumulate a contained Throwable in this validation's owned slot
    }
    abi::emit_symbol_address(emitter, abi::int_arg_reg_name(emitter, 0), release);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter, 1), 16);
    abi::emit_call_label(emitter, "__rt_cleanup_invoke");
    abi::emit_branch_if_int_result_zero(emitter, done);
    abi::emit_load_int_immediate(emitter, result, throw_status);
    emitter.label(done);
    abi::emit_frame_restore(emitter, 32);
    abi::emit_return(emitter);
}
