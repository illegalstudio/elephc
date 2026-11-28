//! Purpose:
//! Emits acquisition, visitation and draining of private descriptor-owner lists.
//!
//! Called from:
//! - Compiler adapters for native eval calls.
//!
//! Key details:
//! - The two-word nodes detach before release, and visitation exposes each real owner slot to GC.

use crate::{abi, Arch, Emitter};

/// Enumerates descriptor ownership as saved Fiber edges instead of treating raw nodes as roots.
pub fn emit_visitor(emitter: &mut impl Emitter, entry: &str, visit_slot: &str) {
    let result = abi::int_result_reg(emitter);
    let next = abi::secondary_scratch_reg(emitter);
    let again = &format!("{entry}_next");
    let done = &format!("{entry}_done");
    emitter.label_global(entry);
    abi::emit_frame_prologue(emitter, 48);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter, 1), 16);
    abi::emit_load_from_address(emitter, result, abi::int_arg_reg_name(emitter, 0), 0);
    abi::store_at_offset(emitter, result, 8);
    emitter.label(again);
    abi::load_at_offset(emitter, result, 8);
    abi::emit_branch_if_int_result_zero(emitter, done);
    abi::emit_load_from_address(emitter, next, result, 0);
    abi::store_at_offset(emitter, next, 8);
    match emitter.arch() {
        Arch::AArch64 => emitter.instruction("add x0, x0, #8"),                 // expose the node's actual descriptor owner slot
        Arch::X86_64 => emitter.instruction("lea rdi, [rax + 8]"),              // pass the descriptor slot rather than the raw node allocation
    }
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter, 1), 0);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter, 2), 16);
    abi::emit_call_label(emitter, visit_slot);
    abi::emit_jump(emitter, again);
    emitter.label(done);
    abi::emit_frame_restore(emitter, 48);
    abi::emit_return(emitter);
}

/// Adopts a managed descriptor in a private native node, returning the same borrowed descriptor.
pub fn emit_keep(emitter: &mut impl Emitter, entry: &str, is_managed: &str) {
    let result = abi::int_result_reg(emitter);
    let node = abi::secondary_scratch_reg(emitter);
    let head = abi::tertiary_scratch_reg(emitter);
    let done = &format!("{entry}_done");
    emitter.label_global(entry);
    abi::emit_frame_prologue(emitter, 48);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter, 0), 8);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter, 1), 16);
    abi::emit_reg_move(emitter, result, abi::int_arg_reg_name(emitter, 1));
    abi::emit_call_label(emitter, is_managed);
    abi::emit_branch_if_int_result_zero(emitter, done);
    abi::emit_load_int_immediate(emitter, result, 16);
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    abi::emit_reg_move(emitter, node, result);
    abi::load_at_offset(emitter, head, 8);
    abi::emit_load_from_address(emitter, result, head, 0);
    abi::emit_store_to_address(emitter, result, node, 0);
    abi::load_at_offset(emitter, result, 16);
    abi::emit_store_to_address(emitter, result, node, 8);
    abi::emit_store_to_address(emitter, node, head, 0);
    emitter.label(done);
    abi::load_at_offset(emitter, result, 16);
    abi::emit_frame_restore(emitter, 48);
    abi::emit_return(emitter);
}

/// Drains every owner while accumulating new throws without replacing an earlier pending chain.
pub fn emit_release(emitter: &mut impl Emitter, entry: &str, release_owner: &str) {
    let result = abi::int_result_reg(emitter);
    let head = abi::secondary_scratch_reg(emitter);
    let next = abi::tertiary_scratch_reg(emitter);
    let again = &format!("{entry}_next");
    let done = &format!("{entry}_done");
    emitter.label_global(entry);
    abi::emit_frame_prologue(emitter, 48);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter, 0), 8);
    abi::emit_store_zero_to_local_slot(emitter, 24);
    emitter.label(again);
    abi::load_at_offset(emitter, head, 8);
    abi::emit_load_from_address(emitter, result, head, 0);
    abi::emit_branch_if_int_result_zero(emitter, done);
    abi::emit_load_from_address(emitter, next, result, 0);
    abi::emit_store_to_address(emitter, next, head, 0);
    abi::emit_load_from_address(emitter, next, result, 8);
    abi::store_at_offset(emitter, next, 16);
    abi::emit_call_label(emitter, "__rt_heap_free");
    abi::emit_symbol_address(emitter, abi::int_arg_reg_name(emitter, 0), release_owner);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter, 1), 16);
    abi::emit_call_label(emitter, "__rt_cleanup_preserve_exception");
    abi::load_at_offset(emitter, next, 24);
    match emitter.arch() {
        Arch::AArch64 => emitter.instruction(&format!("orr {result}, {result}, {next}")), // preserve every newly caught cleanup throw
        Arch::X86_64 => emitter.instruction(&format!("or {result}, {next}")),   // preserve every newly caught cleanup throw
    }
    abi::store_at_offset(emitter, result, 24);
    abi::emit_jump(emitter, again);
    emitter.label(done);
    abi::load_at_offset(emitter, result, 24);
    abi::emit_frame_restore(emitter, 48);
    abi::emit_return(emitter);
}
