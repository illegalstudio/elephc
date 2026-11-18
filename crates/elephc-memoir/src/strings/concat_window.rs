//! Purpose:
//! Grows a scratch-backed byte window before an external chunk is copied.
//!
//! Called from:
//! - Native stream accumulation after the reader has selected the visible length.
//!
//! Key details:
//! - Retires the full scratch reservation before transferring its initialized prefix.

use crate::{Arch, Emitter};

/// Identifies the staged stream window and incoming byte-pair slots.
pub struct ConcatWindow {
    /// Frame offset of the current accumulation buffer.
    pub buffer: usize,
    /// Frame offset of the initialized prefix length.
    pub used: usize,
    /// Frame offset of the reserved capacity.
    pub capacity: usize,
    /// Frame offset of the incoming chunk pointer.
    pub chunk: usize,
    /// Frame offset of the actual bytes the caller will copy.
    pub length: usize,
}

/// Grows the accumulation buffer if needed and reloads the byte pair for the copy loop.
pub fn emit_ensure(emitter: &mut impl Emitter, frame: ConcatWindow, ready: &str) {
    let arm = emitter.arch() == Arch::AArch64;
    let ConcatWindow { buffer, used, capacity, chunk, length } = frame;
    // -- compare the actual returned byte count with the accumulation window --
    if arm {
        emitter.instruction(&format!("ldr x9, [sp, #{used}]"));                 // read the already accumulated byte count
        emitter.instruction(&format!("ldr x10, [sp, #{length}]"));              // inspect the clamped actual chunk length
        emitter.instruction("add x9, x9, x10");                                 // compute the space required before any byte is copied
        emitter.instruction(&format!("ldr x10, [sp, #{capacity}]"));            // compare with the reserved accumulation capacity
        emitter.instruction("cmp x9, x10");                                     // an expanding filter can exceed the requested read size
        emitter.instruction(&format!("b.ls {ready}"));                          // retain the current window when it already fits
        emitter.instruction(&format!("str x9, [sp, #{capacity}]"));             // grow to cover the actual bytes returned
        emitter.instruction(&format!("ldr x1, [sp, #{buffer}]"));               // return the old concat scratch reservation
        emitter.instruction("mov x2, #0");                                      // release the full scratch window before moving to heap storage
        emitter.emit_call_label("__rt_concat_publish");
        emitter.instruction(&format!("ldr x0, [sp, #{buffer}]"));               // preserve the existing accumulation bytes during growth
        emitter.instruction(&format!("ldr x1, [sp, #{used}]"));                 // copy only the initialized prefix
        emitter.instruction(&format!("ldr x2, [sp, #{capacity}]"));             // allocate room for the full actual chunk
        emitter.emit_call_label("__rt_concat_grow");
        emitter.instruction(&format!("str x0, [sp, #{buffer}]"));               // publish the grown accumulation buffer
        emitter.label(ready);
        emitter.instruction(&format!("ldr x1, [sp, #{chunk}]"));                // restore the source pointer after growth clobbered result registers
        emitter.instruction(&format!("ldr x2, [sp, #{length}]"));               // restore the actual chunk length for the copy loop
        emitter.instruction(&format!("ldr x9, [sp, #{used}]"));                 // restore the destination prefix offset
    } else {
        emitter.instruction(&format!("mov r8, QWORD PTR [rbp - {used}]"));      // read the already accumulated byte count
        emitter.instruction(&format!("add r8, QWORD PTR [rbp - {length}]"));    // include the clamped actual chunk length
        emitter.instruction(&format!("cmp r8, QWORD PTR [rbp - {capacity}]"));  // compare actual output with the reserved window
        emitter.instruction(&format!("jbe {ready}"));                           // no allocation is needed when the chunk fits
        emitter.instruction(&format!("mov QWORD PTR [rbp - {capacity}], r8"));  // grow to cover every returned byte
        emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {buffer}]"));   // return the old concat scratch reservation
        emitter.instruction("xor edx, edx");                                    // release the full scratch window before moving to heap storage
        emitter.emit_call_label("__rt_concat_publish");
        emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {buffer}]"));   // preserve the existing accumulation bytes during growth
        emitter.instruction(&format!("mov rdi, QWORD PTR [rbp - {used}]"));     // copy only the initialized prefix
        emitter.instruction(&format!("mov rsi, QWORD PTR [rbp - {capacity}]")); // allocate room for the full actual chunk
        emitter.emit_call_label("__rt_concat_grow");
        emitter.instruction(&format!("mov QWORD PTR [rbp - {buffer}], rax"));   // publish the grown accumulation buffer
        emitter.label(ready);
        emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {chunk}]"));    // restore the source pointer after growth clobbered result registers
        emitter.instruction(&format!("mov rdx, QWORD PTR [rbp - {length}]"));   // restore the actual chunk length for the copy loop
        emitter.instruction(&format!("mov r8, QWORD PTR [rbp - {used}]"));      // restore the destination prefix offset
    }
}
