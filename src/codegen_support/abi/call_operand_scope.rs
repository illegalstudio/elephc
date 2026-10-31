//! Purpose:
//! Publishes short-lived owner slots in the exception cleanup activation chain.
//!
//! Called from:
//! - EIR call-operand lifetime lowering.
//!
//! Key details:
//! - A catch in the same PHP frame still unwinds these records before entering its body.
//! - Temporary records are stack-aligned; return leases use stable frame records.
//! - Exact unlink preserves newer return records when an older temporary is retired.
//! - Records have no PHP backtrace reader and reuse the ordinary owner-slot visitor.

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
    emit_symbol_address(emitter, scratch, "__rt_gc_operand_frame_owners");
    emit_store_to_sp(emitter, scratch, record_offset + 40);
    emit_temporary_stack_address(emitter, scratch, record_offset);
    emit_store_reg_to_symbol(emitter, scratch, "_exc_call_frame_top", 0);
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

/// Publishes a pending return lease without moving the temporary-stack base.
///
/// A return site revisited after a nonthrowing branch can still have its record attached. Remove
/// that identity before republishing it, so the activation chain never acquires a self-cycle.
pub(crate) fn emit_link_call_operand_owner_in_frame(
    emitter: &mut Emitter,
    owner_address: &str,
    frame_offset: usize,
) {
    emit_unlink_call_operand_owner_in_frame(emitter, frame_offset);
    let record = secondary_scratch_reg(emitter);
    let scratch = symbol_scratch_reg(emitter);
    emit_frame_slot_address(emitter, record, frame_offset);
    emitter.comment("publish pending reference return owner");
    emit_store_to_address(emitter, owner_address, record, 16);
    emit_load_symbol_to_reg(emitter, scratch, "_exc_call_frame_top", 0);
    emit_store_to_address(emitter, scratch, record, 0);
    emit_symbol_address(emitter, scratch, "__rt_cleanup_call_operand_owner");
    emit_store_to_address(emitter, scratch, record, 8);
    emit_store_zero_to_address(emitter, record, 24);
    emit_store_zero_to_address(emitter, record, 32);
    emit_symbol_address(emitter, scratch, "__rt_gc_operand_frame_owners");
    emit_store_to_address(emitter, scratch, record, 40);
    emit_store_reg_to_symbol(emitter, record, "_exc_call_frame_top", 0);
}

/// Removes one stable return record without changing stack argument or result registers.
pub(crate) fn emit_unlink_call_operand_owner_in_frame(emitter: &mut Emitter, frame_offset: usize) {
    let record = secondary_scratch_reg(emitter);
    emit_frame_slot_address(emitter, record, frame_offset);
    emit_unlink_record(emitter);
}

/// Splices the record in the secondary scratch register out of the activation chain.
///
/// The common LIFO path uses only the two reserved scratch registers. The uncommon search saves
/// the tertiary scratch register, which is also a live SysV argument register, before borrowing
/// it. Numeric labels keep each inline sequence local on ELF and Mach-O without runtime calls.
fn emit_unlink_record(emitter: &mut Emitter) {
    let record = secondary_scratch_reg(emitter);
    let link = symbol_scratch_reg(emitter);
    let current = tertiary_scratch_reg(emitter);
    emit_load_symbol_to_reg(emitter, link, "_exc_call_frame_top", 0);
    if emitter.target.arch == crate::codegen_support::platform::Arch::AArch64 {
        emitter.instruction(&format!("cmp {link}, {record}"));                  // keep the ordinary top-record removal on its constant-time path
        emitter.instruction("b.ne 961f");                                       // search only when a newer stable record lies above this owner
    } else {
        emitter.instruction(&format!("cmp {link}, {record}"));                  // compare the head with the exact record being retired
        emitter.instruction("jne 961f");                                        // preserve newer pending-return records through the slow path
    }
    emit_load_from_address(emitter, record, record, 0);
    emit_store_reg_to_symbol(emitter, record, "_exc_call_frame_top", 0);
    emitter.instruction(if emitter.target.arch == crate::codegen_support::platform::Arch::AArch64 {
        "b 964f"
    } else { "jmp 964f" });                                                     // finish the ordinary top-record removal without borrowing argument registers
    emitter.label("961");
    emit_reserve_temporary_stack(emitter, 16);
    emit_store_to_sp(emitter, current, 0);
    emit_symbol_address(emitter, link, "_exc_call_frame_top");
    emitter.label("962");
    emit_load_from_address(emitter, current, link, 0);
    if emitter.target.arch == crate::codegen_support::platform::Arch::AArch64 {
        emitter.instruction(&format!("cbz {current}, 963f"));                   // tolerate records already removed by exceptional unwinding
        emitter.instruction(&format!("cmp {current}, {record}"));               // identify the exact older cleanup record
        emitter.instruction("b.eq 965f");                                       // splice it out without touching newer owners
    } else {
        emitter.instruction(&format!("test {current}, {current}"));             // stop at the end of the live chain
        emitter.instruction("jz 963f");                                         // permit an already-detached fixed record
        emitter.instruction(&format!("cmp {current}, {record}"));               // compare this link with the selected record identity
        emitter.instruction("je 965f");                                         // preserve unrelated newer cleanup records
    }
    emit_reg_move(emitter, link, current);
    emitter.instruction(if emitter.target.arch == crate::codegen_support::platform::Arch::AArch64 {
        "b 962b"
    } else { "jmp 962b" });                                                     // continue through each activation's previous-record link
    emitter.label("965");
    emit_load_from_address(emitter, current, record, 0);
    emit_store_to_address(emitter, current, link, 0);
    emitter.label("963");
    emit_load_temporary_stack_slot(emitter, current, 0);
    emit_release_temporary_stack(emitter, 16);
    emitter.label("964");
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
            assert!(asm.find(detach_inner).unwrap() < asm.find(detach_outer).unwrap(), "{target}: {asm}");
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
            assert!(!assembly.contains("bl ") && !assembly.contains("call "), "{target}: no call clobbers");
        }
    }
}
