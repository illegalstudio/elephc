use crate::codegen::emit::Emitter;

/// array_reduce: reduce an integer array to a single value using a callback.
/// Input: x0 = callback function address, x1 = source array pointer, x2 = initial value
/// Output: x0 = accumulated result
/// The callback receives (accumulator, element) and returns the new accumulator.
pub fn emit_array_reduce(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_reduce ---");
    emitter.label_global("__rt_array_reduce");

    // -- set up stack frame, save callee-saved registers --
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up new frame pointer
    emitter.instruction("stp x19, x20, [sp, #32]");                             // save callee-saved x19, x20
    emitter.instruction("str x21, [sp, #24]");                                  // save callee-saved x21
    emitter.instruction("mov x19, x0");                                         // x19 = callback address (callee-saved)
    emitter.instruction("str x1, [sp, #0]");                                    // save source array pointer to stack
    emitter.instruction("mov x21, x2");                                         // x21 = accumulator = initial value

    // -- read source array length --
    emitter.instruction("ldr x9, [x1]");                                        // x9 = source array length
    emitter.instruction("str x9, [sp, #8]");                                    // save length to stack

    // -- set up loop counter --
    emitter.instruction("mov x20, #0");                                         // x20 = loop index i = 0

    // -- loop: apply callback to accumulator and each element --
    emitter.label("__rt_array_reduce_loop");
    emitter.instruction("ldr x9, [sp, #8]");                                    // load source length
    emitter.instruction("cmp x20, x9");                                         // compare i with length
    emitter.instruction("b.ge __rt_array_reduce_done");                         // if i >= length, loop complete

    // -- load element from source array --
    emitter.instruction("ldr x2, [sp, #0]");                                    // reload source array pointer
    emitter.instruction("add x2, x2, #24");                                     // skip header to data region
    emitter.instruction("ldr x1, [x2, x20, lsl #3]");                           // x1 = source[i] (element)
    emitter.instruction("mov x0, x21");                                         // x0 = accumulator

    // -- call callback(accumulator, element) --
    emitter.instruction("blr x19");                                             // call callback → result in x0
    emitter.instruction("mov x21, x0");                                         // accumulator = callback result

    // -- advance loop --
    emitter.instruction("add x20, x20, #1");                                    // i += 1
    emitter.instruction("b __rt_array_reduce_loop");                            // continue loop

    // -- return accumulated result --
    emitter.label("__rt_array_reduce_done");
    emitter.instruction("mov x0, x21");                                         // x0 = final accumulated result

    // -- tear down stack frame and return --
    emitter.instruction("ldr x21, [sp, #24]");                                  // restore callee-saved x21
    emitter.instruction("ldp x19, x20, [sp, #32]");                             // restore callee-saved x19, x20
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = accumulated value
}
