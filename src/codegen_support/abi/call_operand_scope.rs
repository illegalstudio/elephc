//! Purpose:
//! Publishes short-lived owner slots in the exception cleanup activation chain.
//!
//! Called from:
//! - EIR call-operand lifetime lowering.
//!
//! Key details:
//! - A catch in the same PHP frame still unwinds these records before entering its body.
//! - Records are stack-aligned, nest in LIFO order, and have no PHP backtrace reader.

use super::*;
use crate::codegen_support::emit::Emitter;

const RECORD_BYTES: usize = 48;

/// Links a caller-owned slot into the cleanup chain without consuming its current payload.
pub(crate) fn emit_push_call_operand_owner(emitter: &mut Emitter, owner_address: &str, callable: bool) {
    let scratch = secondary_scratch_reg(emitter);
    emitter.comment("publish temporary call operand owner");
    emit_reserve_temporary_stack(emitter, RECORD_BYTES);
    emit_store_to_sp(emitter, owner_address, 16);
    emit_load_symbol_to_reg(emitter, scratch, "_exc_call_frame_top", 0);
    emit_store_to_sp(emitter, scratch, 0);
    let cleanup = if callable { "__rt_cleanup_call_operand_descriptor" } else { "__rt_cleanup_call_operand_owner" };
    emit_symbol_address(emitter, scratch, cleanup);
    emit_store_to_sp(emitter, scratch, 8);
    emit_load_int_immediate(emitter, scratch, 0);
    emit_store_to_sp(emitter, scratch, 24);
    emit_store_to_sp(emitter, scratch, 32);
    emit_store_to_sp(emitter, scratch, 40);
    emit_temporary_stack_address(emitter, scratch, 0);
    emit_store_reg_to_symbol(emitter, scratch, "_exc_call_frame_top", 0);
}

/// Detaches the innermost temporary record before normal-path owner retirement can throw.
pub(crate) fn emit_pop_call_operand_owner(emitter: &mut Emitter) {
    let scratch = secondary_scratch_reg(emitter);
    emitter.comment("detach temporary call operand owner");
    emit_load_temporary_stack_slot(emitter, scratch, 0);
    emit_store_reg_to_symbol(emitter, scratch, "_exc_call_frame_top", 0);
    emit_release_temporary_stack(emitter, RECORD_BYTES);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// All ABIs publish complete invisible cleanup records and detach them without touching results.
    #[test]
    fn call_operand_scope_records_are_complete_and_balanced_on_all_targets() {
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(target).unwrap());
            let owner = tertiary_scratch_reg(&emitter);
            emit_push_call_operand_owner(&mut emitter, owner, false);
            emit_push_call_operand_owner(&mut emitter, owner, true);
            emit_pop_call_operand_owner(&mut emitter);
            emit_pop_call_operand_owner(&mut emitter);
            let offsets = if emitter.target.arch == crate::codegen_support::platform::Arch::AArch64 {
                ["[sp, #24]", "[sp, #32]", "[sp, #40]"]
            } else {
                ["[rsp + 24]", "[rsp + 32]", "[rsp + 40]"]
            };
            let asm = emitter.output();
            assert!(asm.contains("__rt_cleanup_call_operand_owner"), "{target}");
            assert!(asm.contains("__rt_cleanup_call_operand_descriptor"), "{target}");
            assert!(asm.matches("_exc_call_frame_top").count() >= 6, "{target}");
            for offset in offsets { assert!(asm.contains(offset), "{target}: {offset}"); }
        }
    }
}
