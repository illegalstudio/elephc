//! Purpose:
//! Invokes one cleanup callback while retaining any thrown exception for its owning cleanup frame.
//!
//! Called from:
//! - Deep object release, which must finish releasing siblings before propagating a destructor throw.
//!
//! Key details:
//! - The C arguments are a unary native entry, its payload, and an owned pending-Throwable slot.
//! - Both internal and C unary argument registers receive the payload on x86_64.
//! - The caller's handler and active exception are restored before ordinary cleanup resumes.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use crate::codegen_support::try_handlers::{
    TRY_HANDLER_DIAG_DEPTH_OFFSET, TRY_HANDLER_JMP_BUF_OFFSET, TRY_HANDLER_SLOT_SIZE,
};

const FRAME: usize = TRY_HANDLER_SLOT_SIZE + 64;
const HANDLER: usize = FRAME - 16;
const ENTRY: usize = 8;
const PAYLOAD: usize = 16;
const OUTPUT: usize = 24;
const PREVIOUS: usize = 32;

/// Emits the non-escaping unary cleanup boundary; the caller owns the pending exception slot.
pub fn emit_cleanup_invoke(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let arg0 = abi::int_arg_reg_name(emitter.target, 0);
    let older = match emitter.target.arch { Arch::AArch64 => "x1", Arch::X86_64 => "rdi" };
    let scratch = abi::secondary_scratch_reg(emitter);
    emitter.blank();
    emitter.label_global("__rt_cleanup_invoke");
    // -- save the callback and install a handler whose lifetime ends before returning --
    abi::emit_frame_prologue(emitter, FRAME);
    for (index, offset) in [ENTRY, PAYLOAD, OUTPUT].into_iter().enumerate() {
        abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    abi::emit_load_symbol_to_reg(emitter, result, "_exc_value", 0);
    abi::store_at_offset(emitter, result, PREVIOUS);
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    for (symbol, offset) in [
        ("_exc_handler_top", HANDLER),
        ("_exc_call_frame_top", HANDLER - 8),
        ("_rt_diag_suppression", HANDLER - TRY_HANDLER_DIAG_DEPTH_OFFSET),
    ] {
        abi::emit_load_symbol_to_reg(emitter, result, symbol, 0);
        abi::store_at_offset(emitter, result, offset);
    }
    abi::emit_frame_slot_address(emitter, result, HANDLER);
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_handler_top", 0);
    abi::emit_frame_slot_address(emitter, arg0, HANDLER - TRY_HANDLER_JMP_BUF_OFFSET);
    emitter.bl_c("setjmp");                                                     // retain cleanup control when the unary callback throws
    abi::emit_branch_if_int_result_nonzero(emitter, "__rt_cleanup_invoke_caught");
    abi::load_at_offset(emitter, result, PAYLOAD);
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rdi, rax");                                    // support both internal and System V unary callback entries
    }
    abi::load_at_offset(emitter, scratch, ENTRY);
    abi::emit_call_reg(emitter, scratch);
    abi::emit_jump(emitter, "__rt_cleanup_invoke_return");

    // -- publish the newest exception before linking any older pending cleanup throw --
    emitter.label("__rt_cleanup_invoke_caught");
    abi::emit_load_symbol_to_reg(emitter, result, "_exc_value", 0);
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    abi::load_at_offset(emitter, scratch, OUTPUT);
    abi::emit_load_from_address(emitter, older, scratch, 0);
    abi::emit_store_to_address(emitter, result, scratch, 0);
    abi::emit_call_label(emitter, "__rt_exception_chain");
    emitter.label("__rt_cleanup_invoke_return");
    for (symbol, offset) in [
        ("_exc_handler_top", HANDLER),
        ("_rt_diag_suppression", HANDLER - TRY_HANDLER_DIAG_DEPTH_OFFSET),
        ("_exc_value", PREVIOUS),
    ] {
        abi::load_at_offset(emitter, result, offset);
        abi::emit_store_reg_to_symbol(emitter, result, symbol, 0);
    }
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
}

/// Passes one unary cleanup through the bounded helper, accumulating throws in a caller frame slot.
pub(crate) fn emit_guarded_cleanup_call(
    emitter: &mut Emitter,
    entry: &str,
    payload: &str,
    pending_offset: usize,
) {
    let arg0 = abi::int_arg_reg_name(emitter.target, 0);
    let arg1 = abi::int_arg_reg_name(emitter.target, 1);
    let arg2 = abi::int_arg_reg_name(emitter.target, 2);
    abi::emit_reg_move(emitter, arg1, payload);
    abi::emit_symbol_address(emitter, arg0, entry);
    abi::emit_frame_slot_address(emitter, arg2, pending_offset);
    abi::emit_call_label(emitter, "__rt_cleanup_invoke");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every supported target captures callback exceptions and restores native state before returning.
    #[test]
    fn cleanup_invocation_is_bounded_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_cleanup_invoke(&mut emitter);
            let asm = emitter.output();
            let enter = asm.find(&target.extern_symbol("setjmp")).unwrap();
            let invoke = asm.find(if target.arch == Arch::AArch64 { "blr x10" } else { "call r10" }).unwrap();
            let caught = asm.find("__rt_cleanup_invoke_caught:").unwrap();
            let chain = asm.find("__rt_exception_chain").unwrap();
            assert!(enter < invoke && invoke < caught && caught < chain, "{name}");
            assert!(!asm.contains("__rt_throw_current"), "{name}");
            assert!(asm.matches("_exc_handler_top").count() >= 3, "{name}");
        }
    }
}
