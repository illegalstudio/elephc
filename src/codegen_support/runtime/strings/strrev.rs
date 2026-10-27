//! Purpose:
//! Emits the `__rt_strrev`, `__rt_strrev_loop` runtime helper assembly for strrev.
//! Keeps PHP byte-string pointer/length behavior and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - String helpers use PHP pointer/length pairs and target ABI return registers; heap-backed results must remain refcount-compatible.
//! - The reversed payload is reserved through `__rt_concat_reserve` and published through
//!   `__rt_concat_publish`, so a string larger than the 64 KiB concat scratch takes owned heap
//!   storage instead of overwriting `_concat_off` and the adjacent heap.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits `__rt_strrev` which reverses a PHP byte-string into fresh mutable storage.
///
/// Input (ARM64 ABI): `x1` = string ptr, `x2` = string length.
/// Output (ARM64 ABI): `x1` = pointer to reversed string start, `x2` = unchanged length.
/// The destination comes from `__rt_concat_reserve` (concat scratch while the result fits,
/// owned heap storage otherwise) and is finished through `__rt_concat_publish`.
///
/// Falls through to `emit_strrev_linux_x86_64` on x86_64; otherwise emits portable ARM64 assembly.
pub fn emit_strrev(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_strrev_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: strrev ---");
    emitter.label_global("__rt_strrev");

    // -- preserve the source pair across the reservation call --
    emitter.instruction("sub sp, sp, #48");                                     // allocate spill space for the source pair and result start
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the strrev frame pointer
    emitter.instruction("stp x1, x2, [sp]");                                    // save the source pointer and length

    // -- reserve exactly the source length before writing anything --
    emitter.instruction("mov x0, x2");                                          // the reversed result has exactly the source length
    emitter.instruction("bl __rt_concat_reserve");                              // reserve concat scratch or owned heap storage
    emitter.instruction("str x0, [sp, #16]");                                   // save the result start for the published pointer
    emitter.instruction("mov x9, x0");                                          // destination cursor

    // -- copy bytes in reverse order (last-to-first) --
    emitter.instruction("ldp x1, x2, [sp]");                                    // reload the source pointer and length
    emitter.instruction("add x11, x1, x2");                                     // x11 = pointer to end of source string
    emitter.instruction("mov x12, x2");                                         // copy length as loop counter
    emitter.label("__rt_strrev_loop");
    emitter.instruction("cbz x12, __rt_strrev_done");                           // if no bytes remain, done reversing
    emitter.instruction("sub x11, x11, #1");                                    // move source pointer backward (from end)
    emitter.instruction("ldrb w13, [x11]");                                     // load byte from current source position
    emitter.instruction("strb w13, [x9], #1");                                  // store to dest (forward order), advance dest
    emitter.instruction("sub x12, x12, #1");                                    // decrement remaining byte count
    emitter.instruction("b __rt_strrev_loop");                                  // continue reversing

    // -- publish the result and return --
    emitter.label("__rt_strrev_done");
    emitter.instruction("ldr x1, [sp, #16]");                                   // return pointer to reversed string
    emitter.instruction("ldr x2, [sp, #8]");                                    // return the unchanged string length
    emitter.instruction("bl __rt_concat_publish");                              // advance the scratch offset only for scratch-backed results
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore the frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the strrev frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits `__rt_strrev` for the Linux x86_64 target.
///
/// Input (x86_64 System V ABI): `rax` = string ptr, `rdx` = string length.
/// Output (x86_64 System V ABI): `rax` = pointer to reversed string, `rdx` = length.
/// The destination is reserved through `__rt_concat_reserve` and published through
/// `__rt_concat_publish`, matching the AArch64 variant.
fn emit_strrev_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strrev ---");
    emitter.label_global("__rt_strrev");

    // -- preserve the source pair across the reservation call --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving the strrev spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved source pair and result start
    emitter.instruction("sub rsp, 48");                                         // reserve aligned spill slots for the source pointer, length, and result start
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the source string pointer across the reservation call
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the source string length across the reservation call

    // -- reserve exactly the source length before writing anything --
    emitter.instruction("mov rax, rdx");                                        // the reversed result has exactly the source length
    emitter.instruction("call __rt_concat_reserve");                            // reserve concat scratch or owned heap storage
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the result start for the published pointer
    emitter.instruction("mov r11, rax");                                        // destination cursor
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // reload the source string pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the source string length
    emitter.instruction("lea r8, [rsi + rdx]");                                 // seed the reverse source cursor one byte past the end of the source string
    emitter.instruction("mov rcx, rdx");                                        // copy the source length into the remaining-byte counter

    emitter.label("__rt_strrev_loop_linux_x86_64");
    emitter.instruction("test rcx, rcx");                                       // have all source bytes been copied in reverse order?
    emitter.instruction("jz __rt_strrev_done_linux_x86_64");                    // finish once the full source string has been reversed
    emitter.instruction("sub r8, 1");                                           // move the reverse source cursor back to the next byte that should be copied
    emitter.instruction("mov dl, BYTE PTR [r8]");                               // load the next source byte in reverse order
    emitter.instruction("mov BYTE PTR [r11], dl");                              // store the reversed source byte into the destination
    emitter.instruction("add r11, 1");                                          // advance the destination after storing one reversed byte
    emitter.instruction("sub rcx, 1");                                          // decrement the remaining reverse-copy byte count
    emitter.instruction("jmp __rt_strrev_loop_linux_x86_64");                   // continue reversing bytes until the full source string has been emitted

    emitter.label("__rt_strrev_done_linux_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the reversed-string start pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // restore the original source-string length into the result-length register
    emitter.instruction("call __rt_concat_publish");                            // advance the scratch offset only for scratch-backed results
    emitter.instruction("add rsp, 48");                                         // release the strrev spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the reversed string in the standard x86_64 string result registers
}
