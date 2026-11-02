//! Purpose:
//! Emits the `__rt_strcopy`, `__rt_strcopy_loop` runtime helper assembly for string copy allocation.
//! Keeps PHP byte-string pointer/length behavior and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - String helpers use PHP pointer/length pairs and target ABI return registers; heap-backed results must remain refcount-compatible.
//! - The destination is RESERVED through `__rt_concat_reserve` before the copy: the scratch
//!   tail when the string fits, an owned heap string otherwise. The copy used to go straight to
//!   `_concat_buf + _concat_off`, so `ucfirst()`, `lcfirst()`, `ucwords()`, and a no-op
//!   `str_pad()` of a string longer than the free scratch overwrote the adjacent globals.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the `__rt_strcopy` runtime helper.
///
/// Copies a PHP byte-string into fresh mutable storage so it can be safely modified in place:
/// the concat scratch when it fits (advancing `_concat_off` past it), otherwise an owned heap
/// string.
///
/// # Input (ARM64 calling convention)
/// - `x1`: source string pointer
/// - `x2`: source string length
///
/// # Output (ARM64 calling convention)
/// - `x1`: new pointer to the copy
/// - `x2`: length (unchanged from input)
///
/// Clobbers every caller-saved register.
pub fn emit_strcopy(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_strcopy_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: strcopy ---");
    emitter.label_global("__rt_strcopy");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #32");                                     // allocate slots for the source slice
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish new frame pointer
    emitter.instruction("stp x1, x2, [sp]");                                    // keep the source slice across the reservation

    // -- reserve the destination: scratch when it fits, an owned heap string otherwise --
    emitter.instruction("mov x0, x2");                                          // the copy needs exactly the source length
    emitter.instruction("bl __rt_concat_reserve");                              // x0 = destination with room for the copy
    emitter.instruction("ldp x1, x2, [sp]");                                    // reload the source slice

    // -- copy bytes from source to the destination --
    emitter.instruction("mov x9, x0");                                          // destination cursor
    emitter.instruction("mov x10, x0");                                         // save destination start pointer
    emitter.instruction("mov x11, x2");                                         // copy length as loop counter
    emitter.label("__rt_strcopy_loop");
    emitter.instruction("cbz x11, __rt_strcopy_done");                          // if no bytes remain, done copying
    emitter.instruction("ldrb w12, [x1], #1");                                  // load byte from source, advance source ptr
    emitter.instruction("strb w12, [x9], #1");                                  // store byte to dest, advance dest ptr
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining byte count
    emitter.instruction("b __rt_strcopy_loop");                                 // continue copying

    // -- publish the scratch offset and return the new pointer --
    emitter.label("__rt_strcopy_done");
    emitter.instruction("mov x1, x10");                                         // return new pointer (start of copy)
    emitter.instruction("bl __rt_concat_publish");                              // advance _concat_off past a scratch-backed copy (x1/x2 preserved)

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the x86_64 Linux variant of `__rt_strcopy`.
///
/// Identical behavior to the ARM64 path but uses x86_64 System V ABI registers:
/// - `rax`: source string pointer (input) / copy pointer (output)
/// - `rdx`: source string length (input, preserved as return value)
///
/// Clobbers every caller-saved register.
fn emit_strcopy_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strcopy ---");
    emitter.label_global("__rt_strcopy");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer and realign the stack
    emitter.instruction("mov rbp, rsp");                                        // establish the strcopy frame
    emitter.instruction("sub rsp, 16");                                         // reserve aligned slots for the source slice
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // keep the source pointer across the reservation
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // keep the source length across the reservation

    // -- reserve the destination: scratch when it fits, an owned heap string otherwise --
    emitter.instruction("mov rax, rdx");                                        // the copy needs exactly the source length
    emitter.instruction("call __rt_concat_reserve");                            // rax = destination with room for the copy
    emitter.instruction("mov r11, rax");                                        // destination cursor
    emitter.instruction("mov r10, rax");                                        // save destination start pointer
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // source cursor
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // copy length as loop counter

    // -- copy bytes from source to the destination --
    emitter.label("__rt_strcopy_loop_linux_x86_64");
    emitter.instruction("test rcx, rcx");                                       // stop once every source byte has been copied
    emitter.instruction("jz __rt_strcopy_done_linux_x86_64");                   // finish once the full source string length has been consumed
    emitter.instruction("mov dl, BYTE PTR [r8]");                               // load one source byte
    emitter.instruction("mov BYTE PTR [r11], dl");                              // store one copied byte
    emitter.instruction("add r8, 1");                                           // advance the source string cursor after copying one byte
    emitter.instruction("add r11, 1");                                          // advance the destination cursor after storing one byte
    emitter.instruction("sub rcx, 1");                                          // decrement the remaining byte count after copying one source byte
    emitter.instruction("jmp __rt_strcopy_loop_linux_x86_64");                  // continue copying bytes until the full source string has been cloned

    // -- publish the scratch offset and return the new pointer --
    emitter.label("__rt_strcopy_done_linux_x86_64");
    emitter.instruction("mov rax, r10");                                        // return the start of the copy
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // return the original string length
    emitter.instruction("call __rt_concat_publish");                            // advance _concat_off past a scratch-backed copy (rax/rdx preserved)
    emitter.instruction("add rsp, 16");                                         // release the source slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the copied string in the standard x86_64 string result registers
}
