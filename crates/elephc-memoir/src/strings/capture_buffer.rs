//! Purpose:
//! Emits raw capture-buffer growth, finalization and abandoned-owner retirement.
//!
//! Called from:
//! - Compiler output-capture runtime adapters.
//!
//! Key details:
//! - Growth is geometric and overflow-checked; finalization produces an owned string.
//! - The compiler callback resets routing mode after storage is cleared, preserving instruction order.
//! - Output formatting and destination selection remain compiler responsibilities.

use crate::{abi, owner_records, Arch, Emitter};

const CAPTURE_OWNER_CHAIN: &str = "_exc_call_frame_top";

/// Links a capture owner in the current temporary activation.
fn link_capture_owner(emitter: &mut impl Emitter, address: &str) {
    emitter.emit_reserve_temporary_stack(owner_records::RECORD_BYTES);
    owner_records::publish_stack(emitter, address, 0, owner_records::Context {
        head: CAPTURE_OWNER_CHAIN,
        cleanup: "__rt_cleanup_call_operand_owner",
        visitor: "__rt_gc_operand_frame_owners",
    });
}

/// Roots the raw capture across rendering and switches it into return mode.
pub fn emit_begin(emitter: &mut impl Emitter) {
    let address = emitter.tertiary_scratch_reg();
    emitter.emit_symbol_address(address, CaptureField::Buffer.symbol());
    link_capture_owner(emitter, address);
    emitter.emit_symbol_address(address, "__rt_cleanup_print_r_capture");
    emitter.emit_store_to_sp(address, crate::activation_records::CLEANUP);
    emitter.emit_call_label("__rt_pr_abort");
    let result = emitter.int_result_reg();
    emitter.emit_load_int_immediate(result, 1);
    emitter.emit_store_reg_to_symbol(result, CaptureField::Mode.symbol(), 0);
}

/// Keeps the completed owner across a possibly reentrant output callback, then releases it.
pub fn emit_write_owned(emitter: &mut impl Emitter, write: &str) {
    let (pointer, length) = abi::string_result_regs(emitter);
    emitter.emit_reserve_temporary_stack(16);
    emitter.emit_store_to_sp(pointer, 0);
    emitter.emit_store_to_sp(length, 8);
    let address = emitter.tertiary_scratch_reg();
    emitter.emit_temporary_stack_address(address, 0);
    link_capture_owner(emitter, address);
    emitter.emit_load_temporary_stack_slot(emitter.int_arg_reg_name(0), owner_records::RECORD_BYTES);
    emitter.emit_load_temporary_stack_slot(emitter.int_arg_reg_name(1), owner_records::RECORD_BYTES + 8);
    emitter.emit_call_label(write);
    let record = emitter.secondary_scratch_reg();
    emitter.comment("detach temporary call operand owner");
    emitter.emit_temporary_stack_address(record, 0);
    owner_records::unlink_record(emitter, CAPTURE_OWNER_CHAIN);
    emitter.emit_release_temporary_stack(owner_records::RECORD_BYTES);
    emitter.emit_load_temporary_stack_slot(emitter.int_result_reg(), 0);
    emitter.emit_call_label("__rt_heap_free_safe");
    emitter.emit_release_temporary_stack(16);
}

/// Detaches the live raw buffer for explicit retirement during persistent-request cleanup.
pub fn emit_take_raw_buffer(emitter: &mut impl Emitter) {
    let buffer = CaptureField::Buffer.symbol();
    emitter.emit_load_symbol_to_reg(emitter.int_result_reg(), buffer, 0);
    emitter.emit_store_zero_to_symbol(buffer, 0);
}

/// Clears the capture after its arena or raw buffer has been retired by the caller.
pub fn emit_reset(emitter: &mut impl Emitter) {
    for field in [CaptureField::Mode, CaptureField::Length, CaptureField::Buffer, CaptureField::Capacity] {
        emitter.emit_store_zero_to_symbol(field.symbol(), 0);
    }
}

/// Appends bytes into growable raw capture storage without a fixed output-size clamp.
/// Inputs are a C-ABI source pointer and byte length; zero bytes require no allocation.
pub fn emit_pr_append(emitter: &mut impl Emitter) {
    emitter.blank();
    emitter.label_global("__rt_pr_append");
    abi::emit_frame_prologue(emitter, 80);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter, 0), 8);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter, 1), 16);
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    abi::load_at_offset(emitter, result, 16);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_pr_append_done");
    for (symbol, offset) in [("_print_r_ptr", 24), ("_print_r_off", 32), ("_print_r_cap", 40)] {
        abi::emit_load_symbol_to_reg(emitter, result, symbol, 0);
        abi::store_at_offset(emitter, result, offset);
    }
    abi::load_at_offset(emitter, result, 24);
    abi::store_at_offset(emitter, result, 56);
    abi::load_at_offset(emitter, result, 32);
    abi::load_at_offset(emitter, scratch, 16);
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("adds x0, x0, x10");                            // check the required capture size before any narrowing allocation
            emitter.instruction("b.cs __rt_pr_append_overflow");                // reject a wrapped byte count through the shared fatal path
        }
        Arch::X86_64 => {
            emitter.instruction("add rax, r10");                                // check the required capture size before any narrowing allocation
            emitter.instruction("jc __rt_pr_append_overflow");                  // reject a wrapped byte count through the shared fatal path
        }
    }
    abi::store_at_offset(emitter, result, 48);
    abi::load_at_offset(emitter, result, 40);
    abi::load_at_offset(emitter, scratch, 48);
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, x10");                                 // reuse an allocation that already fits the complete append
            emitter.instruction("b.hs __rt_pr_append_ready");                   // existing capacity avoids another allocation
            emitter.instruction("cmp x0, #64");                                 // start small captures with a bounded minimum allocation
            emitter.instruction("b.hs __rt_pr_append_grow");                    // preserve an existing larger capacity
            emitter.instruction("mov x0, #64");                                 // seed geometric growth for an empty capture
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, r10");                                // reuse an allocation that already fits the complete append
            emitter.instruction("jae __rt_pr_append_ready");                    // existing capacity avoids another allocation
            emitter.instruction("cmp rax, 64");                                 // start small captures with a bounded minimum allocation
            emitter.instruction("jae __rt_pr_append_grow");                     // preserve an existing larger capacity
            emitter.instruction("mov rax, 64");                                 // seed geometric growth for an empty capture
        }
    }
    emitter.label("__rt_pr_append_grow");
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, x10");                                 // stop doubling once the pending append fits
            emitter.instruction("b.hs __rt_pr_append_allocate");                // allocate the selected capacity
            emitter.instruction("adds x0, x0, x0");                             // double geometrically without wrapping
            emitter.instruction("b.cs __rt_pr_append_overflow");                // reject capacity overflow instead of looping
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, r10");                                // stop doubling once the pending append fits
            emitter.instruction("jae __rt_pr_append_allocate");                 // allocate the selected capacity
            emitter.instruction("add rax, rax");                                // double geometrically without wrapping
            emitter.instruction("jc __rt_pr_append_overflow");                  // reject capacity overflow instead of looping
        }
    }
    abi::emit_jump(emitter, "__rt_pr_append_grow");
    emitter.label("__rt_pr_append_allocate");
    abi::store_at_offset(emitter, result, 40);
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    abi::store_at_offset(emitter, result, 56);
    abi::load_at_offset(emitter, result, 32);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_pr_append_ready");
    load_capture_argument(emitter, 0, 56);
    load_capture_argument(emitter, 1, 24);
    load_capture_argument(emitter, 2, 32);
    abi::emit_call_label(emitter, &emitter.extern_symbol("memcpy"));
    emitter.label("__rt_pr_append_ready");
    load_capture_argument(emitter, 0, 56);
    abi::load_at_offset(emitter, scratch, 32);
    match emitter.arch() {
        Arch::AArch64 => emitter.instruction("add x0, x0, x10"),                // append after the previously captured prefix
        Arch::X86_64 => emitter.instruction("add rdi, r10"),                    // append after the previously captured prefix
    }
    load_capture_argument(emitter, 1, 8);
    load_capture_argument(emitter, 2, 16);
    abi::emit_call_label(emitter, &emitter.extern_symbol("memmove"));
    for (offset, symbol) in [(56, "_print_r_ptr"), (40, "_print_r_cap"), (48, "_print_r_off")] {
        abi::load_at_offset(emitter, result, offset);
        abi::emit_store_reg_to_symbol(emitter, result, symbol, 0);
    }
    abi::load_at_offset(emitter, result, 24);
    abi::load_at_offset(emitter, scratch, 56);
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, x10");                                 // only a growth allocation retires the old raw buffer
            emitter.instruction("b.eq __rt_pr_append_done");                    // an in-place append keeps its allocation
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, r10");                                // only a growth allocation retires the old raw buffer
            emitter.instruction("je __rt_pr_append_done");                      // an in-place append keeps its allocation
        }
    }
    abi::emit_call_label(emitter, "__rt_heap_free_safe");
    emitter.label("__rt_pr_append_done");
    abi::emit_frame_restore(emitter, 80);
    abi::emit_return(emitter);
    emitter.label("__rt_pr_append_overflow");
    abi::emit_jump(emitter, "__rt_heap_allocation_failed");
}

/// Reloads one append spill into the selected target C argument register.
fn load_capture_argument(emitter: &mut impl Emitter, index: usize, offset: usize) {
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter, index), offset);
}

/// Releases an abandoned raw capture and clears its process-global state.
fn emit_pr_abort<E: Emitter>(emitter: &mut E, reset_mode: fn(&mut E)) {
    emitter.blank();
    emitter.label_global("__rt_cleanup_print_r_capture");
    abi::emit_jump(emitter, "__rt_pr_abort");
    emitter.label_global("__rt_pr_abort");
    abi::emit_frame_prologue(emitter, 16);
    let result = abi::int_result_reg(emitter);
    abi::emit_load_symbol_to_reg(emitter, result, "_print_r_ptr", 0);
    for symbol in ["_print_r_ptr", "_print_r_cap", "_print_r_off"] {
        abi::emit_store_zero_to_symbol(emitter, symbol, 0);
    }
    reset_mode(emitter);
    abi::emit_call_label(emitter, "__rt_heap_free_safe");
    abi::emit_frame_restore(emitter, 16);
    abi::emit_return(emitter);
}

/// Copies raw capture bytes into an owned string with the standard string metadata hooks.
/// The raw buffer is freed and every capture global is cleared before returning the string pair.
pub fn emit_pr_finish<E: Emitter>(emitter: &mut E, reset_mode: fn(&mut E)) {
    emit_pr_abort(emitter, reset_mode);
    emitter.blank();
    emitter.label_global("__rt_pr_finish");
    abi::emit_frame_prologue(emitter, 48);
    let result = abi::int_result_reg(emitter);
    let (pointer, length) = abi::string_result_regs(emitter);
    abi::emit_load_symbol_to_reg(emitter, pointer, "_print_r_ptr", 0);
    abi::store_at_offset(emitter, pointer, 8);
    abi::emit_load_symbol_to_reg(emitter, length, "_print_r_off", 0);
    abi::emit_call_label(emitter, "__rt_str_persist");
    abi::store_at_offset(emitter, pointer, 16);
    abi::store_at_offset(emitter, length, 24);
    abi::load_at_offset(emitter, result, 8);
    abi::emit_call_label(emitter, "__rt_heap_free_safe");
    for symbol in ["_print_r_ptr", "_print_r_cap", "_print_r_off"] {
        abi::emit_store_zero_to_symbol(emitter, symbol, 0);
    }
    reset_mode(emitter);
    abi::load_at_offset(emitter, pointer, 16);
    abi::load_at_offset(emitter, length, 24);
    abi::emit_frame_restore(emitter, 48);
    abi::emit_return(emitter);
}


/// One physical field of a suspended print_r capture.
#[derive(Clone, Copy)]
pub enum CaptureField {
    /// The owned raw byte allocation.
    Buffer,
    /// The allocation capacity.
    Capacity,
    /// The accumulated byte count.
    Length,
    /// The capture mode saved alongside the storage.
    Mode,
}

impl CaptureField {
    /// Resolves the storage symbol for the selected field.
    fn symbol(self) -> &'static str {
        match self {
            Self::Buffer => "_print_r_ptr",
            Self::Capacity => "_print_r_cap",
            Self::Length => "_print_r_off",
            Self::Mode => "_print_r_mode",
        }
    }
}

/// Parks the outer capture in the caller's frame before PHP can reenter the renderer.
pub fn emit_save_state(emitter: &mut impl Emitter, fields: &[(CaptureField, usize); 4]) {
    let result = emitter.int_result_reg();
    for &(field, offset) in fields {
        let symbol = field.symbol();
        emitter.emit_load_symbol_to_reg(result, symbol, 0);
        emitter.store_at_offset(result, offset);
        emitter.emit_store_zero_to_symbol(symbol, 0);
    }
}

/// Retires any abandoned inner capture, then transfers the saved owner back to the live symbols.
pub fn emit_restore_state(
    emitter: &mut impl Emitter, fields: &[(CaptureField, usize); 4], scratch: &str,
) {
    emitter.emit_call_label("__rt_pr_abort");
    for &(field, offset) in fields {
        emitter.load_at_offset(scratch, offset);
        emitter.emit_store_reg_to_symbol(scratch, field.symbol(), 0);
    }
    let offset = fields.iter().find_map(|(field, offset)| {
        matches!(field, CaptureField::Buffer).then_some(*offset)
    }).expect("saved capture has a buffer owner");
    emitter.emit_store_zero_to_local_slot(offset);
}
