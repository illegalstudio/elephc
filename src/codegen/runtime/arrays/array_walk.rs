use crate::codegen::emit::Emitter;

/// array_walk: call a callback on each element of an integer array (no return value).
/// Input: x0 = callback function address, x1 = source array pointer
/// Output: none (void)
pub fn emit_array_walk(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_walk ---");
    emitter.label("__rt_array_walk");

    // -- set up stack frame, save callee-saved registers --
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up new frame pointer
    emitter.instruction("stp x19, x20, [sp, #16]");                             // save callee-saved x19, x20
    emitter.instruction("mov x19, x0");                                         // x19 = callback address (callee-saved)
    emitter.instruction("str x1, [sp, #0]");                                    // save source array pointer to stack

    // -- read source array length --
    emitter.instruction("ldr x9, [x1]");                                        // x9 = source array length
    emitter.instruction("str x9, [sp, #8]");                                    // save length to stack

    // -- set up loop counter --
    emitter.instruction("mov x20, #0");                                         // x20 = loop index i = 0

    // -- loop: call callback on each element --
    emitter.label("__rt_array_walk_loop");
    emitter.instruction("ldr x9, [sp, #8]");                                    // load source length
    emitter.instruction("cmp x20, x9");                                         // compare i with length
    emitter.instruction("b.ge __rt_array_walk_done");                           // if i >= length, loop complete

    // -- load element from source array --
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload source array pointer
    emitter.instruction("add x1, x1, #24");                                     // skip header to data region
    emitter.instruction("ldr x0, [x1, x20, lsl #3]");                           // x0 = source[i]

    // -- call callback with element (ignore return value) --
    emitter.instruction("blr x19");                                             // call callback(element)

    // -- advance loop --
    emitter.instruction("add x20, x20, #1");                                    // i += 1
    emitter.instruction("b __rt_array_walk_loop");                              // continue loop

    // -- done --
    emitter.label("__rt_array_walk_done");

    // -- tear down stack frame and return --
    emitter.instruction("ldp x19, x20, [sp, #16]");                             // restore callee-saved x19, x20
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return (void)
}
