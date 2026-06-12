//! Purpose:
//! Emits the `__rt_array_fill_str` runtime helper assembly: builds an indexed array filled with
//! `count` copies of a string value by repeatedly invoking `__rt_array_push_str`.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - The string payload is `(ptr, len)`, not a single heap pointer, so the refcounted-fill path
//!   cannot be reused; each iteration persists the string to heap through `__rt_array_push_str`.
//! - The loop count is decremented on the stack so the loop survives any registers the append
//!   helper clobbers. The `start_index` argument is ignored (the array is always 0-indexed),
//!   matching the scalar/refcounted fill helpers.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits `__rt_array_fill_str`: create an indexed array of `count` copies of a string value.
///
/// Input (AArch64): `x0` = count, `x1` = string ptr, `x2` = string len.
/// Input (x86_64): `rdi` = count, `rsi` = string ptr, `rdx` = string len.
/// Output: the new array pointer in the integer result register (`x0` / `rax`).
///
/// Each element is appended via `__rt_array_push_str`, which persists the string to the heap and
/// stamps the indexed-array string metadata. The count is held on the stack and decremented each
/// iteration so the loop is robust against registers clobbered by the append helper.
pub fn emit_array_fill_str(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_fill_str_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_fill_str ---");
    emitter.label_global("__rt_array_fill_str");

    // -- set up stack frame, save count and string payload --
    emitter.instruction("sub sp, sp, #48");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save remaining element count
    emitter.instruction("str x1, [sp, #8]");                                    // save borrowed string pointer
    emitter.instruction("str x2, [sp, #16]");                                   // save borrowed string length

    // -- create destination array (capacity = count, 16-byte string slots: ptr + len) --
    emitter.instruction("mov x1, #16");                                         // string arrays use 16-byte slots (pointer + length)
    emitter.instruction("bl __rt_array_new");                                   // allocate destination array (x0 still holds count as capacity)
    emitter.instruction("str x0, [sp, #24]");                                   // save destination array pointer

    // -- append count persisted copies of the string --
    emitter.label("__rt_array_fill_str_loop");
    emitter.instruction("ldr x4, [sp, #0]");                                    // reload remaining count
    emitter.instruction("cbz x4, __rt_array_fill_str_done");                    // finish once the requested count is reached
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload destination array pointer
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload borrowed string pointer
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload borrowed string length
    emitter.instruction("bl __rt_array_push_str");                              // persist and append one string copy
    emitter.instruction("str x0, [sp, #24]");                                   // persist destination pointer after possible growth
    emitter.instruction("ldr x4, [sp, #0]");                                    // reload remaining count (append may clobber x4)
    emitter.instruction("sub x4, x4, #1");                                      // consume one element from the remaining count
    emitter.instruction("str x4, [sp, #0]");                                    // store the decremented remaining count
    emitter.instruction("b __rt_array_fill_str_loop");                          // continue filling

    emitter.label("__rt_array_fill_str_done");
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload destination array pointer
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return filled array
}

/// x86_64 Linux variant of [`emit_array_fill_str`].
///
/// Input: `rdi` = count, `rsi` = string ptr, `rdx` = string len.
/// Output: `rax` = pointer to the new array with `count` persisted copies of the string.
fn emit_array_fill_str_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_fill_str ---");
    emitter.label_global("__rt_array_fill_str");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving fill spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for count, string payload, and destination bookkeeping
    emitter.instruction("sub rsp, 32");                                         // reserve aligned spill slots for count, string ptr, string len, and destination array
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the remaining element count across allocation and append helper calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the borrowed string pointer across allocation and append helper calls
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // preserve the borrowed string length across allocation and append helper calls
    emitter.instruction("mov rsi, 16");                                         // string arrays use 16-byte slots (pointer + length); rdi still holds count as capacity
    emitter.instruction("call __rt_array_new");                                 // allocate the destination indexed array through the shared constructor
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve the destination indexed-array pointer across repeated append helper calls
    emitter.label("__rt_array_fill_str_loop_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the remaining element count
    emitter.instruction("test rax, rax");                                       // test whether any elements remain to be appended
    emitter.instruction("jle __rt_array_fill_str_done_x86");                    // stop once the destination contains the requested number of strings
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the current destination indexed-array pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the borrowed string pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // reload the borrowed string length
    emitter.instruction("call __rt_array_push_str");                            // persist and append one string copy
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // persist the possibly-grown destination indexed-array pointer
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the remaining element count (append may clobber it)
    emitter.instruction("sub rax, 1");                                          // consume one element from the remaining count
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // store the decremented remaining count
    emitter.instruction("jmp __rt_array_fill_str_loop_x86");                    // continue filling until the destination reaches the requested length
    emitter.label("__rt_array_fill_str_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // return the filled destination indexed-array pointer
    emitter.instruction("add rsp, 32");                                         // release the fill spill slots before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                 // return the filled destination indexed-array pointer in rax
}
