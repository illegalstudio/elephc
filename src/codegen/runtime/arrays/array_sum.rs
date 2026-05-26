//! Purpose:
//! Emits the `__rt_array_sum`, `__rt_array_sum_loop` runtime helper assembly for array sum.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - Array helpers operate on runtime array headers and element cells; mutations must respect capacity and COW contracts.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the `__rt_array_sum` runtime helper for computing the sum of all integer elements in an indexed PHP array.
///
/// Dispatches to the x86_64 Linux variant if the target is `Arch::X86_64`; otherwise emits an ARM64
/// implementation directly. Both variants assume the array pointer is in `x0`/`rdi` and return the
/// sum in `x0`/`rax`. The helper skips the 24-byte array header and iterates over scalar payload
/// slots, accumulating a 64-bit integer sum.
///
/// # Arguments
/// * `emitter` - the assembly emitter to write instructions into
pub fn emit_array_sum(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_sum_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_sum ---");
    emitter.label_global("__rt_array_sum");

    // -- set up loop variables --
    emitter.instruction("ldr x9, [x0]");                                        // x9 = array length from header
    emitter.instruction("add x10, x0, #24");                                    // x10 = base of data region (skip 24-byte header)
    emitter.instruction("mov x11, #0");                                         // x11 = i = 0 (loop counter)
    emitter.instruction("mov x12, #0");                                         // x12 = accumulator = 0

    // -- iterate and accumulate sum --
    emitter.label("__rt_array_sum_loop");
    emitter.instruction("cmp x11, x9");                                         // compare i with array length
    emitter.instruction("b.ge __rt_array_sum_done");                            // if i >= length, we're done
    emitter.instruction("ldr x13, [x10, x11, lsl #3]");                         // x13 = data[i]
    emitter.instruction("add x12, x12, x13");                                   // accumulator += data[i]
    emitter.instruction("add x11, x11, #1");                                    // i += 1
    emitter.instruction("b __rt_array_sum_loop");                               // continue loop

    // -- return the sum --
    emitter.label("__rt_array_sum_done");
    emitter.instruction("mov x0, x12");                                         // return sum in x0
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the x86_64 Linux implementation of `__rt_array_sum` using the System V AMD64 ABI.
///
/// Uses `rdi` for the array pointer, `rax` for the accumulator/return value, `rcx` as the loop
/// cursor, `r10` for the array length, and `r11` for the data region base. Skips the 24-byte
/// array header and sums 64-bit scalar payloads at `rdi + 24 + rcx * 8`.
///
/// # Arguments
/// * `emitter` - the assembly emitter to write x86_64 instructions into
fn emit_array_sum_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_sum ---");
    emitter.label_global("__rt_array_sum");

    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the source indexed-array logical length before starting the scalar sum loop
    emitter.instruction("lea r11, [rdi + 24]");                                 // compute the first scalar payload slot address in the source indexed array
    emitter.instruction("xor ecx, ecx");                                        // initialize the scalar sum loop cursor at the front of the source indexed array
    emitter.instruction("xor eax, eax");                                        // seed the scalar sum accumulator with zero before visiting any source payloads

    emitter.label("__rt_array_sum_loop_x86");
    emitter.instruction("cmp rcx, r10");                                        // compare the scalar sum loop cursor against the source indexed-array logical length
    emitter.instruction("jge __rt_array_sum_done_x86");                         // finish once every scalar payload has contributed to the sum accumulator
    emitter.instruction("add rax, QWORD PTR [r11 + rcx * 8]");                  // add the current scalar payload from the source indexed array into the running sum accumulator
    emitter.instruction("add rcx, 1");                                          // advance the scalar sum loop cursor after consuming one source payload
    emitter.instruction("jmp __rt_array_sum_loop_x86");                         // continue summing source scalar payloads until the source array is exhausted

    emitter.label("__rt_array_sum_done_x86");
    emitter.instruction("ret");                                                 // return the scalar sum accumulator in rax
}
