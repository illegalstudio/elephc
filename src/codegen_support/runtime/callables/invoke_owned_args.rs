//! Purpose:
//! Invokes a callable descriptor while owning its temporary boxed argument array.
//!
//! Called from:
//! - Descriptor-backed array callback wrappers and extern callback trampolines.
//!
//! Key details:
//! - The argument box is consumed on normal and exceptional exits.
//! - Separate entries distinguish borrowed descriptors from caller-transferred descriptor owners.
//! - A local native handler keeps cleanup below the caller's exception boundary.
//! - Slots are cleared before release so a cleanup throw cannot release an owner twice.

use crate::codegen_support::{abi, callable_descriptor, emit::Emitter, platform::Arch};
use crate::codegen_support::try_handlers::{
    TRY_HANDLER_DIAG_DEPTH_OFFSET, TRY_HANDLER_JMP_BUF_OFFSET, TRY_HANDLER_SLOT_SIZE,
};

const FRAME_SIZE: usize = TRY_HANDLER_SLOT_SIZE + 64;
const HANDLER_OFFSET: usize = FRAME_SIZE - 16;
const DESCRIPTOR: usize = 8;
const ARGUMENTS: usize = 16;
const RESULT: usize = 24;
const PENDING: usize = 32;
const PREVIOUS: usize = 40;
const OWNED_DESCRIPTOR: usize = 48;

/// Emits a consuming argument-container boundary using the descriptor's two-argument native ABI.
/// Returns an owned Mixed cell, or rethrows only after releasing the arguments and interrupted result.
pub(crate) fn emit_callable_invoke_owned_args(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let descriptor_arg = abi::int_arg_reg_name(emitter.target, 0);
    let array_arg = abi::int_arg_reg_name(emitter.target, 1);
    let invoker = abi::secondary_scratch_reg(emitter);
    let caught = "__rt_callable_owned_args_caught";
    let cleanup = "__rt_callable_owned_args_cleanup";
    let returned = "__rt_callable_owned_args_return";

    emitter.blank();
    emit_owned_args_entry(emitter, "__rt_callable_invoke_owned_args", false);
    abi::emit_jump(emitter, "__rt_callable_owned_args_enter");
    emit_owned_args_entry(emitter, "__rt_callable_invoke_owned_descriptor_args", true);
    emitter.label("__rt_callable_owned_args_enter");
    // -- preserve inputs and install a boundary before entering the descriptor invoker --
    clear_slot(emitter, RESULT);
    clear_slot(emitter, PENDING);
    abi::emit_load_symbol_to_reg(emitter, result, "_exc_value", 0);
    abi::store_at_offset(emitter, result, PREVIOUS);
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    save_handler_state(emitter);
    abi::emit_frame_slot_address(emitter, descriptor_arg, HANDLER_OFFSET - TRY_HANDLER_JMP_BUF_OFFSET);
    emitter.bl_c("setjmp");                                                     // catch descriptor or cleanup throws without discarding the owned slots
    abi::emit_branch_if_int_result_nonzero(emitter, caught);
    abi::load_at_offset(emitter, descriptor_arg, DESCRIPTOR);
    abi::load_at_offset(emitter, array_arg, ARGUMENTS);
    callable_descriptor::emit_load_invoker_from_descriptor(emitter, invoker, descriptor_arg);
    abi::emit_call_reg(emitter, invoker);
    abi::store_at_offset(emitter, result, RESULT);

    // -- release argument ownership before transferring a successful result --
    emitter.label(cleanup);
    release_mixed_slot(emitter, ARGUMENTS);
    abi::load_at_offset(emitter, result, OWNED_DESCRIPTOR);
    clear_slot(emitter, OWNED_DESCRIPTOR);
    abi::emit_call_label(emitter, "__rt_callable_descriptor_release");
    abi::load_at_offset(emitter, result, PENDING);
    abi::emit_branch_if_int_result_zero(emitter, returned);
    release_mixed_slot(emitter, RESULT);
    chain_pending_with_slot(emitter, PREVIOUS);
    abi::load_at_offset(emitter, result, PENDING);
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_value", 0);
    clear_slot(emitter, PENDING);
    restore_handler_state(emitter);
    abi::emit_frame_restore(emitter, FRAME_SIZE);
    abi::emit_jump(emitter, "__rt_throw_current");

    emitter.label(returned);
    abi::load_at_offset(emitter, result, PREVIOUS);
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_value", 0);
    restore_handler_state(emitter);
    abi::load_at_offset(emitter, result, RESULT);
    abi::emit_frame_restore(emitter, FRAME_SIZE);
    abi::emit_return(emitter);

    // -- retain the newest throw and continue cleaning any still-owned slots --
    emitter.label(caught);
    let older = previous_exception_reg(emitter);
    abi::emit_load_symbol_to_reg(emitter, result, "_exc_value", 0);
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    abi::load_at_offset(emitter, older, PENDING);
    abi::store_at_offset(emitter, result, PENDING);
    abi::emit_call_label(emitter, "__rt_exception_chain");
    abi::emit_jump(emitter, cleanup);
}

/// Saves both ABI inputs and records whether this entry consumes the descriptor owner.
fn emit_owned_args_entry(emitter: &mut Emitter, label: &str, owns_descriptor: bool) {
    emitter.label_global(label);
    abi::emit_frame_prologue(emitter, FRAME_SIZE);
    let descriptor = abi::int_arg_reg_name(emitter.target, 0);
    abi::store_at_offset(emitter, descriptor, DESCRIPTOR);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 1), ARGUMENTS);
    if owns_descriptor {
        abi::store_at_offset(emitter, descriptor, OWNED_DESCRIPTOR);
    } else {
        clear_slot(emitter, OWNED_DESCRIPTOR);
    }
}

/// Saves the enclosing handler, activation boundary, and diagnostics before publishing this record.
fn save_handler_state(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    for (symbol, offset) in [
        ("_exc_handler_top", HANDLER_OFFSET),
        ("_exc_call_frame_top", HANDLER_OFFSET - 8),
        ("_rt_diag_suppression", HANDLER_OFFSET - TRY_HANDLER_DIAG_DEPTH_OFFSET),
    ] {
        abi::emit_load_symbol_to_reg(emitter, result, symbol, 0);
        abi::store_at_offset(emitter, result, offset);
    }
    abi::emit_frame_slot_address(emitter, result, HANDLER_OFFSET);
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_handler_top", 0);
}

/// Restores the caller's exception-handler chain and diagnostic suppression depth.
fn restore_handler_state(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    abi::load_at_offset(emitter, result, HANDLER_OFFSET);
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_handler_top", 0);
    abi::load_at_offset(emitter, result, HANDLER_OFFSET - TRY_HANDLER_DIAG_DEPTH_OFFSET);
    abi::emit_store_reg_to_symbol(emitter, result, "_rt_diag_suppression", 0);
}

/// Releases a boxed owner after clearing its slot, leaving reentrant cleanup idempotent.
fn release_mixed_slot(emitter: &mut Emitter, offset: usize) {
    abi::load_at_offset(emitter, abi::int_result_reg(emitter), offset);
    clear_slot(emitter, offset);
    abi::emit_call_label(emitter, "__rt_decref_mixed");
}

/// Clears one cleanup slot without disturbing a loaded owner or either exception-chain argument.
fn clear_slot(emitter: &mut Emitter, offset: usize) {
    let scratch = abi::secondary_scratch_reg(emitter);
    abi::emit_load_int_immediate(emitter, scratch, 0);
    abi::store_at_offset(emitter, scratch, offset);
}

/// Consumes a parked exception owner into the pending chain before propagating the newest throw.
fn chain_pending_with_slot(emitter: &mut Emitter, offset: usize) {
    abi::load_at_offset(emitter, previous_exception_reg(emitter), offset);
    clear_slot(emitter, offset);
    abi::load_at_offset(emitter, abi::int_result_reg(emitter), PENDING);
    abi::emit_call_label(emitter, "__rt_exception_chain");
}

/// Returns the old-exception input of the raw-object exception-chain runtime ABI.
fn previous_exception_reg(emitter: &Emitter) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => "x1",
        Arch::X86_64 => "rdi",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// All five targets protect the callback and clear both owners before their release sites.
    #[test]
    fn owned_callback_args_are_cleaned_before_return_and_rethrow_on_all_targets() {
        assert_eq!(FRAME_SIZE % 16, 0);
        assert!(HANDLER_OFFSET - TRY_HANDLER_SLOT_SIZE >= OWNED_DESCRIPTOR);
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_callable_invoke_owned_args(&mut emitter);
            let asm = emitter.output();
            let setjmp = asm.find("setjmp").unwrap();
            let invoke = asm.find(if target.arch == Arch::AArch64 { "blr " } else { "call r" }).unwrap();
            let cleanup = asm.find("__rt_callable_owned_args_cleanup:").unwrap();
            let released = asm.find("__rt_decref_mixed").unwrap();
            let rethrow = asm.find("__rt_throw_current").unwrap();
            assert!(setjmp < invoke && invoke < cleanup && cleanup < released && released < rethrow, "{name}: {asm}");
            assert_eq!(asm.matches("__rt_decref_mixed").count(), 2, "{name}");
            assert_eq!(asm.matches("__rt_exception_chain").count(), 2, "{name}");
            assert!(asm.contains("__rt_callable_invoke_owned_descriptor_args:"), "{name}");
            assert_eq!(asm.matches("__rt_callable_descriptor_release").count(), 1, "{name}");
            for offset in [ARGUMENTS, RESULT, OWNED_DESCRIPTOR] {
                let clear = if target.arch == Arch::AArch64 {
                    format!("stur x10, [x29, #-{offset}]")
                } else {
                    format!("mov QWORD PTR [rbp - {offset}], r10")
                };
                assert!(asm[cleanup..rethrow].contains(&clear), "{name}: {asm}");
            }
            assert!(asm[rethrow..].contains("__rt_callable_owned_args_return:"), "{name}");
            assert!(asm.contains("_exc_call_frame_top"), "{name}");
            assert!(asm.contains("_rt_diag_suppression"), "{name}");
        }
    }
}
