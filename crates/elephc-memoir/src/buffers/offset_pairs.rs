//! Purpose:
//! Allocates a temporary native vector of two-word offset pairs.
//!
//! Called from:
//! - Compiler emitters that pass frame-owned offset vectors to native matchers.
//!
//! Key details:
//! - The caller supplies the pair count and handles failure and retirement.
//! - The pointer slot is published only after size validation and allocation.

use crate::{Arch, Emitter};

/// Reserves a libc-owned offset vector and stores it relative to the current stack pointer.
pub fn emit_allocate(emitter: &mut impl Emitter, count: &str, slot: usize, failure: &str) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("lsr x10, {count}, #60"));             // check that the pair count can scale by sixteen
            emitter.instruction(&format!("cbnz x10, {failure}"));               // preserve the caller's allocation failure path
            emitter.instruction(&format!("lsl x0, {count}, #4"));               // pass the checked byte count to malloc
            emitter.bl_c("malloc");
            emitter.instruction(&format!("cbz x0, {failure}"));                 // leave an unsuccessful allocation unpublished
            emitter.instruction(&format!("str x0, [sp, #{slot}]"));             // save the allocated vector in the frame
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov r10, {count}"));                  // keep the pair count for the allocation argument
            emitter.instruction("shr r10, 60");                                 // check that the pair count can scale by sixteen
            emitter.instruction(&format!("jnz {failure}"));                     // preserve the caller's allocation failure path
            if count != "rdi" {
                emitter.instruction(&format!("mov rdi, {count}"));              // move the checked count to the malloc argument register
            }
            emitter.instruction("shl rdi, 4");                                  // pass the checked byte count to malloc
            emitter.bl_c("malloc");
            emitter.instruction("test rax, rax");                               // check the allocation result before publication
            emitter.instruction(&format!("jz {failure}"));                      // keep the caller's vector slot unchanged on failure
            emitter.instruction(&format!("mov QWORD PTR [rsp + {slot}], rax")); // save the allocated vector in the frame
        }
    }
}
