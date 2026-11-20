//! Purpose:
//! Exposes a bounded, scratch-only window for a producer with incremental output.
//!
//! Called from:
//! - Pattern replacement, which checks remaining space before copying each segment.
//!
//! Key details:
//! - Scratch storage stays fixed during construction. The producer defines its overflow path.
//! - Start, write cursor and one-past-end are stored in caller-selected stack slots.

use crate::{abi, Arch, Emitter};

/// The stack slots holding the start, write cursor, and end of a scratch-only result.
pub struct ScratchWindow {
    output_start_off: usize,
    output_write_off: usize,
    output_end_off: usize,
}

impl ScratchWindow {
    /// Records the caller's existing stack layout.
    pub fn new(output_start_off: usize, output_write_off: usize, output_end_off: usize) -> Self {
        Self { output_start_off, output_write_off, output_end_off }
    }

    /// Captures the scratch tail and exclusive bound without advancing the cursor.
    pub fn emit_prepare(&self, emitter: &mut impl Emitter) {
        let Self { output_start_off, output_write_off, output_end_off } = *self;
        let cap = emitter.concat_capacity();
        match emitter.arch() {
            Arch::AArch64 => {
                abi::emit_symbol_address(emitter, "x9", "_concat_off");
                emitter.instruction("ldr x10, [x9]");                           // load current offset
                abi::emit_symbol_address(emitter, "x11", "_concat_buf");
                emitter.instruction("add x11, x11, x10");                       // output position
                emitter.instruction(&format!("str x11, [sp, #{}]", output_start_off)); // save output start
                emitter.instruction(&format!("str x11, [sp, #{}]", output_write_off)); // save output write pos
                abi::emit_symbol_address(emitter, "x12", "_concat_buf"); // load the arena base
                abi::emit_load_int_immediate(emitter, "x13", cap as i64); // load the shared scratch capacity
                emitter.instruction("add x12, x12, x13");                       // compute the one-past-end arena address
                emitter.instruction(&format!("str x12, [sp, #{}]", output_end_off)); // save the output guard for every copy loop
            }
            Arch::X86_64 => {
                abi::emit_symbol_address(emitter, "r10", "_concat_off");
                emitter.instruction("mov r11, QWORD PTR [r10]");                // load the current concat scratch-buffer offset before appending the replacement output
                abi::emit_symbol_address(emitter, "rax", "_concat_buf");
                emitter.instruction("add rax, r11");                            // compute the first free byte inside the concat scratch buffer for the replacement output
                emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", output_start_off)); // preserve the start of the replacement output string inside the concat scratch buffer
                emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", output_write_off)); // initialize the replacement output write cursor from the output start pointer
                abi::emit_symbol_address(emitter, "rax", "_concat_buf");                    // load the arena base instead of the already-offset output cursor
                abi::emit_load_int_immediate(emitter, "r10", cap as i64);                   // load the shared scratch capacity
                emitter.instruction("add rax, r10");                            // compute the one-past-end arena address
                emitter.instruction(&format!("mov QWORD PTR [rsp + {}], rax", output_end_off)); // save the output guard for every copy loop
            }
        }
    }

    /// Commits the completed string length in x2/rdx to the shared scratch cursor.
    pub fn emit_advance(&self, emitter: &mut impl Emitter) {
        match emitter.arch() {
            Arch::AArch64 => {
                abi::emit_symbol_address(emitter, "x9", "_concat_off");
                emitter.instruction("ldr x10, [x9]");                           // load current offset
                emitter.instruction("add x10, x10, x2");                        // add result length
                emitter.instruction("str x10, [x9]");                           // store updated offset
            }
            Arch::X86_64 => {
                abi::emit_symbol_address(emitter, "r9", "_concat_off");
                emitter.instruction("mov r10, QWORD PTR [r9]");                 // reload the concat scratch-buffer offset before reserving the emitted replacement bytes
                emitter.instruction("add r10, rdx");                            // extend the concat scratch-buffer offset by the length of the replacement result
                emitter.instruction("mov QWORD PTR [r9], r10");                 // publish the updated concat scratch-buffer offset for later string-producing helpers
            }
        }
    }
}
