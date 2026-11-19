//! Purpose:
//! Emits temporary word-vector allocation and retirement through libc.
//!
//! Called from:
//! - Float sorting and native regex capture emitters.
//!
//! Key details:
//! - The vector's element count uses the caller's existing bounds.
//! - These buffers always use the matching libc deallocator.

use crate::{Arch, Emitter};

/// Controls when a frame slot stops publishing its libc-owned pointer.
#[derive(Clone, Copy)]
pub enum RetireSlot {
    /// The caller leaves the original spill in a frame it is about to retire.
    Keep,
    /// Clear the pointer before releasing the owner on an unwind path.
    BeforeFree,
    /// Release first and then clear the pointer before reusing the frame.
    AfterFree,
}

/// Allocates a native word vector whose count is stored at a fixed local frame offset.
pub fn emit_allocate_frame_vector(
    emitter: &mut impl Emitter, count: usize, slot: usize, ready: &str,
) {
    let argument = emitter.int_arg_reg_name(0);
    let result = emitter.int_result_reg();
    emitter.load_at_offset(argument, count);
    emit_allocate_counted_words(emitter);
    emitter.store_at_offset(result, slot);
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #0");                                  // detect allocation failure without changing the returned pointer
            emitter.instruction(&format!("b.ne {ready}"));                      // enter the ready path on successful allocation
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 0");                                  // detect allocation failure without changing the returned pointer
            emitter.instruction(&format!("jne {ready}"));                       // enter the ready path on successful allocation
        }
    }
    emitter.emit_jump("__rt_heap_allocation_failed");
}

/// Frees the native word vector saved in a fixed local frame slot.
pub fn emit_release_frame_vector(emitter: &mut impl Emitter, slot: usize) {
    emitter.load_at_offset(emitter.int_arg_reg_name(0), slot);
    emitter.bl_c("free");
}

/// Allocates a native word vector whose count is staged relative to the current stack pointer.
pub fn emit_allocate_stack_vector(
    emitter: &mut impl Emitter, count: usize, slot: usize, failure: &str,
) {
    load_stack_argument(emitter, count);
    emit_allocate_counted_words(emitter);
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz x0, {failure}"));                 // leave allocation failure in the caller's cleanup path
            emitter.instruction(&format!("str x0, [sp, #{slot}]"));             // publish only a successful vector
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // inspect the new native allocation
            emitter.instruction(&format!("jz {failure}"));                      // leave allocation failure in the caller's cleanup path
            emitter.instruction(&format!("mov QWORD PTR [rsp + {slot}], rax")); // publish only a successful vector
        }
    }
}

/// Frees the libc-owned offset or row vector staged relative to the current stack pointer.
pub fn emit_release_stack_vector(emitter: &mut impl Emitter, slot: usize, retire: RetireSlot) {
    load_stack_argument(emitter, slot);
    if matches!(retire, RetireSlot::BeforeFree) {
        clear_stack_slot(emitter, slot);
    }
    emitter.bl_c("free");
    if matches!(retire, RetireSlot::AfterFree) {
        clear_stack_slot(emitter, slot);
    }
}

/// Releases a non-null staging pointer, leaving the null branch under caller control.
pub fn emit_release_stack_vector_or_jump(emitter: &mut impl Emitter, slot: usize, empty: &str) {
    load_stack_argument(emitter, slot);
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz x0, {empty}"));                   // keep the caller's null-owner branch
        }
        Arch::X86_64 => {
            emitter.instruction("test rdi, rdi");                               // check the staged vector's pointer
            emitter.instruction(&format!("jz {empty}"));                        // keep the caller's null-owner branch
        }
    }
    emitter.bl_c("free");
}

/// Scales the caller-validated count before calling libc.
fn emit_allocate_counted_words(emitter: &mut impl Emitter) {
    emitter.instruction(match emitter.arch() {
        Arch::AArch64 => "lsl x0, x0, #3",
        Arch::X86_64 => "shl rdi, 3",
    });                                                                         // reserve one native word per element
    emitter.bl_c("malloc");
}

/// Loads a stack-relative count or pointer into the first native C argument.
fn load_stack_argument(emitter: &mut impl Emitter, slot: usize) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr x0, [sp, #{slot}]"));             // read the staged native argument
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {slot}]")); // read the staged native argument
        }
    }
}

/// Detaches a libc owner from its published native spill slot.
fn clear_stack_slot(emitter: &mut impl Emitter, slot: usize) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("str xzr, [sp, #{slot}]"));            // clear the released pointer slot
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov QWORD PTR [rsp + {slot}], 0"));   // clear the released pointer slot
        }
    }
}
