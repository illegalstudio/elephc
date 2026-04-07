use crate::codegen::emit::Emitter;

/// shuffle: shuffle an integer array in place using Fisher-Yates algorithm.
/// Input: x0 = array pointer
/// Modifies array in place, no return value.
pub fn emit_shuffle(emitter: &mut Emitter) {
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
