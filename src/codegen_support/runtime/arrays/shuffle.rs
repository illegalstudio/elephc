//! Purpose:
//! Emits the `__rt_shuffle`, `__rt_shuffle_loop` runtime helper assembly for shuffle.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Shuffle mutates array order using runtime random helpers while preserving element ownership.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// shuffle: shuffle an integer array in place using Fisher-Yates algorithm.
/// Input: x0 = array pointer
/// Modifies array in place, no return value.
pub fn emit_shuffle(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_shuffle_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: shuffle ---");
    emitter.label_global("__rt_shuffle");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #32");                                     // allocate 32 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save array pointer

    // -- Fisher-Yates: iterate i from length-1 down to 1 --
    // The cursor lives in the frame, NOT in x19: x19 is callee-saved, and this helper
    // never saved it, so a caller keeping live state there had it silently destroyed.
    emitter.instruction("ldr x9, [x0]");                                        // x9 = array length
    emitter.instruction("sub x9, x9, #1");                                      // i = length - 1
    emitter.instruction("str x9, [sp, #8]");                                    // save the Fisher-Yates cursor

    emitter.label("__rt_shuffle_loop");
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the cursor
    emitter.instruction("cmp x9, #1");                                          // check if i < 1
    emitter.instruction("b.lt __rt_shuffle_done");                              // if so, shuffling complete

    // -- generate random j in [0, i] --
    emitter.instruction("add x0, x9, #1");                                      // x0 = i + 1 (upper bound, exclusive)
    emitter.instruction("bl __rt_random_uniform");                              // x0 = random value in [0, i]

    // -- swap data[i] and data[j] --
    emitter.instruction("ldr x1, [sp, #0]");                                    // x1 = array pointer
    emitter.instruction("add x2, x1, #24");                                     // x2 = data base
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the cursor after the helper call
    emitter.instruction("ldr x3, [x2, x9, lsl #3]");                            // x3 = data[i]
    emitter.instruction("ldr x4, [x2, x0, lsl #3]");                            // x4 = data[j] (x0 = j from the random helper)
    emitter.instruction("str x4, [x2, x9, lsl #3]");                            // data[i] = data[j]
    emitter.instruction("str x3, [x2, x0, lsl #3]");                            // data[j] = data[i] (complete swap)

    // -- decrement i and continue --
    emitter.instruction("sub x9, x9, #1");                                      // i -= 1
    emitter.instruction("str x9, [sp, #8]");                                    // save the cursor
    emitter.instruction("b __rt_shuffle_loop");                                 // continue loop

    // -- tear down stack frame and return --
    emitter.label("__rt_shuffle_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

/// x86_64 Linux-specific implementation of the Fisher-Yates shuffle.
/// Input: `rdi` = array pointer (System V ABI).
/// Modifies the array payload in place; no return value.
/// Called by `emit_shuffle` when targeting Linux x86_64.
fn emit_shuffle_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: shuffle ---");
    emitter.label_global("__rt_shuffle");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving shuffle spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the array pointer and Fisher-Yates loop cursor
    emitter.instruction("sub rsp, 16");                                         // reserve aligned spill slots for the shuffled array pointer and the current descending Fisher-Yates cursor
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the shuffled indexed-array pointer across random-number helper calls
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the indexed-array logical length once before starting the Fisher-Yates loop
    emitter.instruction("cmp r10, 2");                                          // does the indexed array contain fewer than two elements?
    emitter.instruction("jb __rt_shuffle_done");                                // arrays of length zero or one are already trivially shuffled
    emitter.instruction("sub r10, 1");                                          // initialize the descending Fisher-Yates cursor to the final indexed-array slot
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // preserve the descending Fisher-Yates cursor across random-number helper calls

    emitter.label("__rt_shuffle_loop");
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the descending Fisher-Yates cursor before testing the loop termination condition
    emitter.instruction("cmp r10, 1");                                          // has the Fisher-Yates cursor reached the final swap boundary?
    emitter.instruction("jb __rt_shuffle_done");                                // stop once every slot above index zero has been swapped with a random predecessor
    emitter.instruction("lea rdi, [r10 + 1]");                                  // pass the exclusive upper bound i + 1 to the uniform random helper
    emitter.instruction("call __rt_random_uniform");                            // draw a random slot index j in the inclusive range [0, i]
    emitter.instruction("mov r11, rax");                                        // preserve the sampled Fisher-Yates partner index before reloading the array base pointer
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload the indexed-array pointer after the random helper clobbered caller-saved registers
    emitter.instruction("lea r9, [r8 + 24]");                                   // compute the indexed-array payload base pointer so the swap can address element slots directly
    emitter.instruction("mov rax, QWORD PTR [r9 + r10 * 8]");                   // load the current Fisher-Yates tail element that will be swapped toward the sampled position
    emitter.instruction("mov rdx, QWORD PTR [r9 + r11 * 8]");                   // load the sampled Fisher-Yates partner element before overwriting either slot
    emitter.instruction("mov QWORD PTR [r9 + r10 * 8], rdx");                   // store the sampled partner element into the current Fisher-Yates tail slot
    emitter.instruction("mov QWORD PTR [r9 + r11 * 8], rax");                   // store the saved tail element into the sampled Fisher-Yates partner slot
    emitter.instruction("sub r10, 1");                                          // move the descending Fisher-Yates cursor one slot left after completing the current swap
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // preserve the updated Fisher-Yates cursor for the next loop iteration
    emitter.instruction("jmp __rt_shuffle_loop");                               // continue shuffling until the descending cursor reaches the start of the indexed array

    emitter.label("__rt_shuffle_done");
    emitter.instruction("add rsp, 16");                                         // release the shuffle spill slots before returning to generated code
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning after the in-place shuffle
    emitter.instruction("ret");                                                 // return after shuffling the indexed-array payload in place
}

/// shuffle_str: Fisher-Yates over an indexed STRING array's 16-byte `(ptr, len)` slots.
///
/// The 8-byte helper would tear each descriptor in half — pairing one string's pointer with
/// another's length. Swapping moves whole descriptors; the bytes never move, so no ownership
/// changes hands. The loop state lives in the frame across the random-helper call, like the
/// scalar helper.
pub fn emit_shuffle_str(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_shuffle_str_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: shuffle_str ---");
    emitter.label_global("__rt_shuffle_str");

    // Frame: [0]=array [8]=i, linkage at [16].
    emitter.instruction("sub sp, sp, #32");
    emitter.instruction("stp x29, x30, [sp, #16]");
    emitter.instruction("add x29, sp, #16");
    emitter.instruction("str x0, [sp, #0]");                                    // the shuffled array
    emitter.instruction("ldr x9, [x0]");                                        // its length
    emitter.instruction("sub x9, x9, #1");                                      // i = length - 1
    emitter.instruction("str x9, [sp, #8]");                                    // save the Fisher-Yates cursor

    emitter.label("__rt_shuffle_str_loop");
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the cursor
    emitter.instruction("cmp x9, #1");
    emitter.instruction("b.lt __rt_shuffle_str_done");                          // every slot above zero has been swapped
    emitter.instruction("add x0, x9, #1");                                      // exclusive upper bound i + 1
    emitter.instruction("bl __rt_random_uniform");                              // x0 = random j in [0, i]
    emitter.instruction("ldr x1, [sp, #0]");
    emitter.instruction("add x2, x1, #24");                                     // the data base
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload the cursor after the helper call
    emitter.instruction("add x3, x2, x9, lsl #4");                              // &data[i] (16-byte slots)
    emitter.instruction("add x4, x2, x0, lsl #4");                              // &data[j]
    emitter.instruction("ldp x5, x6, [x3]");                                    // data[i]'s (ptr, len)
    emitter.instruction("ldp x7, x8, [x4]");                                    // data[j]'s (ptr, len)
    emitter.instruction("stp x7, x8, [x3]");                                    // data[i] = data[j]
    emitter.instruction("stp x5, x6, [x4]");                                    // data[j] = the saved descriptor
    emitter.instruction("sub x9, x9, #1");                                      // i -= 1
    emitter.instruction("str x9, [sp, #8]");
    emitter.instruction("b __rt_shuffle_str_loop");

    emitter.label("__rt_shuffle_str_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");
    emitter.instruction("add sp, sp, #32");
    emitter.instruction("ret");
}

/// Emits the x86_64 form of [`emit_shuffle_str`].
fn emit_shuffle_str_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: shuffle_str ---");
    emitter.label_global("__rt_shuffle_str");

    // Frame: [rbp-8]=array [rbp-16]=i.
    emitter.instruction("push rbp");
    emitter.instruction("mov rbp, rsp");
    emitter.instruction("sub rsp, 16");
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // the shuffled array
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // its length
    emitter.instruction("sub r10, 1");                                          // i = length - 1
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // save the Fisher-Yates cursor

    emitter.label("__rt_shuffle_str_loop_x");
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the cursor
    emitter.instruction("cmp r10, 1");
    emitter.instruction("jl __rt_shuffle_str_done_x");                          // every slot above zero has been swapped
    emitter.instruction("lea rdi, [r10 + 1]");                                  // exclusive upper bound i + 1
    emitter.instruction("call __rt_random_uniform");                            // rax = random j in [0, i]
    emitter.instruction("mov r11, rax");                                        // the sampled partner index
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");
    emitter.instruction("lea r9, [r8 + 24]");                                   // the data base
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the cursor after the helper call
    emitter.instruction("mov rcx, r10");
    emitter.instruction("shl rcx, 4");                                          // i * 16 (16-byte slots)
    emitter.instruction("lea rcx, [r9 + rcx]");                                 // &data[i]
    emitter.instruction("shl r11, 4");                                          // j * 16
    emitter.instruction("lea r11, [r9 + r11]");                                 // &data[j]
    emitter.instruction("mov rax, QWORD PTR [rcx]");                            // data[i]'s pointer
    emitter.instruction("mov rdx, QWORD PTR [rcx + 8]");                        // data[i]'s length
    emitter.instruction("mov rsi, QWORD PTR [r11]");                            // data[j]'s pointer
    emitter.instruction("mov r8, QWORD PTR [r11 + 8]");                         // data[j]'s length
    emitter.instruction("mov QWORD PTR [rcx], rsi");                            // data[i] = data[j]
    emitter.instruction("mov QWORD PTR [rcx + 8], r8");
    emitter.instruction("mov QWORD PTR [r11], rax");                            // data[j] = the saved descriptor
    emitter.instruction("mov QWORD PTR [r11 + 8], rdx");
    emitter.instruction("sub r10, 1");                                          // i -= 1
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");
    emitter.instruction("jmp __rt_shuffle_str_loop_x");

    emitter.label("__rt_shuffle_str_done_x");
    emitter.instruction("add rsp, 16");
    emitter.instruction("pop rbp");
    emitter.instruction("ret");
}
