//! Purpose:
//! Emits depth-stamped owner-node allocation and prefix detachment.
//!
//! Called from:
//! - Compiler scoped-owner runtime adapters.
//!
//! Key details:
//! - Each node owns its payload and stores next, payload and scope depth in three words.
//! - The host supplies state symbols; legacy entry labels preserve the existing runtime ABI.
//! - Detachment invokes no PHP cleanup; completion and exception propagation stay in the host.

use crate::{abi, Arch, Emitter, ScopedOwnerState};

/// Consumes one data owner into a depth-stamped context node without changing its payload.
///
/// A direct hydration-hook call has no active decoder boundary to drain the
/// list, so depth zero retires the value immediately instead of publishing it.
pub fn emit_defer_data(emitter: &mut impl Emitter, state: ScopedOwnerState<'_>) {
    let head_symbol = state.head;
    let depth_symbol = state.depth;
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    emitter.blank();
    emitter.label_global("__rt_unserialize_defer_data");
    abi::emit_frame_prologue(emitter, 32);
    abi::store_at_offset(emitter, result, 8);
    abi::emit_load_symbol_to_reg(emitter, scratch, depth_symbol, 0);
    abi::emit_branch_if_zero_wide(emitter, scratch, "__rt_unserialize_defer_data_direct");
    abi::emit_load_int_immediate(emitter, result, 24);
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    abi::emit_load_symbol_to_reg(emitter, scratch, head_symbol, 0);
    abi::emit_store_to_address(emitter, scratch, result, 0);
    abi::load_at_offset(emitter, scratch, 8);
    abi::emit_store_to_address(emitter, scratch, result, 8);
    abi::emit_load_symbol_to_reg(emitter, scratch, depth_symbol, 0);
    abi::emit_store_to_address(emitter, scratch, result, 16);
    abi::emit_store_reg_to_symbol(emitter, result, head_symbol, 0);
    abi::emit_jump(emitter, "__rt_unserialize_defer_data_return");
    emitter.label("__rt_unserialize_defer_data_direct");
    abi::load_at_offset(emitter, result, 8);
    abi::emit_call_label(emitter, "__rt_decref_any");
    emitter.label("__rt_unserialize_defer_data_return");
    abi::emit_frame_restore(emitter, 32);
    abi::emit_return(emitter);
}

/// Removes the current parser's list prefix, preserving every still-active outer context node.
pub fn emit_detach_data(emitter: &mut impl Emitter, state: ScopedOwnerState<'_>) {
    let head_symbol = state.head;
    let depth_symbol = state.depth;
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    let depth = abi::tertiary_scratch_reg(emitter);
    emitter.blank();
    emitter.label_global("__rt_unserialize_detach_data");
    abi::emit_frame_prologue(emitter, 48);
    abi::emit_load_symbol_to_reg(emitter, result, head_symbol, 0);
    abi::store_at_offset(emitter, result, 8);
    abi::emit_store_zero_to_local_slot(emitter, 16);
    abi::emit_load_symbol_to_reg(emitter, depth, depth_symbol, 0);
    emitter.label("__rt_unserialize_detach_data_loop");
    abi::emit_branch_if_int_result_zero(emitter, "__rt_unserialize_detach_data_done");
    abi::emit_load_from_address(emitter, scratch, result, 16);
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {scratch}, {depth}"));            // compare the node depth with the ABI-selected active-depth register
            emitter.instruction("b.lo __rt_unserialize_detach_data_done");      // leave the outer parser's owner list published
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {scratch}, {depth}"));            // compare the node depth with the ABI-selected active-depth register
            emitter.instruction("jb __rt_unserialize_detach_data_done");        // leave the outer parser's owner list published
        }
    }
    abi::store_at_offset(emitter, result, 16);
    abi::emit_load_from_address(emitter, result, result, 0);
    abi::emit_jump(emitter, "__rt_unserialize_detach_data_loop");
    emitter.label("__rt_unserialize_detach_data_done");
    abi::store_at_offset(emitter, result, 24);
    abi::load_at_offset(emitter, result, 16);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_unserialize_detach_data_return");
    abi::emit_store_zero_to_address(emitter, result, 0);
    abi::load_at_offset(emitter, result, 24);
    abi::emit_store_reg_to_symbol(emitter, result, head_symbol, 0);
    abi::load_at_offset(emitter, result, 8);
    emitter.label("__rt_unserialize_detach_data_return");
    abi::emit_frame_restore(emitter, 48);
    abi::emit_return(emitter);
}
