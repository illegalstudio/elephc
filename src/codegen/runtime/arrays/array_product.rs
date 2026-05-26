//! Purpose:
//! Emits the `__rt_array_product`, `__rt_array_product_loop` runtime helper assembly for array product.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - Array helpers operate on runtime array headers and element cells; mutations must respect capacity and COW contracts.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the `__rt_array_product` runtime helper that computes the product of all
/// elements in a PHP array.
///
/// - **Input**: `x0` = pointer to the runtime array header (ARM64) / `rdi` = array pointer (x86_64)
/// - **Output**: `x0` = product of all elements (ARM64) / `rax` = product (x86_64)
/// - **Empty array**: returns 1 (multiplicative identity) when the array has no elements
/// - **ABI**: uses `x9`–`x12` as scratch registers on ARM64; `r8`, `r10`, `r11`, `rcx` on x86_64
/// - **Behavior**: iterates the array's data region, multiplying each element's raw value into
///   an accumulator seeded with 1. No type checking is performed; the caller is responsible
///   for ensuring the array contains integer/float values.
///
/// On x86_64 targets, dispatches to `emit_array_product_linux_x86_64`. ARM64 emits the helper
/// inline using a compare-and-branch loop.
pub fn emit_array_product(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_product_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_product ---");
    emitter.label_global("__rt_array_product");

    // -- set up loop variables --
    emitter.instruction("ldr x9, [x0]");                                        // x9 = array length from header
    emitter.instruction("add x10, x0, #24");                                    // x10 = base of data region (skip 24-byte header)
    emitter.instruction("mov x11, #0");                                         // x11 = i = 0 (loop counter)
    emitter.instruction("mov x12, #1");                                         // x12 = accumulator = 1 (multiplicative identity)

    // -- iterate and accumulate product --
    emitter.label("__rt_array_product_loop");
    emitter.instruction("cmp x11, x9");                                         // compare i with array length
    emitter.instruction("b.ge __rt_array_product_done");                        // if i >= length, we're done
    emitter.instruction("ldr x13, [x10, x11, lsl #3]");                         // x13 = data[i]
    emitter.instruction("mul x12, x12, x13");                                   // accumulator *= data[i]
    emitter.instruction("add x11, x11, #1");                                    // i += 1
    emitter.instruction("b __rt_array_product_loop");                           // continue loop

    // -- return the product --
    emitter.label("__rt_array_product_done");
    emitter.instruction("mov x0, x12");                                         // return product in x0
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the x86_64 Linux variant of `__rt_array_product` using the System V AMD64 ABI.
/// Reads the array length from `[rdi]` (first QWORD of the header), iterates from the data
/// region at `rdi + 24`, and returns the product in `rax`. Uses `rcx` as the loop counter,
/// `rax` as the accumulator (seeded to 1), and `r8` as a temporary load slot.
fn emit_array_product_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_product ---");
    emitter.label_global("__rt_array_product");

    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the source indexed-array logical length before starting the scalar product loop
    emitter.instruction("lea r11, [rdi + 24]");                                 // compute the first scalar payload slot address in the source indexed array
    emitter.instruction("xor ecx, ecx");                                        // initialize the scalar product loop cursor at the front of the source indexed array
    emitter.instruction("mov rax, 1");                                          // seed the scalar product accumulator with the multiplicative identity

    emitter.label("__rt_array_product_loop_x86");
    emitter.instruction("cmp rcx, r10");                                        // compare the scalar product loop cursor against the source indexed-array logical length
    emitter.instruction("jge __rt_array_product_done_x86");                     // finish once every scalar payload has contributed to the product accumulator
    emitter.instruction("mov r8, QWORD PTR [r11 + rcx * 8]");                   // load the current scalar payload from the source indexed array
    emitter.instruction("imul rax, r8");                                        // multiply the running scalar product accumulator by the current source payload
    emitter.instruction("add rcx, 1");                                          // advance the scalar product loop cursor after consuming one source payload
    emitter.instruction("jmp __rt_array_product_loop_x86");                     // continue multiplying source scalar payloads until the source array is exhausted

    emitter.label("__rt_array_product_done_x86");
    emitter.instruction("ret");                                                 // return the scalar product accumulator in rax
}
