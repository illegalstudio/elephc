//! Purpose:
//! Publishes complete owner records and splices exact records from activation chains.
//!
//! Called from:
//! - Compiler ABI adapters for temporary slots, fixed return leases and external records.
//!
//! Key details:
//! - The compiler supplies placement, chain symbols and cleanup/visitor entry points.
//! - Exact unlink preserves newer records and live arguments without runtime calls.

use crate::activation_records::{CLEANUP, DATA, OPERAND_STACK_BASE, OWNER_VISITOR, PREFIX_BYTES, PREVIOUS};
use crate::{Arch, AssemblyEmitter, TargetAbi, FrameOffset};

/// Complete six-word owner activation shared by temporary, fixed-frame and external records.
pub use crate::activation_records::EXTENDED_BYTES as RECORD_BYTES;

/// Compiler-selected storage and callbacks for one complete owner activation.
#[derive(Clone, Copy)]
pub struct Context<'a> {
    /// Writable symbol containing the active chain head.
    pub head: &'a str,
    /// Cleanup entry selected for this caller's owner discipline.
    pub cleanup: &'a str,
    /// Pure GC visitor selected for this record's payload representation.
    pub visitor: &'a str,
}

/// Publishes the shared owner-slot layout with an explicit cleanup discipline.
pub fn publish_stack<E: AssemblyEmitter + TargetAbi>(
    emitter: &mut E, owner_address: &str, record_offset: usize, context: Context<'_>,
) {
    emitter.comment("publish temporary call operand owner");
    write_stack_prefix(emitter, owner_address, record_offset, context.head, context.cleanup);
    let scratch = emitter.secondary_scratch_reg();
    emitter.emit_store_to_sp(scratch, record_offset + OPERAND_STACK_BASE);
    emitter.emit_symbol_address(scratch, context.visitor);
    emitter.emit_store_to_sp(scratch, record_offset + OWNER_VISITOR);
    publish_stack_head(emitter, record_offset, context.head);
}

/// Publishes a compact native frame whose collector dispatch uses its cleanup entry.
pub fn publish_compact_stack(
    emitter: &mut impl TargetAbi,
    owner_address: &str,
    record_offset: usize,
    head: &str,
    cleanup: &str,
) {
    write_stack_prefix(emitter, owner_address, record_offset, head, cleanup);
    publish_stack_head(emitter, record_offset, head);
}

/// Initializes the shared fields before any walker can see the record.
fn write_stack_prefix(
    emitter: &mut impl TargetAbi,
    owner_address: &str,
    record_offset: usize,
    head: &str,
    cleanup: &str,
) {
    let scratch = emitter.secondary_scratch_reg();
    emitter.emit_store_to_sp(owner_address, record_offset + DATA);
    emitter.emit_load_symbol_to_reg(scratch, head, 0);
    emitter.emit_store_to_sp(scratch, record_offset + PREVIOUS);
    emitter.emit_symbol_address(scratch, cleanup);
    emitter.emit_store_to_sp(scratch, record_offset + CLEANUP);
    emitter.emit_load_int_immediate(scratch, 0);
    emitter.emit_store_to_sp(scratch, record_offset + PREFIX_BYTES);
}

/// Stores the initialized stack record address as the activation chain head.
fn publish_stack_head(emitter: &mut impl TargetAbi, record_offset: usize, head: &str) {
    let scratch = emitter.secondary_scratch_reg();
    emitter.emit_temporary_stack_address(scratch, record_offset);
    emitter.emit_store_reg_to_symbol(scratch, head, 0);
}

/// Pops a stack activation whose caller guarantees it is still the newest record.
pub fn pop_stack(emitter: &mut impl TargetAbi, record_offset: usize, head: &str) {
    let scratch = emitter.secondary_scratch_reg();
    emitter.emit_load_temporary_stack_slot(scratch, record_offset + PREVIOUS);
    emitter.emit_store_reg_to_symbol(scratch, head, 0);
}

/// Publishes a pending return lease without moving the temporary-stack base.
///
/// A return site revisited after a nonthrowing branch can still have its record attached. Remove
/// that identity before republishing it, so the activation chain never acquires a self-cycle.
pub fn link_frame<E: AssemblyEmitter + TargetAbi>(
    emitter: &mut E,
    owner_address: &str,
    frame_offset: FrameOffset,
    context: Context<'_>,
) {
    let record = emitter.secondary_scratch_reg();
    emitter.emit_frame_slot_address(record, frame_offset.bytes());
    unlink_record(emitter, context.head);
    let record = emitter.secondary_scratch_reg();
    let scratch = emitter.symbol_scratch_reg();
    emitter.emit_frame_slot_address(record, frame_offset.bytes());
    emitter.comment("publish pending reference return owner");
    emitter.emit_store_to_address(owner_address, record, DATA);
    emitter.emit_load_symbol_to_reg(scratch, context.head, 0);
    emitter.emit_store_to_address(scratch, record, PREVIOUS);
    emitter.emit_symbol_address(scratch, context.cleanup);
    emitter.emit_store_to_address(scratch, record, CLEANUP);
    emitter.emit_store_zero_to_address(record, PREFIX_BYTES);
    emitter.emit_store_zero_to_address(record, OPERAND_STACK_BASE);
    emitter.emit_symbol_address(scratch, context.visitor);
    emitter.emit_store_to_address(scratch, record, OWNER_VISITOR);
    emitter.emit_store_reg_to_symbol(record, context.head, 0);
}

/// Splices the record in the secondary scratch register out of the activation chain.
///
/// The common LIFO path uses only the two reserved scratch registers. The uncommon search saves
/// the tertiary scratch register, which is also a live SysV argument register, before borrowing
/// it. Numeric labels keep each inline sequence local on ELF and Mach-O without runtime calls.
pub fn unlink_record<E: AssemblyEmitter + TargetAbi>(emitter: &mut E, head_symbol: &str) {
    let record = emitter.secondary_scratch_reg();
    let link = emitter.symbol_scratch_reg();
    let current = emitter.tertiary_scratch_reg();
    emitter.emit_load_symbol_to_reg(link, head_symbol, 0);
    if emitter.arch() == Arch::AArch64 {
        emitter.instruction(&format!("cmp {link}, {record}"));                  // keep the ordinary top-record removal on its constant-time path
        emitter.instruction("b.ne 961f");                                       // search only when a newer stable record lies above this owner
    } else {
        emitter.instruction(&format!("cmp {link}, {record}"));                  // compare the head with the exact record being retired
        emitter.instruction("jne 961f");                                        // preserve newer pending-return records through the slow path
    }
    emitter.emit_load_from_address(record, record, PREVIOUS);
    emitter.emit_store_reg_to_symbol(record, head_symbol, 0);
    emitter.instruction(if emitter.arch() == Arch::AArch64 {
        "b 964f"
    } else { "jmp 964f" });                                                     // finish the ordinary top-record removal without borrowing argument registers
    emitter.label("961");
    emitter.emit_reserve_temporary_stack(16);
    emitter.emit_store_to_sp(current, 0);
    emitter.emit_symbol_address(link, head_symbol);
    emitter.label("962");
    emitter.emit_load_from_address(current, link, PREVIOUS);
    if emitter.arch() == Arch::AArch64 {
        emitter.instruction(&format!("cbz {current}, 963f"));                   // tolerate records already removed by exceptional unwinding
        emitter.instruction(&format!("cmp {current}, {record}"));               // identify the exact older cleanup record
        emitter.instruction("b.eq 965f");                                       // splice it out without touching newer owners
    } else {
        emitter.instruction(&format!("test {current}, {current}"));             // stop at the end of the live chain
        emitter.instruction("jz 963f");                                         // permit an already-detached fixed record
        emitter.instruction(&format!("cmp {current}, {record}"));               // compare this link with the selected record identity
        emitter.instruction("je 965f");                                         // preserve unrelated newer cleanup records
    }
    emitter.emit_reg_move(link, current);
    emitter.instruction(if emitter.arch() == Arch::AArch64 {
        "b 962b"
    } else { "jmp 962b" });                                                     // continue through each activation's previous-record link
    emitter.label("965");
    emitter.emit_load_from_address(current, record, PREVIOUS);
    emitter.emit_store_to_address(current, link, PREVIOUS);
    emitter.label("963");
    emitter.emit_load_temporary_stack_slot(current, 0);
    emitter.emit_release_temporary_stack(16);
    emitter.label("964");
}

/// Host registers and callbacks for a complete externally supplied owner activation.
#[derive(Clone, Copy)]
pub struct ExternalRecord<'a> {
    /// Register containing the caller-owned address of the complete record.
    pub record_address: &'a str,
    /// Register containing the pure visitor callback address.
    pub visitor: &'a str,
    /// Register containing the borrowed opaque visitor payload.
    pub data: &'a str,
    /// Writable symbol containing the active activation-chain head.
    pub head_symbol: &'a str,
    /// Cleanup entry selected by the external owner, including a no-op discipline.
    pub cleanup_symbol: &'a str,
}

/// Initializes every external record field before publishing its address in the host chain.
///
/// Input registers must not alias the host ABI's reserved scratch registers.
pub fn publish_external<E: AssemblyEmitter + TargetAbi>(emitter: &mut E, context: ExternalRecord<'_>) {
    let scratch = emitter.secondary_scratch_reg();
    emitter.emit_load_symbol_to_reg(scratch, context.head_symbol, 0);
    emitter.emit_store_to_address(scratch, context.record_address, PREVIOUS);
    emitter.emit_store_to_address(context.data, context.record_address, DATA);
    emitter.emit_store_to_address(context.visitor, context.record_address, OWNER_VISITOR);
    emitter.emit_store_zero_to_address(context.record_address, PREFIX_BYTES);
    emitter.emit_store_zero_to_address(context.record_address, OPERAND_STACK_BASE);
    emitter.emit_symbol_address(scratch, context.cleanup_symbol);
    emitter.emit_store_to_address(scratch, context.record_address, CLEANUP);
    emitter.emit_store_reg_to_symbol(context.record_address, context.head_symbol, 0);
}
