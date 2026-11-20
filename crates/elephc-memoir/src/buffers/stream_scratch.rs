//! Purpose:
//! Owns bounded staging windows for generated stream filters.
//!
//! Called from:
//! - Builtin filter emitters and stream runtime data generation.
//!
//! Key details:
//! - In-place transforms copy back only after finishing the transform in a separate window.

use crate::{data::DataTarget, Arch, Emitter};

/// Capacity of the shared input and expansion windows.
pub const CAPACITY: i64 = 65_536;
/// Most input that can expand from three bytes to four in the staging window.
pub const BASE64_INPUT_LIMIT: i64 = CAPACITY / 4 * 3;
/// Most input that can expand to three bytes per quoted-printable escape.
pub const QUOTED_PRINTABLE_INPUT_LIMIT: i64 = CAPACITY / 3;
const FILTER: &str = "_stream_filter_buf";
const EXPANSION: &str = "_stream_grow_scratch";

/// Declares the shared input and output staging windows.
pub fn append_data(out: &mut String, target: &impl DataTarget) {
    out.push_str(&target.common(FILTER, CAPACITY as usize));
    out.push_str(&target.common(EXPANSION, CAPACITY as usize));
}

/// Loads the input staging window for a stream filter.
pub fn emit_input_address(emitter: &mut impl Emitter, register: &str) {
    emitter.emit_symbol_address(register, FILTER);
}

/// Loads the separate expansion window for a stream filter.
pub fn emit_expansion_address(emitter: &mut impl Emitter, register: &str) {
    emitter.emit_symbol_address(register, EXPANSION);
}

/// Copies the initialized expansion output back to the caller window and returns its length.
pub fn emit_copy_expansion_back(emitter: &mut impl Emitter, loop_label: &str, done: &str) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("mov x5, #0");                                  // start at the first staged output byte
            emitter.label(loop_label);
            emitter.instruction("cmp x5, x6");                                  // compare the copy cursor with the initialized length
            emitter.instruction(&format!("b.ge {done}"));                       // finish once the full output has been copied
            emitter.instruction("ldrb w11, [x4, x5]");                          // load the next staged byte
            emitter.instruction("strb w11, [x1, x5]");                          // store in the caller's bounded window
            emitter.instruction("add x5, x5, #1");                              // advance the copy cursor
            emitter.emit_jump(loop_label);
            emitter.label(done);
            emitter.instruction("mov x2, x6");                                  // return the initialized output length
        }
        Arch::X86_64 => {
            emitter.instruction("xor r9, r9");                                  // start at the first staged output byte
            emitter.label(loop_label);
            emitter.instruction("cmp r9, r10");                                 // compare the copy cursor with the initialized length
            emitter.instruction(&format!("jge {done}"));                        // finish once the full output has been copied
            emitter.instruction("movzx ecx, BYTE PTR [r11 + r9]");              // load the next staged byte
            emitter.instruction("mov BYTE PTR [rax + r9], cl");                 // store in the caller's bounded window
            emitter.instruction("inc r9");                                      // advance the copy cursor
            emitter.emit_jump(loop_label);
            emitter.label(done);
            emitter.instruction("mov rdx, r10");                                // return the initialized output length
        }
    }
    emitter.emit_return();
}

/// Enlarges a positive read reservation to fit the result of a staged expansion.
///
/// The input and output byte count use the allocator argument register. Four output bytes per
/// input byte cover the padded Base64 case and quoted-printable escapes. The expansion window
/// caps actual writes, and the returned reservation never shrinks the requested read.
pub fn emit_growing_read_capacity(emitter: &mut impl Emitter, prefix: &str) {
    let done = format!("{prefix}_reserve");
    let bounded = format!("{prefix}_bounded");
    let (size, scratch, left, right) = match emitter.arch() {
        Arch::AArch64 => ("x0", "x9", "b.hs", "b.lo"),
        Arch::X86_64 => ("rax", "r10", "jae", "jb"),
    };
    emitter.emit_load_int_immediate(scratch, CAPACITY);
    emitter.instruction(&format!("cmp {size}, {scratch}"));                     // the requested bytes already hold any staged result
    emitter.instruction(&format!("{left} {done}"));                             // keep the larger raw read window
    emitter.emit_load_int_immediate(scratch, CAPACITY / 4);
    emitter.instruction(&format!("cmp {size}, {scratch}"));                     // can the entire worst-case expansion fit?
    emitter.instruction(&format!("{right} {bounded}"));                         // multiply smaller reads without overflow
    emitter.emit_load_int_immediate(size, CAPACITY);
    emitter.emit_jump(&done);
    emitter.label(&bounded);
    match emitter.arch() {
        Arch::AArch64 => emitter.instruction("lsl x0, x0, #2"),                 // allow four bytes per source byte
        Arch::X86_64 => emitter.instruction("shl rax, 2"),                      // allow four bytes per source byte
    }
    emitter.label(&done);
}
