//! Purpose:
//! Emits the `__rt_strcopy`, `__rt_strcopy_loop` runtime helper assembly for string copy allocation.
//! Keeps PHP byte-string pointer/length behavior and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - String helpers use PHP pointer/length pairs and target ABI return registers; heap-backed results must remain refcount-compatible.
//! - The destination comes from `__rt_concat_reserve` and is finished through `__rt_concat_publish`, so
//!   a copy larger than the 64 KiB concat scratch takes owned heap storage instead of running off the
//!   end of the arena into the adjacent BSS globals.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the `__rt_strcopy` runtime helper.
///
/// Copies a PHP byte-string into fresh mutable storage so it can be safely
/// modified in-place. The destination is reserved through `__rt_concat_reserve`,
/// which returns concat scratch while the copy still fits and an owned heap
/// block otherwise; `__rt_concat_publish` then advances `_concat_off` only for
/// the scratch-backed case.
///
/// # Input (ARM64 calling convention)
/// - `x1`: source string pointer
/// - `x2`: source string length
///
/// # Output (ARM64 calling convention)
/// - `x1`: new pointer to the mutable copy
/// - `x2`: length (unchanged from input)
pub fn emit_strcopy(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_strcopy_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: strcopy ---");
    emitter.label_global("__rt_strcopy");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #48");                                     // allocate spill space for the source pair and result start
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the strcopy frame pointer
    emitter.instruction("stp x1, x2, [sp]");                                    // save the source pointer and length across the reservation call

    // -- reserve exactly enough mutable storage for the copy --
    emitter.instruction("mov x0, x2");                                          // request one byte per source byte
    emitter.instruction("bl __rt_concat_reserve");                              // reserve concat scratch or owned heap storage
    emitter.instruction("str x0, [sp, #16]");                                   // save the result start for the published pointer
    emitter.instruction("mov x9, x0");                                          // destination cursor

    // -- copy bytes from source to the reserved destination --
    emitter.instruction("ldp x1, x2, [sp]");                                    // reload the source pointer and length
    emitter.instruction("mov x11, x2");                                         // copy length as loop counter
    emitter.label("__rt_strcopy_loop");
    emitter.instruction("cbz x11, __rt_strcopy_done");                          // if no bytes remain, done copying
    emitter.instruction("ldrb w12, [x1], #1");                                  // load byte from source, advance source ptr
    emitter.instruction("strb w12, [x9], #1");                                  // store byte to dest, advance dest ptr
    emitter.instruction("sub x11, x11, #1");                                    // decrement remaining byte count
    emitter.instruction("b __rt_strcopy_loop");                                 // continue copying

    // -- publish the result and return the new pointer --
    emitter.label("__rt_strcopy_done");
    emitter.instruction("ldr x1, [sp, #16]");                                   // return the mutable copy start pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // return the copied length
    emitter.instruction("bl __rt_concat_publish");                              // advance the scratch offset only for scratch-backed results

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate the strcopy frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the x86_64 Linux variant of `__rt_strcopy`.
///
/// Identical behavior to the ARM64 path but uses x86_64 System V ABI registers:
/// - `rax`: source string pointer (input)
/// - `rdx`: source string length (input)
///
/// After the copy, `_concat_off` is advanced by the string length when the
/// result lives in the shared scratch arena. The new pointer is returned in
/// `rax` and the length in `rdx`.
fn emit_strcopy_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strcopy ---");
    emitter.label_global("__rt_strcopy");

    // -- preserve the source pair across the reservation call --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving the strcopy spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved source pair and result start
    emitter.instruction("sub rsp, 48");                                         // reserve aligned spill slots for the source pointer, length, and result start
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the source string pointer across the reservation call
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the source string length across the reservation call

    // -- reserve exactly enough mutable storage for the copy --
    emitter.instruction("mov rax, rdx");                                        // request one byte per source byte
    emitter.instruction("call __rt_concat_reserve");                            // reserve concat scratch or owned heap storage
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the result start for the published pointer
    emitter.instruction("mov r9, rax");                                         // destination cursor

    // -- copy bytes from source to the reserved destination --
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // reload the source string pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the remaining byte count
    emitter.label("__rt_strcopy_loop_linux_x86_64");
    emitter.instruction("test rcx, rcx");                                       // stop once every source byte has been copied
    emitter.instruction("jz __rt_strcopy_done_linux_x86_64");                   // finish once the full source string length has been consumed
    emitter.instruction("mov dl, BYTE PTR [rsi]");                              // load one source byte
    emitter.instruction("mov BYTE PTR [r9], dl");                               // store one copied byte into reserved storage
    emitter.instruction("add rsi, 1");                                          // advance the source string cursor after copying one byte
    emitter.instruction("add r9, 1");                                           // advance the destination cursor after storing one byte
    emitter.instruction("sub rcx, 1");                                          // decrement the remaining byte count
    emitter.instruction("jmp __rt_strcopy_loop_linux_x86_64");                  // continue copying bytes until the full source string has been cloned

    // -- publish the result and return the new pointer --
    emitter.label("__rt_strcopy_done_linux_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the mutable copy start pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // return the copied length
    emitter.instruction("call __rt_concat_publish");                            // advance the scratch offset only for scratch-backed results
    emitter.instruction("add rsp, 48");                                         // release the strcopy spill slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the mutable copy in the standard x86_64 string result registers
}
