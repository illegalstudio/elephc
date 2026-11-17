//! Purpose:
//! Preserves borrowed string bytes across callbacks that may replace their source.
//!
//! Called from:
//! - Compiler emitters surrounding warnings in string writes.
//!
//! Key details:
//! - Owned copies have an unwind guard and concat temporaries keep their existing storage.

use crate::{abi, Arch, Emitter, FrameOffset, RecordOffset};
use crate::layout::CONCAT_TEMP_HEAP_KIND;

/// Native locations of the borrowed string pair and the private owner.
#[derive(Clone, Copy)]
pub struct SnapshotLayout {
    /// Stack-relative start of the AArch64 pointer/length pair.
    pub arm_pair: RecordOffset,
    /// Stack-relative slot holding the AArch64 copy.
    pub arm_owner: RecordOffset,
    /// Frame-relative source pointer on x86_64.
    pub x86_pointer: FrameOffset,
    /// Frame-relative source length on x86_64.
    pub x86_length: FrameOffset,
    /// Frame-relative slot holding the x86_64 copy.
    pub x86_owner: FrameOffset,
}

/// Storage and callback contract of the temporary guard.
#[derive(Clone, Copy)]
pub struct CopyGuard<'a> {
    /// Aligned size of the guard record.
    pub bytes: usize,
    /// Offset of the copy owner inside the guard.
    pub owner: RecordOffset,
    /// Native entry cleaning up the copy during unwinding.
    pub cleanup: &'a str,
}

/// Copies the subject and parks its owner unless persistence would adopt a concat temporary.
pub fn emit_pin(emitter: &mut impl Emitter, layout: SnapshotLayout, skip: &str) {
    match emitter.arch() {
        Arch::AArch64 => emit_pin_subject_aarch64(emitter, layout, skip),
        Arch::X86_64 => emit_pin_subject_x86_64(emitter, layout, skip),
    }
}

/// Publishes an unwind guard over the parked copy before a callback can throw.
pub fn emit_guard_push(emitter: &mut impl Emitter, layout: SnapshotLayout, guard: CopyGuard<'_>) {
    match emitter.arch() {
        Arch::AArch64 => emit_copy_guard_push_aarch64(emitter, layout, guard),
        Arch::X86_64 => emit_copy_guard_push_x86_64(emitter, layout, guard),
    }
}

/// Detaches the guard after a normal callback return, leaving the frame's owner available.
pub fn emit_guard_pop(emitter: &mut impl Emitter, guard: CopyGuard<'_>) {
    crate::guarded_owners::emit_detach_at_stack(emitter, RecordOffset::new(0));
    emitter.emit_release_temporary_stack(guard.bytes);
}

/// Copies the borrowed AArch64 subject into stable storage before a callback.
fn emit_pin_subject_aarch64(emitter: &mut impl Emitter, layout: SnapshotLayout, skip: &str) {
    emitter.instruction(&format!("ldr x0, [sp, #{}]", layout.arm_pair.bytes())); // load the borrowed subject pointer
    abi::emit_call_label(emitter, "__rt_heap_kind");                            // classify its storage
    emitter.instruction(&format!("cmp x0, #{}", CONCAT_TEMP_HEAP_KIND));        // is it a transient concat temporary?
    emitter.instruction(&format!("b.eq {}", skip));                             // nothing can free it: keep borrowing it
    emitter.instruction(&format!("ldp x1, x2, [sp, #{}]", layout.arm_pair.bytes())); // load the borrowed subject pointer and length
    abi::emit_call_label(emitter, "__rt_str_persist");                          // x1 = an owned copy the handler cannot free
    emitter.instruction(&format!("str x1, [sp, #{}]", layout.arm_pair.bytes())); // read the copy from here on
    emitter.instruction(&format!("str x1, [sp, #{}]", layout.arm_owner.bytes())); // and release it once the result is built
    emitter.label(skip);
}

/// Copies the borrowed x86_64 subject into stable storage before a callback.
fn emit_pin_subject_x86_64(emitter: &mut impl Emitter, layout: SnapshotLayout, skip: &str) {
    emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {}]", layout.x86_pointer.bytes())); // load the borrowed subject pointer
    abi::emit_call_label(emitter, "__rt_heap_kind");                            // classify its storage
    emitter.instruction(&format!("cmp eax, {}", CONCAT_TEMP_HEAP_KIND));        // is it a transient concat temporary?
    emitter.instruction(&format!("je {}", skip));                               // nothing can free it: keep borrowing it
    emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {}]", layout.x86_pointer.bytes())); // load the borrowed subject pointer
    emitter.instruction(&format!("mov rdx, QWORD PTR [rbp - {}]", layout.x86_length.bytes())); // load the borrowed subject length
    abi::emit_call_label(emitter, "__rt_str_persist");                          // rax = an owned copy the handler cannot free
    emitter.instruction(&format!("mov QWORD PTR [rbp - {}], rax", layout.x86_pointer.bytes())); // read the copy from here on
    emitter.instruction(&format!("mov QWORD PTR [rbp - {}], rax", layout.x86_owner.bytes())); // and release it once the result is built
    emitter.label(skip);
}

/// Publishes a guard over the private AArch64 copy.
fn emit_copy_guard_push_aarch64(emitter: &mut impl Emitter, layout: SnapshotLayout, guard: CopyGuard<'_>) {
    emitter.instruction(&format!("ldr x9, [sp, #{}]", layout.arm_owner.bytes())); // the private subject copy, or 0
    emitter.emit_reserve_temporary_stack(guard.bytes);                                     // reserve the guard record below the frame
    emitter.instruction(&format!("str x9, [sp, #{}]", guard.owner.bytes()));    // the record's owner
    abi::emit_symbol_address(emitter, "x9", guard.cleanup);
    emitter.instruction("str x9, [sp, #8]");                                    // the cleanup the unwinder calls
    emitter.instruction("mov x0, sp");                                          // the record
    emitter.instruction("mov x1, #0");                                          // link it at the chain head
    abi::emit_call_label(emitter, "__rt_exception_guard_owned");
}

/// Publishes a guard over the private x86_64 copy.
fn emit_copy_guard_push_x86_64(emitter: &mut impl Emitter, layout: SnapshotLayout, guard: CopyGuard<'_>) {
    emitter.emit_reserve_temporary_stack(guard.bytes);                                         // reserve the guard record, keeping calls aligned
    emitter.load_at_offset("r10", layout.x86_owner.bytes());                       // the private subject copy, or 0
    emitter.emit_store_to_sp("r10", guard.owner.bytes());                       // the record's owner
    emitter.emit_local_symbol_address("r10", guard.cleanup);         // the cleanup the unwinder calls
    emitter.instruction("mov QWORD PTR [rsp + 8], r10");                        // store it in the record
    emitter.instruction("mov rdi, rsp");                                        // the record
    emitter.instruction("xor esi, esi");                                        // link it at the chain head
    abi::emit_call_label(emitter, "__rt_exception_guard_owned");
}

/// Emits the unwind callback that frees the private copy.
pub fn emit_unwind(emitter: &mut impl Emitter, guard: CopyGuard<'_>) {
    emitter.label_global(guard.cleanup);
    if emitter.arch() == Arch::X86_64 {
        emitter.instruction(&format!("mov rax, QWORD PTR [rdi + {}]", guard.owner.bytes())); // the private subject copy, or 0
        emitter.instruction("jmp __rt_heap_free_safe");                         // free it and return to the unwinder
        return;
    }
    emitter.instruction(&format!("ldr x0, [x0, #{}]", guard.owner.bytes()));    // the private subject copy, or 0
    emitter.instruction("b __rt_heap_free_safe");                               // free it and return to the unwinder
}
