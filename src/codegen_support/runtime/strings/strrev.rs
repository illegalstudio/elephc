//! Purpose:
//! Emits the `__rt_strrev`, `__rt_strrev_loop` runtime helper assembly for strrev.
//! Keeps PHP byte-string pointer/length behavior and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - String helpers use PHP pointer/length pairs and target ABI return registers; heap-backed results must remain refcount-compatible.
//! - The destination is reserved through `__rt_concat_reserve` first; reversing straight into
//!   `_concat_buf + _concat_off` overwrote the adjacent globals once the string was longer than
//!   the free scratch.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits `__rt_strrev` which reverses a PHP byte-string into fresh storage.
///
/// Input (ARM64 ABI): `x1` = string ptr, `x2` = string length.
/// Output (ARM64 ABI): `x1` = pointer to reversed string start, `x2` = unchanged length.
/// The result lives in the concat scratch (advancing `_concat_off`) when it fits, otherwise in an
/// owned heap string. Clobbers every caller-saved register.
pub fn emit_strrev(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_strrev_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: strrev ---");
    emitter.label_global("__rt_strrev");

    // -- reserve the destination --
    emitter.instruction("sub sp, sp, #32");                                     // allocate slots for the source slice
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish new frame pointer
    emitter.instruction("stp x1, x2, [sp]");                                    // keep the source slice across the reservation
    emitter.instruction("mov x0, x2");                                          // the result needs exactly the source length
    emitter.instruction("bl __rt_concat_reserve");                              // x0 = destination with room for the result
    emitter.instruction("ldp x1, x2, [sp]");                                    // reload the source slice
    emitter.instruction("mov x9, x0");                                          // destination cursor
    emitter.instruction("mov x10, x0");                                         // save destination start for return value
    emitter.instruction("add x11, x1, x2");                                     // x11 = pointer to end of source string
    emitter.instruction("mov x12, x2");                                         // copy length as loop counter

    // -- copy bytes in reverse order (last-to-first) --
    emitter.label("__rt_strrev_loop");
    emitter.instruction("cbz x12, __rt_strrev_done");                           // if no bytes remain, done reversing
    emitter.instruction("sub x11, x11, #1");                                    // move source pointer backward (from end)
    emitter.instruction("ldrb w13, [x11]");                                     // load byte from current source position
    emitter.instruction("strb w13, [x9], #1");                                  // store to dest (forward order), advance dest
    emitter.instruction("sub x12, x12, #1");                                    // decrement remaining byte count
    emitter.instruction("b __rt_strrev_loop");                                  // continue reversing

    // -- publish the scratch offset and return --
    emitter.label("__rt_strrev_done");
    emitter.instruction("mov x1, x10");                                         // return pointer to reversed string
    emitter.instruction("bl __rt_concat_publish");                              // advance _concat_off past a scratch-backed result (x1/x2 preserved)
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits `__rt_strrev` for the Linux x86_64 target.
///
/// Input (x86_64 System V ABI): `rax` = string ptr, `rdx` = string length.
/// Output (x86_64 System V ABI): `rax` = pointer to reversed string, `rdx` = length.
/// Same storage rules as the ARM64 path.
fn emit_strrev_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strrev ---");
    emitter.label_global("__rt_strrev");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer and realign the stack
    emitter.instruction("mov rbp, rsp");                                        // establish the strrev frame
    emitter.instruction("sub rsp, 16");                                         // reserve aligned slots for the source slice
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // keep the source pointer across the reservation
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // keep the source length across the reservation
    emitter.instruction("mov rax, rdx");                                        // the result needs exactly the source length
    emitter.instruction("call __rt_concat_reserve");                            // rax = destination with room for the result
    emitter.instruction("mov r11, rax");                                        // destination cursor
    emitter.instruction("mov r10, rax");                                        // preserve the reversed-string start pointer for the return pair
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // the source string length
    emitter.instruction("mov rcx, rsi");                                        // remaining-byte counter for the reverse-copy loop
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload the source pointer
    emitter.instruction("add r8, rsi");                                         // seed the reverse source cursor one byte past the end of the source string

    emitter.label("__rt_strrev_loop_linux_x86_64");
    emitter.instruction("test rcx, rcx");                                       // have all source bytes been copied in reverse order?
    emitter.instruction("jz __rt_strrev_done_linux_x86_64");                    // finish once the full source string has been reversed
    emitter.instruction("sub r8, 1");                                           // move the reverse source cursor back to the next byte that should be copied
    emitter.instruction("mov dl, BYTE PTR [r8]");                               // load the next source byte in reverse order
    emitter.instruction("mov BYTE PTR [r11], dl");                              // store the reversed source byte into the destination
    emitter.instruction("add r11, 1");                                          // advance the destination after storing one reversed byte
    emitter.instruction("sub rcx, 1");                                          // decrement the remaining reverse-copy byte count after storing one byte
    emitter.instruction("jmp __rt_strrev_loop_linux_x86_64");                   // continue reversing bytes until the full source string has been emitted

    emitter.label("__rt_strrev_done_linux_x86_64");
    emitter.instruction("mov rax, r10");                                        // return the reversed-string start pointer
    emitter.instruction("mov rdx, rsi");                                        // return the original source-string length
    emitter.instruction("call __rt_concat_publish");                            // advance _concat_off past a scratch-backed result (rax/rdx preserved)
    emitter.instruction("add rsp, 16");                                         // release the source slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the reversed string in the standard x86_64 string result registers
}
