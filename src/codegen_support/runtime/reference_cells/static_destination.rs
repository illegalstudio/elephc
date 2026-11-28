//! Purpose:
//! Restores an unaliased static setter destination after conversion re-enters PHP.
//!
//! Called from:
//! - The checked ReflectionClass commit after publishing its converted candidate.
//!
//! Key details:
//! - An originally shared reference keeps its captured destination instead.
//! - The displaced cell owner is returned for cleanup after the entire publication.
//! - This transition runs no PHP and preserves the binding's immutable declaration.

use super::*;

/// Emits restore(binding, borrowed_wrapper), transferring the displaced cell or returning zero.
pub(super) fn emit(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    let unchanged = "__rt_reference_static_destination_unchanged";
    let done = "__rt_reference_static_destination_done";
    emitter.label_global("__rt_reference_static_destination_restore");
    abi::emit_frame_prologue(emitter, 64);
    for (index, offset) in [8, 16].into_iter().enumerate() {
        abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    abi::load_at_offset(emitter, result, 8);
    abi::emit_load_from_address(emitter, result, result, 0);
    abi::store_at_offset(emitter, result, 24);
    abi::emit_call_label(emitter, "__rt_reference_cell_canonical");
    abi::load_at_offset(emitter, scratch, 16);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("cmp {result}, {scratch}")), // preserve an unchanged physical destination
        Arch::X86_64 => emitter.instruction(&format!("cmp {result}, {scratch}")), // preserve an unchanged physical destination
    }
    abi::emit_branch_if_equal_wide(emitter, unchanged);
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 0), 7);
    abi::emit_call_label(emitter, "__rt_reference_cell_new");
    abi::store_at_offset(emitter, result, 32);
    abi::load_at_offset(emitter, result, 16);
    abi::emit_call_label(emitter, "__rt_incref");
    abi::load_at_offset(emitter, scratch, 32);
    abi::emit_store_to_address(emitter, result, scratch, 0);
    for (index, offset) in [8, 32].into_iter().enumerate() {
        abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    let entry = emitter.target.extern_symbol("__elephc_reference_sources_rebind_v1");
    abi::emit_call_label(emitter, &entry);
    super::source_lifecycle::require_success(emitter);
    abi::load_at_offset(emitter, result, 32);
    super::source_lifecycle::associate_result(emitter);
    abi::load_at_offset(emitter, scratch, 8);
    abi::emit_store_to_address(emitter, result, scratch, 0);
    abi::emit_store_zero_to_address(emitter, scratch, 8);
    abi::emit_load_int_immediate(emitter, result, 2);
    abi::emit_store_to_address(emitter, result, scratch, 16);
    abi::load_at_offset(emitter, result, 24);
    abi::emit_jump(emitter, done);
    emitter.label(unchanged);
    abi::emit_load_int_immediate(emitter, result, 0);
    emitter.label(done);
    abi::emit_frame_restore(emitter, 64);
    abi::emit_return(emitter);
}
