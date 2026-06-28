//! Purpose:
//! Emits the `__rt_array_multisort` runtime helper for array_multisort over two parallel arrays.
//! Stable-sorts the first indexed array ascending and applies the same element moves to the second.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - Leaf helper (no calls); in-place tandem bubble sort; scalar (8-byte) elements, equal-length arrays.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_multisort: sort arr1 ascending in place, reordering arr2 in tandem.
/// Input:  x0 = arr1 pointer (primary sort key), x1 = arr2 pointer (reordered to match)
/// Output: none (both arrays mutated in place)
///
/// Stable tandem bubble sort: each time two adjacent arr1 elements are out of ascending
/// order they are swapped together with the corresponding arr2 elements. Uses arr1 length
/// for both arrays (PHP requires equal-length arrays). Scalar 8-byte elements only.
pub fn emit_array_multisort(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_multisort_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_multisort ---");
    emitter.label_global("__rt_array_multisort");
    emitter.instruction("ldr x9, [x0]");                                        // x9 = arr1 length (used for both arrays)
    emitter.instruction("add x10, x0, #24");                                    // x10 = arr1 data base (skip header)
    emitter.instruction("add x11, x1, #24");                                    // x11 = arr2 data base (skip header)
    emitter.instruction("cmp x9, #2");                                          // arrays shorter than 2 elements are already sorted
    emitter.instruction("b.lt __rt_array_multisort_done");                      // nothing to sort
    emitter.label("__rt_array_multisort_outer");
    emitter.instruction("mov x12, #0");                                         // swapped flag = 0 for this pass
    emitter.instruction("mov x13, #0");                                         // inner index j = 0
    emitter.instruction("sub x14, x9, #1");                                     // x14 = length - 1 (last comparable index)
    emitter.label("__rt_array_multisort_inner");
    emitter.instruction("cmp x13, x14");                                        // has j reached length - 1?
    emitter.instruction("b.ge __rt_array_multisort_pass_end");                  // end of this bubble pass
    emitter.instruction("add x16, x13, #1");                                    // x16 = j + 1
    emitter.instruction("ldr x15, [x10, x13, lsl #3]");                         // x15 = arr1[j]
    emitter.instruction("ldr x17, [x10, x16, lsl #3]");                         // x17 = arr1[j+1]
    emitter.instruction("cmp x15, x17");                                        // is arr1[j] greater than arr1[j+1]?
    emitter.instruction("b.le __rt_array_multisort_no_swap");                   // already in ascending order (stable: keep equal pairs)
    emitter.instruction("str x17, [x10, x13, lsl #3]");                         // swap: arr1[j] = old arr1[j+1]
    emitter.instruction("str x15, [x10, x16, lsl #3]");                         // swap: arr1[j+1] = old arr1[j]
    emitter.instruction("ldr x15, [x11, x13, lsl #3]");                         // x15 = arr2[j]
    emitter.instruction("ldr x17, [x11, x16, lsl #3]");                         // x17 = arr2[j+1]
    emitter.instruction("str x17, [x11, x13, lsl #3]");                         // tandem swap: arr2[j] = old arr2[j+1]
    emitter.instruction("str x15, [x11, x16, lsl #3]");                         // tandem swap: arr2[j+1] = old arr2[j]
    emitter.instruction("mov x12, #1");                                         // mark that a swap happened this pass
    emitter.label("__rt_array_multisort_no_swap");
    emitter.instruction("add x13, x13, #1");                                    // advance the inner index
    emitter.instruction("b __rt_array_multisort_inner");                        // continue the bubble pass
    emitter.label("__rt_array_multisort_pass_end");
    emitter.instruction("cbnz x12, __rt_array_multisort_outer");                // repeat passes until no swaps occur
    emitter.label("__rt_array_multisort_done");
    emitter.instruction("ret");                                                 // both arrays are sorted in place
}

/// x86_64 Linux implementation of `__rt_array_multisort`.
/// Input:  rdi = arr1 pointer, rsi = arr2 pointer
/// Output: none (both arrays mutated in place)
fn emit_array_multisort_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_multisort ---");
    emitter.label_global("__rt_array_multisort");
    emitter.instruction("mov r9, QWORD PTR [rdi]");                             // r9 = arr1 length (used for both arrays)
    emitter.instruction("lea r10, [rdi + 24]");                                 // r10 = arr1 data base (skip header)
    emitter.instruction("lea r11, [rsi + 24]");                                 // r11 = arr2 data base (skip header)
    emitter.instruction("cmp r9, 2");                                           // arrays shorter than 2 elements are already sorted
    emitter.instruction("jl __rt_array_multisort_done");                        // nothing to sort
    emitter.label("__rt_array_multisort_outer");
    emitter.instruction("xor r8, r8");                                          // swapped flag = 0 for this pass
    emitter.instruction("xor rax, rax");                                        // inner index j = 0
    emitter.label("__rt_array_multisort_inner");
    emitter.instruction("mov rcx, r9");                                         // copy the length
    emitter.instruction("sub rcx, 1");                                          // rcx = length - 1 (last comparable index)
    emitter.instruction("cmp rax, rcx");                                        // has j reached length - 1?
    emitter.instruction("jge __rt_array_multisort_pass_end");                   // end of this bubble pass
    emitter.instruction("mov rcx, QWORD PTR [r10 + rax * 8]");                  // rcx = arr1[j]
    emitter.instruction("mov rdx, QWORD PTR [r10 + rax * 8 + 8]");              // rdx = arr1[j+1]
    emitter.instruction("cmp rcx, rdx");                                        // is arr1[j] greater than arr1[j+1]?
    emitter.instruction("jle __rt_array_multisort_no_swap");                    // already in ascending order (stable: keep equal pairs)
    emitter.instruction("mov QWORD PTR [r10 + rax * 8], rdx");                  // swap: arr1[j] = old arr1[j+1]
    emitter.instruction("mov QWORD PTR [r10 + rax * 8 + 8], rcx");              // swap: arr1[j+1] = old arr1[j]
    emitter.instruction("mov rcx, QWORD PTR [r11 + rax * 8]");                  // rcx = arr2[j]
    emitter.instruction("mov rdx, QWORD PTR [r11 + rax * 8 + 8]");              // rdx = arr2[j+1]
    emitter.instruction("mov QWORD PTR [r11 + rax * 8], rdx");                  // tandem swap: arr2[j] = old arr2[j+1]
    emitter.instruction("mov QWORD PTR [r11 + rax * 8 + 8], rcx");              // tandem swap: arr2[j+1] = old arr2[j]
    emitter.instruction("mov r8, 1");                                           // mark that a swap happened this pass
    emitter.label("__rt_array_multisort_no_swap");
    emitter.instruction("add rax, 1");                                          // advance the inner index
    emitter.instruction("jmp __rt_array_multisort_inner");                      // continue the bubble pass
    emitter.label("__rt_array_multisort_pass_end");
    emitter.instruction("test r8, r8");                                         // did any swap happen this pass?
    emitter.instruction("jnz __rt_array_multisort_outer");                      // repeat passes until no swaps occur
    emitter.label("__rt_array_multisort_done");
    emitter.instruction("ret");                                                 // both arrays are sorted in place
}

