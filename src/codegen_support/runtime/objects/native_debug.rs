//! Purpose:
//! Renders native object property snapshots without exposing prelude storage.
//!
//! Called from:
//! - The recursive var_dump and print_r object walkers on every supported target.
//!
//! Key details:
//! - A protected zero-argument hook returns a cell; a separate adapter produces the owned hash.
//! - Cleanup records protect the receiver, snapshot, indentation, and recursion state.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use crate::types::PhpType;

const FRAME_BYTES: usize = 160;
const RECEIVER: usize = 8;
const BASE: usize = 16;
const MODE: usize = 24;
const HOOK: usize = 32;
const PROJECTION: usize = 40;
// A cleanup callback receives the address of HASH. Following words are saved
// indentation, recursion depth, and an independently retained receiver.
const HASH: usize = 128;
const INDENT: usize = HASH - 8;
const SEEN: usize = HASH - 16;
const OWNER: usize = HASH - 24;
const BUFFER: usize = HASH - 32;
const PRINT_MODE: usize = HASH - 40;
const PRINT_OFFSET: usize = HASH - 48;
const CAPACITY: usize = HASH - 56;
const CELL: usize = HASH - 64;

/// Emits native snapshot dispatch and its non-escaping unwind cleanup callback.
pub(crate) fn emit_native_debug(emitter: &mut Emitter) {
    super::super::exceptions::emit_protected_status(emitter, "__rt_native_debug_call", protected_projection);
    emit_native_debug_cleanup(emitter);
    emitter.blank();
    emitter.label_global("__rt_try_native_debug_object");
    abi::emit_frame_prologue(emitter, FRAME_BYTES);
    for (index, offset) in [(0, RECEIVER), (1, BASE), (2, MODE)] {
        abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    emit_lookup_hook(emitter);
    let result = abi::int_result_reg(emitter);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_native_debug_miss");
    abi::store_at_offset(emitter, result, HOOK);
    load_argument(emitter, 0, RECEIVER);
    abi::emit_call_label(emitter, "__rt_vd_seen_find");
    abi::emit_branch_if_int_result_nonzero(emitter, "__rt_native_debug_miss");
    for (offset, symbol) in [(INDENT, "_vd_indent"), (SEEN, "_vd_seen_n")] {
        abi::emit_load_symbol_to_reg(emitter, result, symbol, 0);
        abi::store_at_offset(emitter, result, offset);
    }
    abi::emit_store_zero_to_local_slot(emitter, HASH);
    abi::emit_store_zero_to_local_slot(emitter, OWNER);
    abi::emit_store_zero_to_local_slot(emitter, BUFFER);
    abi::emit_store_zero_to_local_slot(emitter, CELL);
    let address = abi::tertiary_scratch_reg(emitter);
    abi::emit_frame_slot_address(emitter, address, HASH);
    abi::emit_push_call_operand_owner(emitter, address, false);
    abi::emit_symbol_address(emitter, address, "__rt_cleanup_native_debug");
    abi::emit_store_to_sp(emitter, address, 8);
    abi::load_at_offset(emitter, result, RECEIVER);
    abi::emit_incref_if_refcounted(emitter, &PhpType::Object(String::new()));
    abi::store_at_offset(emitter, result, OWNER);
    abi::load_at_offset(emitter, result, MODE);
    abi::emit_branch_if_int_result_nonzero(emitter, "__rt_native_debug_hook_unprotected_object");
    load_argument(emitter, 0, RECEIVER);
    abi::emit_call_label(emitter, "__rt_vd_seen_push");
    abi::emit_call_label(emitter, "__rt_vd_pad");
    emitter.label("__rt_native_debug_hook_unprotected_object");
    save_output_state(emitter);
    load_argument(emitter, 0, RECEIVER);
    load_argument(emitter, 1, HOOK);
    abi::emit_frame_slot_address(emitter, abi::int_arg_reg_name(emitter.target, 2), CELL);
    abi::emit_call_label(emitter, "__rt_native_debug_call");
    abi::emit_branch_if_int_result_nonzero(emitter, "__rt_native_debug_hook_failed");
    load_argument(emitter, 0, RECEIVER);
    load_argument(emitter, 1, CELL);
    abi::load_at_offset(emitter, address, PROJECTION);
    abi::emit_call_reg(emitter, address);
    abi::store_at_offset(emitter, result, HASH);
    abi::load_at_offset(emitter, result, CELL);
    abi::emit_store_zero_to_local_slot(emitter, CELL);
    abi::emit_call_label(emitter, "__rt_decref_any");
    restore_output_state(emitter);
    restore_walk_state(emitter);
    load_argument(emitter, 0, RECEIVER);
    abi::emit_call_label(emitter, "__rt_vd_seen_push");
    abi::load_at_offset(emitter, result, MODE);
    abi::emit_branch_if_int_result_nonzero(emitter, "__rt_native_debug_print_r");

    load_argument(emitter, 0, HASH);
    abi::emit_call_label(emitter, "__rt_hash_count");
    abi::emit_reg_move(emitter, abi::int_arg_reg_name(emitter.target, 1), result);
    load_argument(emitter, 0, RECEIVER);
    abi::emit_store_zero_to_symbol(emitter, "_vd_indent", 0);
    abi::emit_call_label(emitter, "__rt_var_dump_open_object_with_count");
    abi::load_at_offset(emitter, abi::secondary_scratch_reg(emitter), INDENT);
    abi::emit_store_reg_to_symbol(emitter, abi::secondary_scratch_reg(emitter), "_vd_indent", 0);
    abi::emit_call_label(emitter, "__rt_vd_indent_push");
    load_argument(emitter, 0, HASH);
    abi::emit_call_label(emitter, "__rt_var_dump_hash");
    abi::emit_call_label(emitter, "__rt_vd_indent_pop");
    abi::emit_call_label(emitter, "__rt_var_dump_close_container");
    abi::emit_jump(emitter, "__rt_native_debug_rendered");

    emitter.label("__rt_native_debug_print_r");
    load_argument(emitter, 0, RECEIVER);
    abi::emit_call_label(emitter, "__rt_print_r_object_header");
    load_argument(emitter, 0, BASE);
    abi::emit_call_label(emitter, "__rt_print_r_open");
    load_argument(emitter, 0, HASH);
    load_argument(emitter, 1, BASE);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("add x1, x1, #4"),                 // object entries sit four columns after the base
        Arch::X86_64 => emitter.instruction("add rsi, 4"),                      // object entries sit four columns after the base
    }
    abi::emit_call_label(emitter, "__rt_print_r_hash_entries");
    load_argument(emitter, 0, BASE);
    abi::emit_call_label(emitter, "__rt_print_r_close");

    emitter.label("__rt_native_debug_rendered");
    save_output_state(emitter);
    restore_walk_state(emitter);
    for offset in [HASH, OWNER] {
        abi::load_at_offset(emitter, result, offset);
        abi::emit_store_zero_to_local_slot(emitter, offset);
        abi::emit_call_label(emitter, "__rt_decref_any");
    }
    restore_output_state(emitter);
    restore_walk_state(emitter);
    abi::emit_pop_call_operand_owner(emitter);
    abi::emit_load_int_immediate(emitter, result, 1);
    abi::emit_jump(emitter, "__rt_native_debug_done");
    emitter.label("__rt_native_debug_hook_failed");
    abi::emit_frame_slot_address(emitter, abi::int_arg_reg_name(emitter.target, 0), HASH);
    abi::emit_call_label(emitter, "__rt_cleanup_native_debug");
    abi::emit_pop_call_operand_owner(emitter);
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 0), 2);
    abi::emit_symbol_address(emitter, abi::int_arg_reg_name(emitter.target, 1), "_native_debug_info_fatal");
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 2), elephc_builtin_contract::NATIVE_DEBUG_INFO_FATAL.len() as i64);
    abi::emit_call_label(emitter, &emitter.target.extern_symbol("write"));
    abi::emit_exit(emitter, 255);
    emitter.label("__rt_native_debug_miss");
    abi::emit_load_int_immediate(emitter, result, 0);
    emitter.label("__rt_native_debug_done");
    abi::emit_frame_restore(emitter, FRAME_BYTES);
    abi::emit_return(emitter);
}

/// Invokes only the PHP debug hook inside a complete exception boundary and stores its owned cell.
fn protected_projection(emitter: &mut Emitter) {
    let entry = abi::tertiary_scratch_reg(emitter);
    abi::emit_load_temporary_stack_slot(emitter, abi::int_arg_reg_name(emitter.target, 0), 224);
    abi::emit_load_temporary_stack_slot(emitter, entry, 232);
    abi::emit_call_reg(emitter, entry);
    let output = abi::secondary_scratch_reg(emitter);
    abi::emit_load_temporary_stack_slot(emitter, output, 240);
    abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), output, 0);
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
}

/// Loads the protected info entry and its separate, catchable projection adapter.
fn emit_lookup_hook(emitter: &mut Emitter) {
    let object = abi::int_arg_reg_name(emitter.target, 0);
    let class = abi::secondary_scratch_reg(emitter);
    let result = abi::int_result_reg(emitter);
    abi::emit_load_from_address(emitter, class, object, 0);
    abi::emit_load_symbol_to_reg(emitter, result, "_class_gc_desc_count", 0);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x10, x0");                                 // reject unsigned class ids outside the metadata table
            emitter.instruction("b.hs __rt_native_debug_lookup_miss");          // unknown classes use the ordinary property walker
        }
        Arch::X86_64 => {
            emitter.instruction("cmp r10, rax");                                // reject unsigned class ids outside the metadata table
            emitter.instruction("jae __rt_native_debug_lookup_miss");           // unknown classes use the ordinary property walker
        }
    }
    abi::emit_symbol_address(emitter, result, "_class_debug_projection_ptrs");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("add x11, x0, x10, lsl #4");                    // each class has one info entry and one projection adapter
            emitter.instruction("ldr x0, [x11]");                               // load the protected debug-info method entry
            emitter.instruction("ldr x10, [x11, #8]");                          // load the catchable projection adapter
        }
        Arch::X86_64 => {
            emitter.instruction("shl r10, 4");                                  // each class has one info entry and one projection adapter
            emitter.instruction("add r10, rax");                                // resolve the paired entries for this class id
            emitter.instruction("mov rax, QWORD PTR [r10]");                    // load the protected debug-info method entry
            emitter.instruction("mov r10, QWORD PTR [r10 + 8]");                // load the catchable projection adapter
        }
    }
    abi::store_at_offset(emitter, class, PROJECTION);
    abi::emit_jump(emitter, "__rt_native_debug_lookup_done");
    emitter.label("__rt_native_debug_lookup_miss");
    abi::emit_load_int_immediate(emitter, result, 0);
    emitter.label("__rt_native_debug_lookup_done");
}

/// Reloads a saved value directly into one target ABI argument register.
fn load_argument(emitter: &mut Emitter, index: usize, offset: usize) {
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
}

/// Restores the containing renderer after PHP callbacks or nested debug walks.
fn restore_walk_state(emitter: &mut Emitter) {
    let scratch = abi::secondary_scratch_reg(emitter);
    for (offset, symbol) in [(INDENT, "_vd_indent"), (SEEN, "_vd_seen_n")] {
        abi::load_at_offset(emitter, scratch, offset);
        abi::emit_store_reg_to_symbol(emitter, scratch, symbol, 0);
    }
}

/// Restores renderer globals and retires remaining owners during a caught PHP exception.
fn emit_native_debug_cleanup(emitter: &mut Emitter) {
    emitter.blank();
    emitter.label_global("__rt_cleanup_native_debug");
    abi::emit_frame_prologue(emitter, 32);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), 8);
    let state = abi::tertiary_scratch_reg(emitter);
    let result = abi::int_result_reg(emitter);
    restore_callback_walk(emitter);
    discard_capture(emitter);
    for (offset, release) in [(0, "__rt_decref_any"), (24, "__rt_decref_any"),
        (32, "__rt_heap_free_safe"), (64, "__rt_decref_any")] {
        abi::load_at_offset(emitter, state, 8);
        abi::emit_load_from_address(emitter, result, state, offset);
        abi::emit_store_zero_to_address(emitter, state, offset);
        abi::emit_unary_cleanup_preserving_exception(emitter, release, result);
    }
    restore_callback_walk(emitter);
    discard_capture(emitter);
    abi::emit_frame_restore(emitter, 32);
    abi::emit_return(emitter);
}

/// Moves the outer capture owner into the guarded frame before arbitrary PHP code runs.
fn save_output_state(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    for (offset, symbol) in [(BUFFER, "_print_r_ptr"), (CAPACITY, "_print_r_cap"),
        (PRINT_MODE, "_print_r_mode"), (PRINT_OFFSET, "_print_r_off")] {
        abi::emit_load_symbol_to_reg(emitter, result, symbol, 0);
        abi::store_at_offset(emitter, result, offset);
        abi::emit_store_zero_to_symbol(emitter, symbol, 0);
    }
}

/// Transfers the saved capture owner back after nested PHP calls have finalized their own output.
fn restore_output_state(emitter: &mut Emitter) {
    discard_capture(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    for (offset, symbol) in [(BUFFER, "_print_r_ptr"), (CAPACITY, "_print_r_cap"),
        (PRINT_MODE, "_print_r_mode"), (PRINT_OFFSET, "_print_r_off")] {
        abi::load_at_offset(emitter, scratch, offset);
        abi::emit_store_reg_to_symbol(emitter, scratch, symbol, 0);
    }
    abi::emit_store_zero_to_local_slot(emitter, BUFFER);
}

/// Discards an abandoned raw capture without retaining a stale pointer across request reset.
fn discard_capture(emitter: &mut Emitter) {
    abi::emit_call_label(emitter, "__rt_pr_abort");
}

/// Restores indentation and recursion depth; balanced nested walks never overwrite the older prefix.
fn restore_callback_walk(emitter: &mut Emitter) {
    let state = abi::tertiary_scratch_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    for (offset, symbol) in [(8, "_vd_indent"), (16, "_vd_seen_n")] {
        abi::load_at_offset(emitter, state, 8);
        abi::emit_load_from_address(emitter, scratch, state, offset);
        abi::emit_store_reg_to_symbol(emitter, scratch, symbol, 0);
    }
}
