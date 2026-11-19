//! Purpose:
//! Owns diagnostic fragments accumulated in a libc-allocated byte buffer.
//!
//! Called from:
//! - The compiler's warning dispatcher and request reset.
//!
//! Key details:
//! - The caller supplies its preserved input registers and working stack slots.
//! - Completed fragments leave the global pair before invoking reentrant handlers.
//! - This buffer is always freed with libc rather than the PHP heap allocator.

use crate::{abi, Arch, Emitter};

const PENDING_PTR: &str = "_rt_diag_pending_ptr";
const PENDING_LEN: &str = "_rt_diag_pending_len";

/// Stack slots used to append one fragment and park the resulting owner.
pub struct FragmentFrame {
    /// Incoming fragment pointer.
    pub input_ptr: usize,
    /// Incoming byte count.
    pub input_len: usize,
    /// Destination owner after successful allocation.
    pub buffer: usize,
    /// Updated buffer byte count.
    pub length: usize,
    /// Previous prefix byte count.
    pub prefix: usize,
}

/// Loads one captured fragment value into a native libc argument register.
fn arg(e: &mut impl Emitter, index: usize, offset: usize) {
    let register = e.int_arg_reg_name(index);
    e.emit_load_temporary_stack_slot(register, offset);
}

/// Appends a nonempty fragment to the pending buffer, growing it with checked size arithmetic.
pub fn emit_append(e: &mut impl Emitter, frame: FragmentFrame) {
    let arm = e.arch() == Arch::AArch64;
    let result = abi::int_result_reg(e);
    let a0 = e.int_arg_reg_name(0);
    let a1 = e.int_arg_reg_name(1);
    let scratch = if arm { "x10" } else { "r10" };
    arg(e, 0, frame.input_len);
    abi::emit_reg_move(e, result, a0);
    abi::emit_branch_if_int_result_zero(e, "__rt_warning_done");
    abi::emit_load_symbol_to_reg(e, scratch, PENDING_LEN, 0);
    e.emit_store_to_sp(scratch, frame.prefix);
    arg(e, 1, frame.input_len);
    e.instruction(if arm { "adds x1, x1, x10" } else { "add rsi, r10" });       // add the fragment length and detect unsigned overflow
    e.instruction(if arm { "b.hs __rt_warning_allocation_failed" } else { "jc __rt_warning_allocation_failed" }); // retain the allocator failure path
    e.emit_store_to_sp(a1, frame.length);
    abi::emit_load_symbol_to_reg(e, a0, PENDING_PTR, 0);
    let realloc = e.extern_symbol("realloc");
    abi::emit_call_label(e, &realloc);
    abi::emit_branch_if_int_result_zero(e, "__rt_warning_allocation_failed");
    e.emit_store_to_sp(result, frame.buffer);
    abi::emit_reg_move(e, a0, result);
    e.emit_load_temporary_stack_slot(scratch, frame.prefix);
    e.instruction(if arm { "add x0, x0, x10" } else { "add rdi, r10" });        // address the first byte past the prefix
    arg(e, 1, frame.input_ptr);
    arg(e, 2, frame.input_len);
    let memcpy = e.extern_symbol("memcpy");
    abi::emit_call_label(e, &memcpy);
    arg(e, 0, frame.buffer);
    arg(e, 1, frame.length);
    abi::emit_store_reg_to_symbol(e, a0, PENDING_PTR, 0);
    abi::emit_store_reg_to_symbol(e, a1, PENDING_LEN, 0);
}

/// Detaches a completed buffer so a nested warning starts with an empty pair.
pub fn emit_detach(e: &mut impl Emitter) {
    abi::emit_store_zero_to_symbol(e, PENDING_PTR, 0);
    abi::emit_store_zero_to_symbol(e, PENDING_LEN, 0);
}

/// Releases the detached warning buffer and any constructed callback/result boxes.
pub fn emit_cleanup(e: &mut impl Emitter, buffer_slot: usize, owned_cells: [usize; 2]) {
    let a0 = e.int_arg_reg_name(0);
    let result = abi::int_result_reg(e);
    e.label_global("__rt_warning_cleanup");
    abi::emit_frame_prologue(e, 32);
    e.emit_store_to_sp(a0, 0);
    for offset in owned_cells {
        e.emit_load_temporary_stack_slot(result, 0);
        let record = if e.arch() == Arch::AArch64 { "x10" } else { "r10" };
        abi::emit_reg_move(e, record, result);
        abi::emit_load_from_address(e, result, record, offset);
        abi::emit_load_int_immediate(e, if e.arch() == Arch::AArch64 { "x11" } else { "r11" }, 0);
        abi::emit_store_to_address(e, if e.arch() == Arch::AArch64 { "x11" } else { "r11" }, record, offset);
        abi::emit_call_label(e, "__rt_decref_mixed");
    }
    e.emit_load_temporary_stack_slot(a0, 0);
    abi::emit_load_from_address(e, a0, a0, buffer_slot);
    let free = e.extern_symbol("free");
    abi::emit_call_label(e, &free);
    abi::emit_frame_restore(e, 32);
    e.instruction("ret");                                                       // release warning-owned state on normal return and native unwind
}

/// Discards incomplete diagnostic fragments at request/process teardown.
pub fn emit_reset(e: &mut impl Emitter) {
    e.label_global("__rt_diag_reset");
    abi::emit_frame_prologue(e, 16);
    let a0 = e.int_arg_reg_name(0);
    abi::emit_load_symbol_to_reg(e, a0, PENDING_PTR, 0);
    let free = e.extern_symbol("free");
    abi::emit_call_label(e, &free);
    emit_detach(e);
    abi::emit_frame_restore(e, 16);
    e.instruction("ret");                                                       // never join warning fragments from distinct web requests
}

/// Appends the pending pointer and length in their existing fixed data order.
pub fn append_storage(out: &mut String, target: &impl crate::data::DataTarget) {
    for symbol in [PENDING_PTR, PENDING_LEN] {
        out.push_str(&target.common(symbol, 8));
    }
}
