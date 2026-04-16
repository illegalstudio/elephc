use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

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

    // -- load array metadata --
    emitter.instruction("ldr x9, [x0]");                                        // x9 = array length
    emitter.instruction("str x9, [sp, #8]");                                    // save length to stack

    // -- Fisher-Yates: iterate i from length-1 down to 1 --
    emitter.instruction("sub x19, x9, #1");                                     // x19 = i = length - 1

    emitter.label("__rt_shuffle_loop");
    emitter.instruction("cmp x19, #1");                                         // check if i < 1
    emitter.instruction("b.lt __rt_shuffle_done");                              // if so, shuffling complete

    // -- generate random j in [0, i] --
    emitter.instruction("add x0, x19, #1");                                     // x0 = i + 1 (upper bound, exclusive)
    emitter.instruction("bl __rt_random_uniform");                              // x0 = random value in [0, i]

    // -- swap data[i] and data[j] --
    emitter.instruction("ldr x1, [sp, #0]");                                    // x1 = array pointer
    emitter.instruction("add x2, x1, #24");                                     // x2 = data base
    emitter.instruction("ldr x3, [x2, x19, lsl #3]");                           // x3 = data[i]
    emitter.instruction("ldr x4, [x2, x0, lsl #3]");                            // x4 = data[j] (x0 = j from arc4random)
    emitter.instruction("str x4, [x2, x19, lsl #3]");                           // data[i] = data[j]
    emitter.instruction("str x3, [x2, x0, lsl #3]");                            // data[j] = data[i] (complete swap)

    // -- decrement i and continue --
    emitter.instruction("sub x19, x19, #1");                                    // i -= 1
    emitter.instruction("b __rt_shuffle_loop");                                 // continue loop

    // -- tear down stack frame and return --
    emitter.label("__rt_shuffle_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

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
