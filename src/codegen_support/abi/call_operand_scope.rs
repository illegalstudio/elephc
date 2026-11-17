//! Purpose:
//! Publishes short-lived owner slots in the exception cleanup activation chain.
//!
//! Called from:
//! - EIR call-operand lifetime lowering and recursive runtime comparisons.
//!
//! Key details:
//! - A catch in the same PHP frame still unwinds these records before entering its body.
//! - Temporary records are stack-aligned; return leases use stable frame records.
//! - Exact unlink preserves newer return records when an older temporary is retired.
//! - Records have no PHP backtrace reader and reuse the ordinary owner-slot visitor.
//! - Memoir owns publication and exact unlink; this adapter selects placement and callbacks.

use super::*;
use crate::codegen_support::emit::Emitter;

/// Shared complete record size used when reserving temporary caller storage.
pub(crate) use elephc_memoir::owner_records::RECORD_BYTES as CALL_OPERAND_OWNER_RECORD_BYTES;

/// Publishes a value in a preallocated owned-value guard before later code can throw.
pub(crate) fn emit_guard_owned_at_stack(
    emitter: &mut Emitter,
    owner: &str,
    cleanup: &str,
    record_offset: usize,
) {
    let scratch = temp_int_reg(emitter.target);
    emit_store_to_sp(emitter, owner,
        record_offset + crate::codegen_support::try_handlers::EXCEPTION_GUARD_OWNER_OFFSET);
    emit_symbol_address(emitter, scratch, cleanup);
    emit_store_to_sp(emitter, scratch, record_offset + 8);
    emit_temporary_stack_address(emitter, int_arg_reg_name(emitter.target, 0), record_offset);
    emit_load_int_immediate(emitter, int_arg_reg_name(emitter.target, 1), 0);
    emit_call_label(emitter, "__rt_exception_guard_owned");
}

/// Detaches a preallocated value guard while preserving any guards published after it.
pub(crate) fn emit_unguard_owned_at_stack(emitter: &mut Emitter, record_offset: usize) {
    emit_temporary_stack_address(emitter, int_arg_reg_name(emitter.target, 0), record_offset);
    emit_call_label(emitter, "__rt_exception_unguard_owned");
}

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
    let cleanup = if callable { "__rt_cleanup_call_operand_descriptor" } else { "__rt_cleanup_call_operand_owner" };
    emit_link_owner_with_cleanup(emitter, owner_address, cleanup, "__rt_gc_operand_frame_owners", record_offset);
}

/// Links an owner slot whose bounded cleanup drains a bridge-specific collection of values.
pub(crate) fn emit_link_call_operand_custom_cleanup_at_stack(
    emitter: &mut Emitter, owner_address: &str, cleanup: &str, visitor: &str, record_offset: usize,
) {
    emit_link_owner_with_cleanup(emitter, owner_address, cleanup, visitor, record_offset);
}

/// Links a pending reference argument whose existing owner can become the native parameter owner.
pub(crate) fn emit_push_call_reference_argument_owner(emitter: &mut Emitter, owner_address: &str) {
    emit_reserve_temporary_stack(emitter, CALL_OPERAND_OWNER_RECORD_BYTES);
    emit_link_owner_with_cleanup(emitter, owner_address, "__rt_cleanup_call_reference_argument",
        "__rt_gc_operand_frame_owners", 0);
}

/// Publishes a preallocated owner record using compiler-selected placement and callbacks.
fn emit_link_owner_with_cleanup(
    emitter: &mut Emitter, owner_address: &str, cleanup: &str, visitor: &str, record_offset: usize,
) {
    elephc_memoir::owner_records::publish_stack(emitter, owner_address, record_offset,
        elephc_memoir::owner_records::Context { head: "_exc_call_frame_top", cleanup, visitor });
}

/// Detaches the innermost temporary record before normal-path owner retirement can throw.
pub(crate) fn emit_pop_call_operand_owner(emitter: &mut Emitter) {
    emit_unlink_call_operand_owner_at_stack(emitter, 0);
    emit_release_temporary_stack(emitter, CALL_OPERAND_OWNER_RECORD_BYTES);
}

/// Unlinks the exact temporary record while retaining newer fixed-frame return guards.
pub(crate) fn emit_unlink_call_operand_owner_at_stack(emitter: &mut Emitter, record_offset: usize) {
    let record = secondary_scratch_reg(emitter);
    emitter.comment("detach temporary call operand owner");
    emit_temporary_stack_address(emitter, record, record_offset);
    emit_unlink_record(emitter);
}

/// Publishes a fixed return owner using compiler-selected placement and callbacks.
pub(crate) fn emit_link_call_operand_owner_in_frame(
    emitter: &mut Emitter,
    owner_address: &str,
    frame_offset: usize,
) {
    elephc_memoir::owner_records::link_frame(emitter, owner_address,
        elephc_memoir::FrameOffset::new(frame_offset), elephc_memoir::owner_records::Context {
            head: "_exc_call_frame_top",
            cleanup: "__rt_cleanup_call_operand_owner",
            visitor: "__rt_gc_operand_frame_owners",
        });
}

/// Removes one stable return record without changing stack argument or result registers.
pub(crate) fn emit_unlink_call_operand_owner_in_frame(emitter: &mut Emitter, frame_offset: usize) {
    let record = secondary_scratch_reg(emitter);
    emit_frame_slot_address(emitter, record, frame_offset);
    emit_unlink_record(emitter);
}

/// Detaches an externally supplied exact record using the ordinary owner-chain splice.
pub(crate) fn emit_unlink_call_operand_owner_at_address(emitter: &mut Emitter, address: &str) {
    emit_reg_move(emitter, secondary_scratch_reg(emitter), address);
    emit_unlink_record(emitter);
}

/// Splices the selected record through Memoir's exact-identity chain operation.
fn emit_unlink_record(emitter: &mut Emitter) {
    elephc_memoir::owner_records::unlink_record(emitter, "_exc_call_frame_top");
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
                (["[rsp + 112]", "[rsp + 160]"], "lea r10, [rsp + 144]", "lea r10, [rsp + 96]")
            } else {
                (["[sp, #112]", "[sp, #160]"], "add x10, x10, #144", "add x10, x10, #96")
            };
            for offset in owners { assert!(asm.contains(offset), "{target}: {asm}"); }
            assert!(asm.rfind(detach_inner).unwrap() < asm.rfind(detach_outer).unwrap(), "{target}: {asm}");
            let (reserve, release) = if target == "linux-x86_64" {
                ("sub rsp, 16", "add rsp, 16")
            } else { ("sub sp, sp, #16", "add sp, sp, #16") };
            assert_eq!(asm.matches(reserve).count(), asm.matches(release).count(), "{target}: {asm}");
            assert!(!asm.contains("#48") && !asm.contains("rsp, 48"), "{target}: no record allocation");
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

    /// Fixed return records coexist with older stack owners and preserve every ABI result pair.
    #[test]
    fn fixed_return_guards_preserve_newer_links_and_live_argument_registers() {
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(target).unwrap());
            let owner = tertiary_scratch_reg(&emitter);
            emit_push_call_operand_owner(&mut emitter, owner, false);
            emit_link_call_operand_owner_in_frame(&mut emitter, owner, 256);
            emit_pop_call_operand_owner(&mut emitter);
            let assembly = emitter.output();
            assert!(assembly.contains("publish pending reference return owner"), "{target}");
            assert!(assembly.contains("965:"), "{target}: exact-link splice must be emitted");
            let (save, restore, splice) = if target == "linux-x86_64" {
                ("mov QWORD PTR [rsp], rcx", "mov rcx, QWORD PTR [rsp]", "mov QWORD PTR [r11], rcx")
            } else {
                ("str x11, [sp]", "ldr x11, [sp]", "str x11, [x9]")
            };
            assert_eq!(assembly.matches(save).count(), 2, "{target}: {assembly}");
            assert_eq!(assembly.matches(restore).count(), 2, "{target}: {assembly}");
            assert!(assembly.contains(splice), "{target}: keep the newer guard's predecessor link");
            assert!(
                !assembly.lines().any(|line| {
                    let line = line.trim_start();
                    line.starts_with("bl ") || line.starts_with("call ")
                }),
                "{target}: no call clobbers"
            );
        }
    }
}

#[cfg(test)]
mod memoir_tests;
