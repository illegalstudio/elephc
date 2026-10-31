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
    emitter.instruction("sub sp, sp, #48");                                     // allocate slots for the source slice and destination
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish new frame pointer
    emitter.instruction("stp x1, x2, [sp]");                                    // keep the source slice across the reservation
    emitter.instruction("str x19, [sp, #16]");                                  // preserve the destination across the libc copy

    // -- reserve the destination: scratch when it fits, an owned heap string otherwise --
    emitter.instruction("mov x0, x2");                                          // the copy needs exactly the source length
    emitter.instruction("bl __rt_concat_reserve");                              // x0 = destination with room for the copy
    emitter.instruction("mov x19, x0");                                         // keep the destination start across memcpy
    emitter.instruction("ldr x1, [sp]");                                        // reload the source pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the source length
    emitter.bl_c("memcpy");                                                     // copy the whole source with the libc vectorized routine

    // -- publish the scratch offset and return the new pointer --
    emitter.instruction("mov x1, x19");                                         // return new pointer (start of copy)
    emitter.instruction("ldr x2, [sp, #8]");                                    // restore the copy length
    emitter.instruction("bl __rt_concat_publish");                              // advance _concat_off past a scratch-backed copy (x1/x2 preserved)

    // -- restore frame and return --
    emitter.instruction("ldr x19, [sp, #16]");                                  // restore the destination register
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
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
    emitter.instruction("sub rsp, 32");                                         // reserve aligned slots for the source slice and destination
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // keep the source pointer across the reservation
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // keep the source length across the reservation

    // -- reserve the destination: scratch when it fits, an owned heap string otherwise --
    emitter.instruction("mov rax, rdx");                                        // the copy needs exactly the source length
    emitter.instruction("call __rt_concat_reserve");                            // rax = destination with room for the copy
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // keep the destination start across memcpy
    emitter.instruction("mov rdi, rax");                                        // first memcpy argument = destination
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // second memcpy argument = source pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // third memcpy argument = source length
    emitter.bl_c("memcpy");                                                     // copy the whole source with the libc vectorized routine

    // -- publish the scratch offset and return the new pointer --
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the start of the copy
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // return the original string length
    emitter.instruction("call __rt_concat_publish");                            // advance _concat_off past a scratch-backed copy (rax/rdx preserved)
    emitter.instruction("add rsp, 32");                                         // release the source slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the copied string in the standard x86_64 string result registers
}

/// Emits `__rt_str_dup_owned`, which always copies a borrowed slice into a fresh owned string.
///
/// AArch64 input: `x1`/`x2` = source pointer/length; output is the same pair.
/// x86_64 input: `rax`/`rdx` = source pointer/length; output is the same pair.
/// The result is an ordinary owned string with no mbstring logical-identity metadata.
pub fn emit_str_dup_owned(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emitter.blank();
        emitter.comment("--- runtime: str_dup_owned ---");
        emitter.label_global("__rt_str_dup_owned");
        emitter.instruction("push rbp");                                        // preserve the caller frame pointer across allocation
        emitter.instruction("mov rbp, rsp");                                    // establish a stable copy frame
        emitter.instruction("sub rsp, 32");                                     // reserve the source pair and destination
        emitter.instruction("mov QWORD PTR [rbp - 8], rax");                    // save the borrowed source pointer
        emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                   // save the borrowed source length
        emitter.instruction("mov rax, rdx");                                    // request exactly the source byte count
        emitter.instruction("call __rt_str_new_owned");                         // allocate one owned untracked string
        emitter.instruction("mov QWORD PTR [rbp - 24], rax");                   // keep the destination across memcpy
        emitter.instruction("mov rdi, rax");                                    // first memcpy argument = destination
        emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                    // second memcpy argument = source pointer
        emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                   // third memcpy argument = source length
        emitter.bl_c("memcpy");                                                // copy the borrowed bytes with the libc vectorized routine
        emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                   // return the owned destination pointer
        emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                   // return the unchanged logical length
        emitter.instruction("leave");                                           // restore the caller frame pointer
        emitter.instruction("ret");                                             // transfer the owned copy to the caller
        return;
    }
    emitter.blank();
    emitter.comment("--- runtime: str_dup_owned ---");
    emitter.label_global("__rt_str_dup_owned");
    emitter.instruction("sub sp, sp, #48");                                    // reserve the source pair, destination, and linkage
    emitter.instruction("stp x29, x30, [sp, #32]");                            // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #32");                                   // establish a stable copy frame
    emitter.instruction("stp x1, x2, [sp, #0]");                               // save the borrowed source pointer and length
    emitter.instruction("mov x0, x2");                                         // request exactly the source byte count
    emitter.instruction("bl __rt_str_new_owned");                             // allocate one owned untracked string
    emitter.instruction("str x0, [sp, #16]");                                  // keep the destination across memcpy
    emitter.instruction("ldr x1, [sp, #0]");                                   // pass the borrowed source pointer
    emitter.instruction("ldr x2, [sp, #8]");                                   // pass the borrowed source length
    emitter.bl_c("memcpy");                                                     // copy the borrowed bytes with the libc vectorized routine
    emitter.instruction("ldr x1, [sp, #16]");                                  // return the owned destination pointer
    emitter.instruction("ldr x2, [sp, #8]");                                   // return the unchanged logical length
    emitter.instruction("ldp x29, x30, [sp, #32]");                            // restore the caller frame and return address
    emitter.instruction("add sp, sp, #48");                                    // release the copy frame
    emitter.instruction("ret");                                               // transfer the owned copy to the caller
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Verifies both targets copy with one libc `memcpy` rather than a byte loop.
    #[test]
    fn strcopy_emits_one_memcpy_on_all_targets() {
        for name in ["macos-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).expect("supported target");
            let mut emitter = Emitter::new(target);
            emit_strcopy(&mut emitter);
            let assembly = emitter.output();
            assert!(assembly.contains("memcpy"), "{name}: {assembly}");
            assert!(!assembly.contains("strcopy_loop"), "{name}: {assembly}");
        }
    }
}
