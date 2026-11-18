//! Purpose:
//! Adopts native owned payloads directly into fresh Mixed cells.
//!
//! Called from:
//! - Compiler value boxing and native I/O result adapters.
//!
//! Key details:
//! - Inputs already carry the owner being transferred into the cell.
//! - Result validation and tag selection happen at the call site.

use crate::layout::{MIXED_CELL_SIZE, MIXED_HEAP_KIND};
use crate::{Arch, AssemblyEmitter, Emitter, HeapKind, TargetAbi, ValueTag};

const STRING_TAG: ValueTag = ValueTag::new(1);

/// Adopts the owned string return pair using the expression-result spill sequence.
pub fn emit_owned_string(emitter: &mut (impl AssemblyEmitter + TargetAbi)) {
    spill_string(emitter);
    emit_cell_allocation(emitter);
    match emitter.arch() {
        Arch::AArch64 => {
            emit_aarch64_tag(emitter, "x10", STRING_TAG);
            emitter.instruction("ldp x11, x12, [sp], #16");                     // recover the transferred pointer and length
            emitter.instruction("stp x11, x12, [x0, #8]");                      // move both payload words into the new cell
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov QWORD PTR [rax], {STRING_TAG}")); // tag the cell as a string
            emitter.emit_load_temporary_stack_slot("r10", 0);
            emitter.instruction("mov QWORD PTR [rax + 8], r10");                // move the pointer into the new cell
            emitter.emit_load_temporary_stack_slot("r10", 8);
            emitter.instruction("mov QWORD PTR [rax + 16], r10");               // move the length into the new cell
            emitter.emit_release_temporary_stack(16);
        }
    }
}

/// Adopts the owned string result of a native helper using the paired spill sequence.
pub fn emit_owned_string_pair(emitter: &mut (impl AssemblyEmitter + TargetAbi)) {
    spill_string(emitter);
    emit_cell_allocation(emitter);
    match emitter.arch() {
        Arch::AArch64 => {
            emit_aarch64_tag(emitter, "x9", STRING_TAG);
            emitter.instruction("ldp x10, x11, [sp], #16");                     // recover the transferred pointer and length
            emitter.instruction("stp x10, x11, [x0, #8]");                      // transfer the complete string pair
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov r10, {STRING_TAG}"));             // prepare the string tag before restoring the pair
            emitter.instruction("mov QWORD PTR [rax], r10");                    // publish the string tag
            emitter.emit_load_temporary_stack_slot("r10", 0);
            emitter.emit_load_temporary_stack_slot("r11", 8);
            emitter.emit_release_temporary_stack(16);
            emitter.instruction("mov QWORD PTR [rax + 8], r10");                // transfer the string pointer
            emitter.instruction("mov QWORD PTR [rax + 16], r11");               // transfer the string length
        }
    }
}

/// Adopts one already-owned pointer in the native result register into a tagged cell.
pub fn emit_owned_pointer(emitter: &mut (impl AssemblyEmitter + TargetAbi), tag: ValueTag) {
    emitter.emit_push_reg(emitter.int_result_reg());
    emit_cell_allocation(emitter);
    match emitter.arch() {
        Arch::AArch64 => {
            emit_aarch64_tag(emitter, "x9", tag);
            emitter.emit_pop_reg("x10");
            emitter.instruction("str x10, [x0, #8]");                           // transfer the one-word pointer payload
            emitter.instruction("str xzr, [x0, #16]");                          // clear the unused high word
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov QWORD PTR [rax], {tag}"));        // select the caller's storage tag
            emitter.emit_pop_reg("r10");
            emitter.instruction("mov QWORD PTR [rax + 8], r10");                // transfer the one-word pointer payload
            emitter.instruction("mov QWORD PTR [rax + 16], 0");                 // clear the unused high word
        }
    }
}

/// Adopts an already-owned pointer saved in a native frame slot into a fresh Mixed cell.
/// The caller supplies the architecture's memory operand, which stays valid across allocation.
pub fn emit_owned_pointer_from_slot(emitter: &mut impl Emitter, tag: ValueTag, source_slot: &str) {
    emit_cell_allocation(emitter);
    match emitter.arch() {
        Arch::AArch64 => {
            emit_aarch64_tag(emitter, "x10", tag);
            emitter.instruction(&format!("ldr x11, {source_slot}"));            // recover the staged owner
            emitter.instruction("str x11, [x0, #8]");                           // transfer the pointer into the low payload word
            emitter.instruction("str xzr, [x0, #16]");                          // clear the unused high payload word
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov QWORD PTR [rax], {tag}"));        // write the selected pointer tag
            emitter.instruction(&format!("mov r10, {source_slot}"));            // recover the staged owner
            emitter.instruction("mov QWORD PTR [rax + 8], r10");                // transfer the pointer into the low payload word
            emitter.instruction("mov QWORD PTR [rax + 16], 0");                 // clear the unused high payload word
        }
    }
}

/// Adopts a pointer with a runtime tag already saved alongside it in the native frame.
pub fn emit_owned_tagged_pointer_from_slots(emitter: &mut impl Emitter, tag: &str, payload: &str) {
    emit_cell_allocation(emitter);
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr x9, {tag}"));                     // recover the staged runtime tag
            emitter.instruction("str x9, [x0]");                                // publish the tag in the independent cell
            emitter.instruction(&format!("ldr x1, {payload}"));                 // recover the owned pointer
            emitter.instruction("str x1, [x0, #8]");                            // transfer the pointer to the cell
            emitter.instruction("str xzr, [x0, #16]");                          // clear the unused high word
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov r10, {tag}"));                    // recover the staged runtime tag
            emitter.instruction("mov QWORD PTR [rax], r10");                    // publish the tag in the independent cell
            emitter.instruction(&format!("mov r10, {payload}"));                // recover the owned pointer
            emitter.instruction("mov QWORD PTR [rax + 8], r10");                // transfer the pointer to the cell
            emitter.instruction("mov QWORD PTR [rax + 16], 0");                 // clear the unused high word
        }
    }
}

/// Preserves the native string pair across cell allocation.
fn spill_string(emitter: &mut (impl AssemblyEmitter + TargetAbi)) {
    match emitter.arch() {
        Arch::AArch64 => emitter.instruction("stp x1, x2, [sp, #-16]!"),        // park the native pointer and length
        Arch::X86_64 => {
            emitter.emit_reserve_temporary_stack(16);
            emitter.emit_store_to_sp("rax", 0);
            emitter.emit_store_to_sp("rdx", 8);
        }
    }
}

/// Allocates one Mixed cell and stamps its uniform heap header.
fn emit_cell_allocation(emitter: &mut (impl AssemblyEmitter + TargetAbi)) {
    emitter.emit_load_int_immediate(emitter.int_result_reg(), MIXED_CELL_SIZE as i64);
    crate::heap_alloc::emit_fresh_with_kind(emitter, HeapKind::new(MIXED_HEAP_KIND));
}

/// Writes the storage tag using the scratch register selected by the spill sequence.
fn emit_aarch64_tag(emitter: &mut impl AssemblyEmitter, scratch: &str, tag: ValueTag) {
    emitter.instruction(&format!("mov {scratch}, #{tag}"));                     // materialize the selected storage tag
    emitter.instruction(&format!("str {scratch}, [x0]"));                       // publish the tag in the fresh cell
}
