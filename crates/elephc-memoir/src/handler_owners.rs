//! Purpose:
//! Transfers handler records between active slots and their saved stack.
//!
//! Called from:
//! - Native error and exception handler adapters.
//!
//! Key details:
//! - The caller supplies field order, native labels and final exception policy.

use crate::{Arch, Emitter};

/// Frame offset of the pending exception owner shared with the host completion routine.
pub const PENDING: usize = 8;

/// Publishes a previous handler record into its stack after saving each active field.
/// Eval's C entry uses explicit displacement syntax for the node's link word.
pub fn emit_save_node(
    emitter: &mut impl Emitter, node: &str, scratch: &str, fields: &[&str], explicit_link_offset: bool,
) {
    for (index, symbol) in fields.iter().enumerate() {
        let offset = index * 8;
        emitter.emit_load_symbol_to_reg(scratch, symbol, 0);
        if explicit_link_offset && offset == 0 {
            match emitter.arch() {
                Arch::AArch64 => emitter.instruction(&format!("str {scratch}, [{node}, #0]")), // save the previous stack link
                Arch::X86_64 => emitter.instruction(&format!("mov QWORD PTR [{node} + 0], {scratch}")), // save the previous stack link
            }
        } else {
            emitter.emit_store_to_address(scratch, node, offset);
        }
    }
    emitter.emit_store_reg_to_symbol(node, fields[0], 0);
}

/// Releases a detached snapshot, keeping captured contexts alive until descriptors are gone.
pub fn emit_release<E: Emitter>(emitter: &mut E, prefix: &str, active: [&str; 4], finish: fn(&mut E, &str, usize)) {
    let stack = if emitter.arch() == Arch::AArch64 { "sp" } else { "rsp" };
    let result = emitter.int_result_reg();
    emitter.blank();
    emitter.label_global(&format!("{prefix}_release"));
    emitter.emit_frame_prologue(64);
    emitter.emit_store_zero_to_local_slot(PENDING);
    for (index, symbol) in active.into_iter().enumerate() {
        emitter.emit_load_symbol_to_reg(result, &symbol, 0);
        emitter.emit_store_to_address(result, stack, index * 8);
        emitter.emit_store_zero_to_symbol(symbol, 0);
    }
    for (offset, helper) in [(0, "__rt_decref_mixed"), (8, "__rt_callable_descriptor_release")] {
        let skip = format!("{prefix}_release_skip_{offset}");
        emitter.emit_load_from_address(result, stack, offset);
        emitter.emit_branch_if_int_result_zero(&skip);
        emitter.emit_guarded_cleanup_call(helper, result, PENDING);
        emitter.label(&skip);
    }
    let done = format!("{prefix}_release_done");
    emitter.emit_load_from_address(result, stack, 24);
    emitter.emit_branch_if_int_result_zero(&done);
    emitter.emit_reg_move(emitter.int_arg_reg_name(0), result);
    emitter.emit_load_from_address(emitter.int_arg_reg_name(1), stack, 16);
    emitter.emit_frame_slot_address(emitter.int_arg_reg_name(2), PENDING);
    emitter.emit_call_label("__rt_cleanup_invoke");
    emitter.label(&done);
    finish(emitter, &format!("{prefix}_release_return"), 64);
}

/// Clears the active handler before checking whether a previous registration exists.
pub fn emit_pop<E: Emitter>(emitter: &mut E, prefix: &str, fields: &[&str], finish: fn(&mut E, &str, usize)) {
    let stack = if emitter.arch() == Arch::AArch64 { "sp" } else { "rsp" };
    let result = emitter.int_result_reg();
    let scratch = emitter.secondary_scratch_reg();
    let done = format!("{prefix}_pop_done");
    emitter.blank();
    emitter.label_global(&format!("{prefix}_pop"));
    emitter.emit_frame_prologue(48);
    emitter.emit_store_zero_to_local_slot(PENDING);
    emitter.emit_load_int_immediate(result, 0);
    emitter.emit_guarded_cleanup_call(
        &format!("{prefix}_release"), result, PENDING,
    );
    emitter.emit_load_symbol_to_reg(result, fields[0], 0);
    emitter.emit_branch_if_int_result_zero(&done);
    emitter.emit_store_to_address(result, stack, 0);
    for (index, symbol) in fields.iter().enumerate() {
        emitter.emit_load_from_address(result, stack, 0);
        emitter.emit_load_from_address(scratch, result, index * 8);
        emitter.emit_store_reg_to_symbol(scratch, symbol, 0);
    }
    emitter.emit_load_from_address(result, stack, 0);
    emitter.emit_call_label("__rt_heap_free");
    emitter.label(&done);
    finish(emitter, &format!("{prefix}_pop_return"), 48);
}
