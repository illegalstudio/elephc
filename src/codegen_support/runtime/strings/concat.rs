//! Purpose:
//! Emits the `__rt_concat`, `__rt_concat_cl` runtime helper assembly for runtime string concatenation.
//! Keeps PHP byte-string pointer/length behavior and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - String helpers use PHP pointer/length pairs and target ABI return registers; heap-backed results must remain refcount-compatible.
//! - Destination storage comes from `__rt_concat_reserve`, so a result that no longer fits the
//!   fixed 64 KiB `_concat_buf` scratch buffer lands in an owned heap block instead of running
//!   past the buffer end into the adjacent BSS globals.
//! - A heap-backed result is stamped with `CONCAT_TEMP_HEAP_KIND` so `__rt_str_persist` can
//!   take it over in place rather than allocating a second copy of it.
//! - `left_len + right_len` is checked for unsigned wrap before the reservation, so a wrapped
//!   total can never size a destination smaller than the bytes the copy loops write.

use crate::codegen_support::runtime::strings::concat_scratch::{
    CONCAT_BUF_CAPACITY, CONCAT_TEMP_HEAP_KIND,
};
use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits the `__rt_concat` runtime helper for concatenating two byte-strings.
///
/// Sizes the result through `__rt_concat_reserve` (concat scratch while it fits the shared
/// 64 KiB buffer, an owned heap block otherwise), copies both operands into the reservation,
/// and publishes the written length through `__rt_concat_publish`, which advances `_concat_off`
/// only for scratch-backed results.
/// Dispatches to `emit_concat_linux_x86_64` on x86_64; uses the ARM64 path otherwise.
///
/// Input:  x1=left_ptr, x2=left_len, x3=right_ptr, x4=right_len
/// Output: x1=result_ptr, x2=result_len
///
/// Clobbers every caller-saved register, because the reservation can reach `__rt_heap_alloc`.
/// A wrapped `left_len + right_len` reports PHP's allocation-overflow fatal through
/// `__rt_alloc_overflow` instead of under-sizing the destination.
pub fn emit_concat(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_concat_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: concat ---");
    emitter.label_global("__rt_concat");

    // -- set up stack frame (64 bytes) --
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish new frame pointer

    // -- save input arguments to stack --
    emitter.instruction("stp x1, x2, [sp, #0]");                                // save left string ptr and length
    emitter.instruction("stp x3, x4, [sp, #16]");                               // save right string ptr and length
    emitter.instruction("adds x5, x2, x4");                                     // compute total result length and record unsigned wrap
    emitter.instruction("b.cs __rt_concat_size_overflow");                      // a wrapped total can never describe the bytes the copy loops write
    emitter.instruction("str x5, [sp, #32]");                                   // save total length on stack

    // -- reserve bounded destination storage instead of appending blindly at _concat_off --
    emitter.instruction("mov x0, x5");                                          // request storage for the full concatenated payload
    emitter.instruction("bl __rt_concat_reserve");                              // reserve concat scratch or owned heap storage for the result
    emitter.instruction("str x0, [sp, #40]");                                   // save result start pointer on stack

    // -- stamp heap-backed results so __rt_str_persist can take them over in place --
    crate::codegen_support::runtime::ctx::emit_concat_buf_address(emitter, "x6");
    emitter.instruction("sub x7, x0, x6");                                      // compute the candidate scratch offset of the reservation
    emitter.instruction(&format!("mov x8, #{}", CONCAT_BUF_CAPACITY));          // load the concat scratch capacity in bytes
    emitter.instruction("cmp x7, x8");                                          // is the reservation outside the shared scratch window (unsigned)?
    emitter.instruction("b.lo __rt_concat_dest_ready");                         // scratch-backed reservations carry no heap header to stamp
    emitter.instruction(&format!("mov x8, #{}", CONCAT_TEMP_HEAP_KIND));        // heap kind 7 = transient `.` operator temporary
    emitter.instruction("str x8, [x0, #-8]");                                   // stamp the heap reservation as a concat temporary
    emitter.label("__rt_concat_dest_ready");

    // -- copy left string bytes --
    emitter.instruction("ldp x1, x2, [sp, #0]");                                // reload left ptr and length
    emitter.instruction("mov x10, x0");                                         // set dest cursor to start of output
    emitter.label("__rt_concat_cl");
    emitter.instruction("cbz x2, __rt_concat_cr_setup");                        // if no bytes left, move to right string
    emitter.instruction("ldrb w11, [x1], #1");                                  // load byte from left string, advance src
    emitter.instruction("strb w11, [x10], #1");                                 // store byte to dest, advance dest
    emitter.instruction("sub x2, x2, #1");                                      // decrement remaining left bytes
    emitter.instruction("b __rt_concat_cl");                                    // continue copying left string

    // -- copy right string bytes --
    emitter.label("__rt_concat_cr_setup");
    emitter.instruction("ldp x3, x4, [sp, #16]");                               // reload right ptr and length
    emitter.label("__rt_concat_cr");
    emitter.instruction("cbz x4, __rt_concat_done");                            // if no bytes left, concatenation complete
    emitter.instruction("ldrb w11, [x3], #1");                                  // load byte from right string, advance src
    emitter.instruction("strb w11, [x10], #1");                                 // store byte to dest, advance dest
    emitter.instruction("sub x4, x4, #1");                                      // decrement remaining right bytes
    emitter.instruction("b __rt_concat_cr");                                    // continue copying right string

    // -- publish the written length and return the result --
    emitter.label("__rt_concat_done");
    emitter.instruction("ldr x1, [sp, #40]");                                   // return result pointer (start of output)
    emitter.instruction("ldr x2, [sp, #32]");                                   // return result length
    emitter.instruction("bl __rt_concat_publish");                              // advance the concat scratch offset only for scratch-backed results
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller

    // -- fatal error: left_len + right_len does not fit a machine word --
    emitter.label("__rt_concat_size_overflow");
    emitter.instruction("b __rt_alloc_overflow");                               // unconditional branch keeps the fatal trampoline cross-atom safe
}

/// Emits the x86_64 Linux variant of `__rt_concat`.
/// Uses the System V AMD64 ABI: left string in rax/rdx, right string in rdi/rsi.
/// Result returned in rax (pointer) and rdx (length).
/// Behavior mirrors the ARM64 path: `__rt_concat_reserve` picks concat scratch or an owned
/// heap block, a heap-backed result is stamped as a transient `.` temporary, and
/// `__rt_concat_publish` advances `_concat_off` only for scratch-backed results.
fn emit_concat_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: concat ---");
    emitter.label_global("__rt_concat");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while concat uses stack locals
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for concat bookkeeping
    emitter.instruction("sub rsp, 48");                                         // reserve local slots for input strings, total length, and result pointer

    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save left string pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save left string length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // save right string pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rsi");                       // save right string length
    emitter.instruction("mov r8, rdx");                                         // seed total length from the left string length
    emitter.instruction("add r8, rsi");                                         // total length = left length + right length
    emitter.instruction("jc __rt_concat_size_overflow_x86");                    // a wrapped total can never describe the bytes the copy loops write
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // save total length for the publish step and return

    // -- reserve bounded destination storage instead of appending blindly at _concat_off --
    emitter.instruction("mov rax, r8");                                         // request storage for the full concatenated payload
    emitter.instruction("call __rt_concat_reserve");                            // reserve concat scratch or owned heap storage for the result
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save result start pointer for return

    // -- stamp heap-backed results so __rt_str_persist can take them over in place --
    crate::codegen_support::runtime::ctx::emit_concat_buf_address(emitter, "r8");
    emitter.instruction("mov r9, rax");                                         // copy the reservation before deriving its candidate scratch offset
    emitter.instruction("sub r9, r8");                                          // compute the candidate scratch offset of the reservation
    emitter.instruction(&format!("cmp r9, {}", CONCAT_BUF_CAPACITY));           // is the reservation outside the shared scratch window (unsigned)?
    emitter.instruction("jb __rt_concat_dest_ready_x86");                       // scratch-backed reservations carry no heap header to stamp
    emitter.instruction(&format!("mov r8, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(CONCAT_TEMP_HEAP_KIND))); // materialize the transient-concat heap kind word with the x86_64 heap marker
    emitter.instruction("mov QWORD PTR [rax - 8], r8");                         // stamp the heap reservation as a concat temporary
    emitter.label("__rt_concat_dest_ready_x86");

    emitter.instruction("mov r10, rax");                                        // set the concat destination cursor to the start of the reservation
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // load left source pointer
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // load remaining left byte count
    emitter.label("__rt_concat_cl");
    emitter.instruction("test r9, r9");                                         // check whether all left bytes have been copied
    emitter.instruction("je __rt_concat_cr_setup");                             // continue with the right string when left is exhausted
    emitter.instruction("mov r11b, BYTE PTR [r8]");                             // load one byte from the left string
    emitter.instruction("mov BYTE PTR [r10], r11b");                            // store the byte into the concat destination
    emitter.instruction("add r8, 1");                                           // advance the left source pointer
    emitter.instruction("add r10, 1");                                          // advance the concat destination pointer
    emitter.instruction("sub r9, 1");                                           // decrement remaining left bytes
    emitter.instruction("jmp __rt_concat_cl");                                  // continue copying left bytes

    emitter.label("__rt_concat_cr_setup");
    emitter.instruction("mov r8, QWORD PTR [rbp - 24]");                        // load right source pointer
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");                        // load remaining right byte count
    emitter.label("__rt_concat_cr");
    emitter.instruction("test r9, r9");                                         // check whether all right bytes have been copied
    emitter.instruction("je __rt_concat_done");                                 // finish once the right string is exhausted
    emitter.instruction("mov r11b, BYTE PTR [r8]");                             // load one byte from the right string
    emitter.instruction("mov BYTE PTR [r10], r11b");                            // store the byte into the concat destination
    emitter.instruction("add r8, 1");                                           // advance the right source pointer
    emitter.instruction("add r10, 1");                                          // advance the concat destination pointer
    emitter.instruction("sub r9, 1");                                           // decrement remaining right bytes
    emitter.instruction("jmp __rt_concat_cr");                                  // continue copying right bytes

    emitter.label("__rt_concat_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // return result pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // return result length
    emitter.instruction("call __rt_concat_publish");                            // advance the concat scratch offset only for scratch-backed results
    emitter.instruction("add rsp, 48");                                         // release concat local slots
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return concatenated string in rax/rdx

    // -- fatal error: left_len + right_len does not fit a machine word --
    emitter.label("__rt_concat_size_overflow_x86");
    emitter.instruction("jmp __rt_alloc_overflow");                             // unconditional branch keeps the fatal trampoline reachable from every caller
}

#[cfg(test)]
mod tests {
    use crate::codegen_support::platform::{Arch, Platform, Target};

    use super::*;

    #[test]
    /// Verifies that the x86_64 Linux concat path reserves bounded destination storage,
    /// uses native byte-copy loops, and returns the result pointer/length in rax/rdx per
    /// the AMD64 ABI convention.
    fn test_emit_concat_linux_x86_64_uses_native_copy_loop() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_concat(&mut emitter);
        let asm = emitter.output();

        assert!(asm.contains("__rt_concat:\n"));
        assert!(asm.contains("mov QWORD PTR [rbp - 8], rax\n"));
        assert!(asm.contains("call __rt_concat_reserve\n"));
        assert!(asm.contains("call __rt_concat_publish\n"));
        assert!(asm.contains("mov r11b, BYTE PTR [r8]\n"));
        assert!(asm.contains("mov rax, QWORD PTR [rbp - 48]\n"));
    }

    #[test]
    /// Verifies that the AArch64 concat path bounds its destination through the shared
    /// reservation helpers and rejects a wrapped `left_len + right_len` total.
    fn test_emit_concat_aarch64_reserves_bounded_destination() {
        let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_concat(&mut emitter);
        let asm = emitter.output();

        assert!(asm.contains("bl __rt_concat_reserve\n"));
        assert!(asm.contains("bl __rt_concat_publish\n"));
        assert!(asm.contains("adds x5, x2, x4\n"));
        assert!(asm.contains("b.cs __rt_concat_size_overflow\n"));
    }
}
