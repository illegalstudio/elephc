//! Purpose:
//! Emits the `__rt_ptr_write_string` runtime helper assembly for PHP string to raw memory copies.
//! Copies borrowed string bytes into caller-owned storage without writing a trailing NUL byte.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::pointers`.
//!
//! Key details:
//! - The helper does not retain or release the borrowed source string and returns the copied byte count.

use crate::codegen::{emit::Emitter, platform::Arch};

/// Emits the `__rt_ptr_write_string` runtime helper for copying PHP string bytes into raw memory.
/// Dispatches to architecture-specific implementation (ARM64 or x86_64).
///
/// ARM64 ABI: x0 = destination pointer, x1 = source string pointer, x2 = byte length; returns x0 = byte count.
/// x86_64 ABI: rdi = destination pointer, rax = source string pointer, rdx = byte length; returns rax = byte count.
///
/// The source string is borrowed (not consumed) and no trailing NUL is written.
pub(crate) fn emit_ptr_write_string(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_ptr_write_string_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.raw("    .p2align 2");                                             // ensure 4-byte alignment for ARM64 instructions
    emitter.comment("--- runtime: ptr_write_string ---");
    emitter.label_global("__rt_ptr_write_string");

    // -- preserve cursors and byte count for the copy loop --
    emitter.instruction("mov x9, x0");                                          // initialize destination cursor from the checked raw pointer
    emitter.instruction("mov x10, x1");                                         // initialize source cursor from the borrowed PHP string payload
    emitter.instruction("mov x11, x2");                                         // initialize remaining byte counter from the string length
    emitter.instruction("mov x12, x2");                                         // preserve the original byte count for the integer return value

    emitter.label("__rt_ptr_write_string_loop");
    emitter.instruction("cbz x11, __rt_ptr_write_string_done");                 // finish once every source byte has been copied
    emitter.instruction("ldrb w13, [x10], #1");                                 // load the next borrowed source byte and advance the source cursor
    emitter.instruction("strb w13, [x9], #1");                                  // store the byte into raw memory and advance the destination cursor
    emitter.instruction("sub x11, x11, #1");                                    // decrement the number of bytes left to copy
    emitter.instruction("b __rt_ptr_write_string_loop");                        // continue copying until the full string payload has been written

    emitter.label("__rt_ptr_write_string_done");
    emitter.instruction("mov x0, x12");                                         // return the original string byte length as the number of bytes written
    emitter.instruction("ret");                                                 // return to the caller without touching the borrowed source string
}

/// Emits the x86_64 Linux implementation of `__rt_ptr_write_string`.
/// Input: rdi = destination pointer, rax = source string pointer, rdx = byte length.
/// Output: rax = number of bytes written (same as input length).
/// Does not write a trailing NUL; caller retains ownership of destination buffer.
fn emit_ptr_write_string_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ptr_write_string ---");
    emitter.label_global("__rt_ptr_write_string");

    // -- preserve cursors and byte count for the copy loop --
    emitter.instruction("mov r8, rdi");                                         // initialize destination cursor from the checked raw pointer
    emitter.instruction("mov r9, rax");                                         // initialize source cursor from the borrowed PHP string payload
    emitter.instruction("mov rcx, rdx");                                        // initialize remaining byte counter from the string length
    emitter.instruction("mov r10, rdx");                                        // preserve the original byte count for the integer return value

    emitter.label("__rt_ptr_write_string_loop_x86");
    emitter.instruction("test rcx, rcx");                                       // finish once every source byte has been copied
    emitter.instruction("jz __rt_ptr_write_string_done_x86");                   // exit the byte-copy loop when the counter reaches zero
    emitter.instruction("mov r11b, BYTE PTR [r9]");                             // load the next borrowed source byte
    emitter.instruction("mov BYTE PTR [r8], r11b");                             // store the byte into raw memory
    emitter.instruction("add r9, 1");                                           // advance the borrowed source cursor after copying one byte
    emitter.instruction("add r8, 1");                                           // advance the raw destination cursor after copying one byte
    emitter.instruction("sub rcx, 1");                                          // decrement the number of bytes left to copy
    emitter.instruction("jmp __rt_ptr_write_string_loop_x86");                  // continue copying until the full string payload has been written

    emitter.label("__rt_ptr_write_string_done_x86");
    emitter.instruction("mov rax, r10");                                        // return the original string byte length as the number of bytes written
    emitter.instruction("ret");                                                 // return to the caller without touching the borrowed source string
}
