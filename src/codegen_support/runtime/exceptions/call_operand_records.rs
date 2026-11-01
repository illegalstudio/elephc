//! Purpose:
//! Emits the out-of-line halves of call-operand owner records:
//! `__rt_call_operand_link_owner`, `__rt_call_operand_link_descriptor` and
//! `__rt_call_operand_unlink`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters` (always emitted).
//! - `crate::codegen::lower_inst::ownership` for the standalone `push_call_operand_owner` /
//!   `pop_call_operand_owner` EIR ops, through `abi::emit_push_call_operand_owner_call` and
//!   `abi::emit_pop_call_operand_owner_call`.
//!
//! Key details:
//! - Ordinary call ABI: the record address (AArch64 `x0`, x86_64 `rdi`) and, for the link
//!   helpers, the owner slot address (`x1`, `rsi`). Only caller-saved registers are touched.
//! - The record layout is the one `abi::emit_link_call_operand_owner_at_stack` writes inline:
//!   `[0]` previous activation head, `[8]` cleanup callback, `[16]` owner address, `[24..48]`
//!   zero. Mid-sequence callers whose argument registers are live keep that inline form.
//! - Inlined, the link was about sixteen instructions at every user call site; called, it is three.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits both link helpers and the unlink helper for the current target.
pub fn emit_call_operand_records(emitter: &mut Emitter) {
    emit_link(emitter, "__rt_call_operand_link_owner", "__rt_cleanup_call_operand_owner");
    emit_link(emitter, "__rt_call_operand_link_descriptor", "__rt_cleanup_call_operand_descriptor");
    emit_unlink(emitter);
    emit_stack_record_entries(emitter);
}

/// AArch64 only: the reserve-link and unlink-release forms of the call-operand scope.
///
/// A call-operand scope used to cost five instructions to open (`sub sp`, owner address, record
/// address, `bl`) and three to close. These entries adjust the CALLER's `sp` themselves -- legal
/// because the return address travels in x30, not on the stack -- so a scope opens with the owner
/// address in `x1` plus one `bl`, and closes with one `bl`. x86_64 keeps the explicit forms: its
/// return address is on the stack, and a `jmp`-return would defeat the return predictor.
fn emit_stack_record_entries(emitter: &mut Emitter) {
    if emitter.target.arch != Arch::AArch64 {
        return;
    }
    for (name, link) in [
        ("__rt_call_operand_push_owner", "__rt_call_operand_link_owner"),
        ("__rt_call_operand_push_descriptor", "__rt_call_operand_link_descriptor"),
    ] {
        emitter.blank();
        emitter.comment(&format!("--- runtime: {} ---", name));
        emitter.label_global(name);
        emitter.instruction("sub sp, sp, #48");                                 // reserve the record in the caller's frame
        emitter.instruction("mov x0, sp");                                      // the record is the new stack top
        emitter.instruction(&format!("b {}", link));                            // link it; the helper returns straight to the caller
    }
    emitter.blank();
    emitter.comment("--- runtime: __rt_call_operand_pop ---");
    emitter.label_global("__rt_call_operand_pop");
    emitter.instruction("ldr x3, [sp]");                                        // the record's saved head
    abi::emit_store_reg_to_symbol(emitter, "x3", "_exc_call_frame_top", 0);
    emitter.instruction("add sp, sp, #48");                                     // release the record from the caller's frame
    emitter.instruction("ret");                                                 // the scope is closed
}

/// Emits one link helper whose record names `cleanup` as its unwind callback.
fn emit_link(emitter: &mut Emitter, name: &str, cleanup: &str) {
    emitter.blank();
    emitter.comment(&format!("--- runtime: {} ---", name));
    emitter.label_global(name);
    let (record, owner, scratch) = match emitter.target.arch {
        Arch::AArch64 => ("x0", "x1", "x3"),
        Arch::X86_64 => ("rdi", "rsi", "rax"),
    };
    abi::emit_store_to_address(emitter, owner, record, 16);
    abi::emit_load_symbol_to_reg(emitter, scratch, "_exc_call_frame_top", 0);
    abi::emit_store_to_address(emitter, scratch, record, 0);
    abi::emit_symbol_address(emitter, scratch, cleanup);
    abi::emit_store_to_address(emitter, scratch, record, 8);
    abi::emit_load_int_immediate(emitter, scratch, 0);
    abi::emit_store_to_address(emitter, scratch, record, 24);
    abi::emit_store_to_address(emitter, scratch, record, 32);
    abi::emit_store_to_address(emitter, scratch, record, 40);
    abi::emit_store_reg_to_symbol(emitter, record, "_exc_call_frame_top", 0);
    emitter.instruction("ret");                                                 // the record is now the innermost cleanup activation
}

/// Emits the unlink helper: the record's saved head becomes the current head again.
fn emit_unlink(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: __rt_call_operand_unlink ---");
    emitter.label_global("__rt_call_operand_unlink");
    let (record, scratch) = match emitter.target.arch {
        Arch::AArch64 => ("x0", "x3"),
        Arch::X86_64 => ("rdi", "rax"),
    };
    abi::emit_load_from_address(emitter, scratch, record, 0);
    abi::emit_store_reg_to_symbol(emitter, scratch, "_exc_call_frame_top", 0);
    emitter.instruction("ret");                                                 // the record is detached; its storage is the caller's to release
}
