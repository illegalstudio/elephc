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

pub(crate) const CALL_OPERAND_OWNER_RECORD_BYTES: usize = 48;

/// Links a caller-owned slot into the cleanup chain without consuming its current payload.
pub(crate) fn emit_push_call_operand_owner(emitter: &mut Emitter, owner_address: &str, callable: bool) {
    emit_reserve_temporary_stack(emitter, CALL_OPERAND_OWNER_RECORD_BYTES);
    emit_link_call_operand_owner_at_stack(emitter, owner_address, callable, 0);
}

/// Links an owner record in preallocated temporary storage without moving the caller's stack base.
pub(crate) fn emit_link_call_operand_owner_at_stack(
    emitter: &mut Emitter,
    owner_address: &str,
    callable: bool,
    record_offset: usize,
) {
    let scratch = secondary_scratch_reg(emitter);
    emitter.comment("publish temporary call operand owner");
    emit_store_to_sp(emitter, owner_address, record_offset + 16);
    emit_load_symbol_to_reg(emitter, scratch, "_exc_call_frame_top", 0);
    emit_store_to_sp(emitter, scratch, record_offset);
    let cleanup = if callable { "__rt_cleanup_call_operand_descriptor" } else { "__rt_cleanup_call_operand_owner" };
    emit_symbol_address(emitter, scratch, cleanup);
    emit_store_to_sp(emitter, scratch, record_offset + 8);
    emit_load_int_immediate(emitter, scratch, 0);
    emit_store_to_sp(emitter, scratch, record_offset + 24);
    emit_store_to_sp(emitter, scratch, record_offset + 32);
    emit_store_to_sp(emitter, scratch, record_offset + 40);
    emit_temporary_stack_address(emitter, scratch, record_offset);
    emit_store_reg_to_symbol(emitter, scratch, "_exc_call_frame_top", 0);
}

/// Detaches the innermost temporary record before normal-path owner retirement can throw.
pub(crate) fn emit_pop_call_operand_owner(emitter: &mut Emitter) {
    emit_unlink_call_operand_owner_at_stack(emitter, 0);
    emit_release_temporary_stack(emitter, CALL_OPERAND_OWNER_RECORD_BYTES);
}

/// Unlinks an innermost record from preallocated storage without changing the temporary stack base.
pub(crate) fn emit_unlink_call_operand_owner_at_stack(emitter: &mut Emitter, record_offset: usize) {
    let scratch = secondary_scratch_reg(emitter);
    emitter.comment("detach temporary call operand owner");
    emit_load_temporary_stack_slot(emitter, scratch, record_offset);
    emit_store_reg_to_symbol(emitter, scratch, "_exc_call_frame_top", 0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Preallocated records preserve the stack base and detach in reverse publication order.
    #[test]
    fn preallocated_call_operand_records_keep_offsets_on_all_targets() {
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(target).unwrap());
            let owner = tertiary_scratch_reg(&emitter);
            emit_link_call_operand_owner_at_stack(&mut emitter, owner, false, 96);
            emit_link_call_operand_owner_at_stack(&mut emitter, owner, false, 144);
            emit_unlink_call_operand_owner_at_stack(&mut emitter, 144);
            emit_unlink_call_operand_owner_at_stack(&mut emitter, 96);
            let asm = emitter.output();
            let (owners, detach_inner, detach_outer) = if target == "linux-x86_64" {
                (["[rsp + 112]", "[rsp + 160]"], "mov r10, QWORD PTR [rsp + 144]", "mov r10, QWORD PTR [rsp + 96]")
            } else {
                (["[sp, #112]", "[sp, #160]"], "ldr x10, [sp, #144]", "ldr x10, [sp, #96]")
            };
            for offset in owners { assert!(asm.contains(offset), "{target}: {asm}"); }
            assert!(asm.find(detach_inner).unwrap() < asm.find(detach_outer).unwrap(), "{target}: {asm}");
            for movement in ["sub sp,", "add sp,", "sub rsp,", "add rsp,"] {
                assert!(!asm.contains(movement), "{target}: {asm}");
            }
        }
    }

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
