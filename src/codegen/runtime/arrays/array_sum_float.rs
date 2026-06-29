//! Purpose:
//! Emits the `__rt_array_sum_float` runtime helper assembly for floating-point array sum.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - Used when `array_sum()` is applied to a `float[]`. Slots hold raw IEEE doubles, so the sum must
//!   accumulate with floating-point adds (`fadd`/`addsd`) and return in the float result register
//!   (`d0`/`xmm0`), unlike the integer `__rt_array_sum` which adds raw 64-bit words.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the `__rt_array_sum_float` runtime helper for summing all double elements of an indexed array.
///
/// Dispatches to the x86_64 Linux variant if the target is `Arch::X86_64`; otherwise emits an ARM64
/// implementation. Both assume the array pointer is in `x0`/`rdi` and return the sum in the float
/// result register `d0`/`xmm0`. The helper skips the 24-byte array header and iterates over 8-byte
/// double payload slots, accumulating a floating-point sum (0.0 for an empty array).
///
/// # Arguments
/// * `emitter` - the assembly emitter to write instructions into
pub fn emit_array_sum_float(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_sum_float_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_sum_float ---");
    emitter.label_global("__rt_array_sum_float");

    // -- set up loop variables --
    emitter.instruction("ldr x9, [x0]");                                        // x9 = array length from header
    emitter.instruction("add x10, x0, #24");                                    // x10 = base of data region (skip 24-byte header)
    emitter.instruction("mov x11, #0");                                         // x11 = i = 0 (loop counter)
    emitter.instruction("movi d0, #0");                                         // d0 = accumulator = 0.0 (float result register)

    // -- iterate and accumulate floating-point sum --
    emitter.label("__rt_array_sum_float_loop");
    emitter.instruction("cmp x11, x9");                                         // compare i with array length
    emitter.instruction("b.ge __rt_array_sum_float_done");                      // if i >= length, we're done
    emitter.instruction("ldr d1, [x10, x11, lsl #3]");                          // d1 = data[i] as a double
    emitter.instruction("fadd d0, d0, d1");                                     // accumulator += data[i] (floating-point add)
    emitter.instruction("add x11, x11, #1");                                    // i += 1
    emitter.instruction("b __rt_array_sum_float_loop");                         // continue loop

    // -- return the sum (already in d0) --
    emitter.label("__rt_array_sum_float_done");
    emitter.instruction("ret");                                                 // return to caller with sum in d0
}

/// Emits the x86_64 Linux implementation of `__rt_array_sum_float` using the System V AMD64 ABI.
///
/// Uses `rdi` for the array pointer, `xmm0` for the accumulator/return value, `rcx` as the loop
/// cursor, `r10` for the array length, and `r11` for the data region base. Skips the 24-byte
/// array header and sums 8-byte double payloads at `rdi + 24 + rcx * 8` with `addsd`.
///
/// # Arguments
/// * `emitter` - the assembly emitter to write x86_64 instructions into
fn emit_array_sum_float_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_sum_float ---");
    emitter.label_global("__rt_array_sum_float");

    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the source indexed-array logical length before starting the double sum loop
    emitter.instruction("lea r11, [rdi + 24]");                                 // compute the first double payload slot address in the source indexed array
    emitter.instruction("xor ecx, ecx");                                        // initialize the double sum loop cursor at the front of the source indexed array
    emitter.instruction("xorps xmm0, xmm0");                                    // seed the double sum accumulator with 0.0 before visiting any source payloads

    emitter.label("__rt_array_sum_float_loop_x86");
    emitter.instruction("cmp rcx, r10");                                        // compare the double sum loop cursor against the source indexed-array logical length
    emitter.instruction("jge __rt_array_sum_float_done_x86");                   // finish once every double payload has contributed to the sum accumulator
    emitter.instruction("addsd xmm0, QWORD PTR [r11 + rcx * 8]");               // add the current double payload from the source indexed array into the running sum accumulator
    emitter.instruction("add rcx, 1");                                          // advance the double sum loop cursor after consuming one source payload
    emitter.instruction("jmp __rt_array_sum_float_loop_x86");                   // continue summing source double payloads until the source array is exhausted

    emitter.label("__rt_array_sum_float_done_x86");
    emitter.instruction("ret");                                                 // return the double sum accumulator in xmm0
}
