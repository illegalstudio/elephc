//! Purpose:
//! Transfers a pending native reference argument binding into its formal parameter owner.
//!
//! Called from:
//! - Native by-reference parameter entry after ABI arguments have reached their local slots.
//!
//! Key details:
//! - An activated argument record identifies the native callee's incoming stack pointer.
//! - Claiming clears the caller owner before transferring it, then consumes eval staging leases.
//! - Ordinary variables and legacy callers still acquire an independent parameter owner.

use super::*;

/// Emits C-ABI acquire(cell, incoming_sp), accepting only exact live managed reference cells.
pub fn emit(emitter: &mut impl Emitter) {
    let result = abi::int_result_reg(emitter);
    let (cursor, scratch, expected) = if emitter.arch() == Arch::AArch64 {
        ("x10", "x11", "x9")
    } else { ("r10", "r11", "r9") };
    let next = "__rt_reference_parameter_acquire_next";
    let advance = "__rt_reference_parameter_acquire_advance";
    let retain = "__rt_reference_parameter_acquire_retain";
    let acquired = "__rt_reference_parameter_acquire_acquired";
    let consume = "__rt_reference_parameter_acquire_consume";
    let staging = "__rt_reference_parameter_acquire_staging";
    let done = "__rt_reference_parameter_acquire_done";
    emitter.blank();
    emitter.label_global("__rt_reference_parameter_acquire");
    abi::emit_frame_prologue(emitter, 64);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter, 0), 8);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter, 1), 16);
    abi::emit_reg_move(emitter, result, abi::int_arg_reg_name(emitter, 0));
    abi::emit_call_label(emitter, "__rt_reference_cell_owner");
    abi::store_at_offset(emitter, result, 40);
    abi::emit_branch_if_int_result_zero(emitter, done);
    abi::emit_symbol_address(emitter, scratch, "__rt_cleanup_call_reference_argument");
    abi::store_at_offset(emitter, scratch, 24);
    abi::emit_load_symbol_to_reg(emitter, cursor, "_exc_call_frame_top", 0);
    emitter.label(next);
    branch_zero(emitter, cursor, retain);
    abi::emit_load_from_address(emitter, scratch, cursor, 8);
    abi::load_at_offset(emitter, expected, 24);
    branch_not_equal(emitter, scratch, expected, advance);
    abi::emit_load_from_address(emitter, scratch, cursor, 32);
    abi::load_at_offset(emitter, expected, 16);
    branch_not_equal(emitter, scratch, expected, advance);
    abi::emit_load_from_address(emitter, scratch, cursor, 16);
    abi::emit_load_from_address(emitter, result, scratch, 0);
    abi::load_at_offset(emitter, expected, 8);
    branch_not_equal(emitter, result, expected, advance);
    abi::emit_store_zero_to_address(emitter, scratch, 0);
    abi::emit_store_zero_to_address(emitter, cursor, 32);
    abi::emit_jump(emitter, acquired);
    emitter.label(advance);
    abi::emit_load_from_address(emitter, cursor, cursor, 0);
    abi::emit_jump(emitter, next);
    emitter.label(retain);
    abi::load_at_offset(emitter, result, 8);
    abi::emit_call_label(emitter, "__rt_incref");
    emitter.label(acquired);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter, 0), 8);
    abi::emit_call_label(emitter, "__rt_reference_argument_pins_staging");
    abi::store_at_offset(emitter, result, 32);
    emitter.label(consume);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter, 0), 8);
    abi::emit_call_label(emitter, "__rt_reference_argument_pins_activate");
    abi::emit_branch_if_int_result_zero(emitter, staging);
    // The acquired cell still owns this source value, so dropping its extra lease cannot run PHP.
    abi::emit_call_label(emitter, "__rt_decref_any");
    abi::emit_jump(emitter, consume);
    emitter.label(staging);
    abi::load_at_offset(emitter, result, 32);
    abi::emit_branch_if_int_result_zero(emitter, done);
    abi::emit_reg_move(emitter, abi::int_arg_reg_name(emitter, 0), result);
    abi::emit_call_label(emitter, "__rt_reference_validation_pin_take");
    // The formal owner now replaces staging, including when the last PHP binding is unset.
    abi::emit_call_label(emitter, "__rt_decref_any");
    emitter.label(done);
    // Failed authentication returns null rather than adopting an arbitrary incoming address.
    // The early branch retains that null result; successful paths reload their authenticated cell.
    abi::load_at_offset(emitter, result, 40);
    abi::emit_frame_restore(emitter, 64);
    abi::emit_return(emitter);
}

/// Skips a record that belongs to another call activation or another cell owner.
fn branch_not_equal(emitter: &mut impl Emitter, left: &str, right: &str, target: &str) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {left}, {right}"));               // compare exact native record identity
            emitter.instruction(&format!("b.ne {target}"));                     // unrelated records remain untouched
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {left}, {right}"));               // compare exact native record identity
            emitter.instruction(&format!("jne {target}"));                      // unrelated records remain untouched
        }
    }
}

/// Detects the end of the activation chain without borrowing an implicit scratch register.
fn branch_zero(emitter: &mut impl Emitter, left: &str, target: &str) {
    match emitter.arch() {
        Arch::AArch64 => emitter.instruction(&format!("cbz {left}, {target}")), // older or ordinary callers need a retained parameter owner
        Arch::X86_64 => {
            emitter.instruction(&format!("test {left}, {left}"));               // inspect the current activation cursor
            emitter.instruction(&format!("jz {target}"));                       // older or ordinary callers need a retained parameter owner
        }
    }
}
