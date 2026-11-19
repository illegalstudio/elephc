//! Purpose:
//! Builds packed two-element rows from an owned cell and an integer offset.
//!
//! Called from:
//! - Native regex result emitters.
//!
//! Key details:
//! - The caller prepares the first cell and retires the separate array owner.

use crate::{Arch, Emitter};

/// Stack-relative slots used to construct a packed boxed offset row.
pub struct BoxedOffsetRow {
    /// Slot for the raw packed array owner.
    pub array: usize,
    /// Owns the first cell on entry and receives the boxed row.
    pub value: usize,
    /// Holds the unboxed integer offset.
    pub offset: usize,
}

impl BoxedOffsetRow {
    /// Allocates the array before the caller installs the first cell.
    pub fn emit_allocate(&self, emitter: &mut impl Emitter) {
        match emitter.arch() {
            Arch::AArch64 => {
                emitter.instruction("mov x0, #2");                              // capacity for [value, offset]
                emitter.instruction("mov x1, #8");                              // row stores boxed Mixed pointers
                emitter.instruction("bl __rt_array_new");                       // allocate the offset-capture row
                emit_mixed_header(emitter, "x0");
                emitter.instruction(&format!("str x0, [sp, #{}]", self.array)); // save the offset-capture row pointer
            }
            Arch::X86_64 => {
                emitter.instruction("mov edi, 2");                              // capacity for [value, offset]
                emitter.instruction("mov esi, 8");                              // row stores boxed Mixed pointers
                emitter.instruction("call __rt_array_new");                     // allocate the offset-capture row
                emit_mixed_header(emitter, "rax");
                emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", self.array)); // save the offset-capture row pointer
            }
        }
    }

    /// Installs the owned first cell, boxes the offset, then boxes the complete row.
    pub fn emit_box(&self, emitter: &mut impl Emitter) {
        match emitter.arch() {
            Arch::AArch64 => {
                emitter.instruction(&format!("ldr x9, [sp, #{}]", self.array)); // reload the row array
                emitter.instruction(&format!("ldr x10, [sp, #{}]", self.value)); // reload the boxed value cell
                emitter.instruction("str x10, [x9, #24]");                      // store row[0] = boxed value
                emitter.instruction("mov x11, #1");                             // row length after storing the value cell
                emitter.instruction("str x11, [x9]");                           // publish row length 1
                emitter.instruction("mov x0, #0");                              // runtime value tag 0 = integer
                emitter.instruction(&format!("ldr x1, [sp, #{}]", self.offset)); // load the absolute or unmatched offset
                emitter.instruction("mov x2, xzr");                             // integer payload has no high word
                emitter.instruction("bl __rt_mixed_from_value");                // box the integer offset
                emitter.instruction(&format!("ldr x9, [sp, #{}]", self.array)); // reload the row array
                emitter.instruction("str x0, [x9, #32]");                       // store row[1] = boxed offset
                emitter.instruction("mov x11, #2");                             // row length after storing both cells
                emitter.instruction("str x11, [x9]");                           // publish row length 2
                emitter.instruction("mov x0, #4");                              // runtime value tag 4 = indexed array
                emitter.instruction(&format!("ldr x1, [sp, #{}]", self.array)); // load the row array for boxing
                emitter.instruction("mov x2, xzr");                             // indexed-array payload has no high word
                emitter.instruction("bl __rt_mixed_from_value");                // box the offset-capture row as Mixed
                emitter.instruction(&format!("str x0, [sp, #{}]", self.value)); // save the boxed [value, offset] cell
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", self.array)); // reload the row array
                emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {}]", self.value)); // reload the boxed value cell
                emitter.instruction("mov QWORD PTR [r9 + 24], r10");            // store row[0] = boxed value
                emitter.instruction("mov QWORD PTR [r9], 1");                   // publish row length 1
                emitter.instruction("xor eax, eax");                            // runtime value tag 0 = integer
                emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", self.offset)); // load the absolute or unmatched offset
                emitter.instruction("xor esi, esi");                            // integer payload has no high word
                emitter.instruction("call __rt_mixed_from_value");              // box the integer offset
                emitter.instruction(&format!("mov r9, QWORD PTR [rsp + {}]", self.array)); // reload the row array
                emitter.instruction("mov QWORD PTR [r9 + 32], rax");            // store row[1] = boxed offset
                emitter.instruction("mov QWORD PTR [r9], 2");                   // publish row length 2
                emitter.instruction("mov rax, 4");                              // runtime value tag 4 = indexed array
                emitter.instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", self.array)); // load the row array for boxing
                emitter.instruction("xor esi, esi");                            // indexed-array payload has no high word
                emitter.instruction("call __rt_mixed_from_value");              // box the offset-capture row as Mixed
                emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", self.value)); // save the boxed [value, offset] cell
            }
        }
    }
}

/// Stamps the packed Mixed header using the capture emitter's scratch registers.
pub fn emit_mixed_header(emitter: &mut impl Emitter, array_reg: &str) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr x10, [{array_reg}, #-8]"));       // load indexed-array packed kind word
            emitter.instruction("mov x11, #0x80ff");                            // preserve indexed-array kind and persistent COW flag
            emitter.instruction("and x10, x10, x11");                           // clear stale value_type bits
            emitter.instruction("mov x11, #7");                                 // runtime value_type 7 = boxed Mixed
            emitter.instruction("lsl x11, x11, #8");                            // move Mixed tag into packed kind word
            emitter.instruction("orr x10, x10, x11");                           // combine stable metadata with Mixed tag
            emitter.instruction(&format!("str x10, [{array_reg}, #-8]"));       // store boxed-Mixed indexed-array metadata
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov r10, QWORD PTR [{} - 8]", array_reg)); // load indexed-array packed kind word
            emitter.instruction(&format!(
                "mov r8, 0x{:x}",
                crate::layout::x86_64_heap_kind_word(0x80ff)
            ));                                                                 // preserve heap magic, indexed kind, and COW flag
            emitter.instruction("and r10, r8");                                 // clear stale value_type bits
            emitter.instruction("or r10, 0x700");                               // stamp runtime value_type 7 = boxed Mixed
            emitter.instruction(&format!("mov QWORD PTR [{} - 8], r10", array_reg)); // store boxed-Mixed indexed-array metadata
        }
    }
}
