//! Purpose:
//! Grows a scratch-backed byte window before a read or an external chunk copy.
//!
//! Called from:
//! - Native line and stream accumulation around reads and filters.
//!
//! Key details:
//! - Retires the full scratch reservation before transferring its initialized prefix.

use crate::{Arch, Emitter};

/// Identifies the buffer, initialized prefix and capacity slots in one native frame.
#[derive(Clone, Copy)]
pub struct WindowSlots {
    /// Frame offset of the current accumulation buffer.
    pub buffer: usize,
    /// Frame offset of the initialized prefix length.
    pub used: usize,
    /// Frame offset of the reserved capacity.
    pub capacity: usize,
}

/// Identifies a window and the actual bytes an external read returned.
pub struct ConcatWindow {
    /// Accumulation window whose ownership is transferred during growth.
    pub slots: WindowSlots,
    /// Frame offset of the incoming chunk pointer.
    pub chunk: usize,
    /// Frame offset of the actual bytes the caller will copy.
    pub length: usize,
}

/// Grows the accumulation buffer if needed and reloads the byte pair for the copy loop.
pub fn emit_ensure(emitter: &mut impl Emitter, frame: ConcatWindow, ready: &str) {
    let arm = emitter.arch() == Arch::AArch64;
    let ConcatWindow { slots, chunk, length } = frame;
    let WindowSlots { used, capacity, .. } = slots;
    // -- compare the actual returned byte count with the accumulation window --
    if arm {
        emitter.instruction(&format!("ldr x9, [sp, #{used}]"));                 // read the already accumulated byte count
        emitter.instruction(&format!("ldr x10, [sp, #{length}]"));              // inspect the clamped actual chunk length
        emitter.instruction("add x9, x9, x10");                                 // compute the space required before any byte is copied
        emitter.instruction(&format!("ldr x10, [sp, #{capacity}]"));            // compare with the reserved accumulation capacity
        emitter.instruction("cmp x9, x10");                                     // an expanding filter can exceed the requested read size
        emitter.instruction(&format!("b.ls {ready}"));                          // retain the current window when it already fits
        emitter.instruction(&format!("str x9, [sp, #{capacity}]"));             // grow to cover the actual bytes returned
        emit_move_window(emitter, slots);
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
        emit_move_window(emitter, slots);
        emitter.label(ready);
        emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {chunk}]"));    // restore the source pointer after growth clobbered result registers
        emitter.instruction(&format!("mov rdx, QWORD PTR [rbp - {length}]"));   // restore the actual chunk length for the copy loop
        emitter.instruction(&format!("mov r8, QWORD PTR [rbp - {used}]"));      // restore the destination prefix offset
    }
}

/// Capacity policy before issuing the next native read.
pub enum ReadAhead {
    /// Read one byte and double the capacity if full.
    Byte,
    /// Reserve a chunk, with an optional frame slot for the caller's byte limit.
    Chunk { bytes: usize, limit: Option<usize> },
}

/// Ensures capacity for the next read while preserving the initialized prefix.
pub fn emit_ensure_before_read(
    emitter: &mut impl Emitter,
    slots: WindowSlots,
    request: ReadAhead,
    ready: &str,
) {
    let (bytes, limit, chunk) = match request {
        ReadAhead::Byte => (1, None, false),
        ReadAhead::Chunk { bytes, limit } => (bytes, limit, true),
    };
    let WindowSlots { used, capacity, .. } = slots;
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr x9, [sp, #{used}]"));             // load the used prefix
            emitter.instruction(&format!("add x9, x9, #{bytes}"));              // include the next request
            if let Some(limit) = limit {
                emitter.instruction(&format!("ldr x10, [sp, #{limit}]"));       // load the caller's byte limit
                emitter.instruction("cmp x9, x10");                             // compare the next request with the limit
                emitter.instruction("csel x9, x9, x10, lt");                    // cap the request
            }
            emitter.instruction(&format!("ldr x10, [sp, #{capacity}]"));        // load the reserved capacity
            emitter.instruction("cmp x9, x10");                                 // check whether the request fits
            emitter.instruction(&format!("b.ls {ready}"));                      // skip growth for a fitting request
            emitter.instruction("lsl x10, x10, #1");                            // double the reserved capacity
            if chunk {
                emitter.instruction("cmp x10, x9");                             // compare the doubled capacity with the chunk request
                emitter.instruction("csel x10, x10, x9, hi");                   // reserve at least the required size
            }
            if let Some(limit) = limit {
                emitter.instruction(&format!("ldr x11, [sp, #{limit}]"));       // reload the caller's byte limit
                emitter.instruction("cmp x10, x11");                            // check the increased capacity against the limit
                emitter.instruction("csel x10, x10, x11, lt");                  // keep the reservation within the limit
            }
            emitter.instruction(&format!("str x10, [sp, #{capacity}]"));        // publish the new capacity for growth
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov r8, QWORD PTR [rbp - {used}]"));  // load the used prefix
            emitter.instruction(&format!("add r8, {bytes}"));                   // include the next request
            if let Some(limit) = limit {
                emitter.instruction(&format!("mov r9, QWORD PTR [rbp - {limit}]")); // load the caller's byte limit
                emitter.instruction("cmp r8, r9");                              // compare the next request with the limit
                emitter.instruction("cmovg r8, r9");                            // cap the request
            }
            emitter.instruction(&format!("mov r9, QWORD PTR [rbp - {capacity}]")); // load the reserved capacity
            emitter.instruction("cmp r8, r9");                                  // check whether the request fits
            emitter.instruction(&format!("jbe {ready}"));                       // skip growth for a fitting request
            emitter.instruction("add r9, r9");                                  // double the reserved capacity
            if chunk {
                emitter.instruction("cmp r9, r8");                              // compare the doubled capacity with the chunk request
                emitter.instruction("cmovb r9, r8");                            // reserve at least the required size
            }
            if let Some(limit) = limit {
                emitter.instruction(&format!("mov r10, QWORD PTR [rbp - {limit}]")); // reload the caller's byte limit
                emitter.instruction("cmp r9, r10");                             // check the increased capacity against the limit
                emitter.instruction("cmovg r9, r10");                           // keep the reservation within the limit
            }
            emitter.instruction(&format!("mov QWORD PTR [rbp - {capacity}], r9")); // publish the new capacity for growth
        }
    }
    emit_move_window(emitter, slots);
    emitter.label(ready);
}

/// Relinquishes the old scratch window and publishes the grown buffer.
fn emit_move_window(emitter: &mut impl Emitter, slots: WindowSlots) {
    let WindowSlots { buffer, used, capacity } = slots;
    if emitter.arch() == Arch::AArch64 {
        emitter.instruction(&format!("ldr x1, [sp, #{buffer}]"));               // return the old concat scratch reservation
        emitter.instruction("mov x2, #0");                                      // release the full scratch window before moving to heap storage
        emitter.emit_call_label("__rt_concat_publish");
        emitter.instruction(&format!("ldr x0, [sp, #{buffer}]"));               // preserve the existing accumulation bytes during growth
        emitter.instruction(&format!("ldr x1, [sp, #{used}]"));                 // copy only the initialized prefix
        emitter.instruction(&format!("ldr x2, [sp, #{capacity}]"));             // allocate room for the full actual chunk
        emitter.emit_call_label("__rt_concat_grow");
        emitter.instruction(&format!("str x0, [sp, #{buffer}]"));               // publish the grown accumulation buffer
    } else {
        emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {buffer}]"));   // return the old concat scratch reservation
        emitter.instruction("xor edx, edx");                                    // release the full scratch window before moving to heap storage
        emitter.emit_call_label("__rt_concat_publish");
        emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {buffer}]"));   // preserve the existing accumulation bytes during growth
        emitter.instruction(&format!("mov rdi, QWORD PTR [rbp - {used}]"));     // copy only the initialized prefix
        emitter.instruction(&format!("mov rsi, QWORD PTR [rbp - {capacity}]")); // allocate room for the full actual chunk
        emitter.emit_call_label("__rt_concat_grow");
        emitter.instruction(&format!("mov QWORD PTR [rbp - {buffer}], rax"));   // publish the grown accumulation buffer
    }
}

/// Discards a read chunk by rewinding scratch storage or retiring a heap owner.
/// The current string result contains the chunk pointer. Some callers reload it from their frame.
pub fn emit_discard_chunk(emitter: &mut impl Emitter, spill: Option<usize>) {
    // -- first return the scratch window, then release any owned chunk --
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction("mov x2, #0");                                  // return the entire temporary window
            emitter.instruction("bl __rt_concat_publish");                      // rewind only a scratch-backed chunk
            if let Some(spill) = spill {
                emitter.instruction(&format!("ldr x0, [sp, #{spill}]"));        // pass the saved chunk to the release helper
            } else {
                emitter.instruction("mov x0, x1");                              // pass the published pointer to the release helper
            }
        }
        Arch::X86_64 => {
            emitter.instruction("xor edx, edx");                                // return the entire temporary window
            emitter.instruction("call __rt_concat_publish");                    // rewind only a scratch-backed chunk
            if let Some(spill) = spill {
                emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {spill}]")); // reload the chunk saved for release
            }
        }
    }
    emitter.emit_call_label("__rt_decref_any");
}
