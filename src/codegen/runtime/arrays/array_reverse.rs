use crate::codegen::emit::Emitter;

/// array_reverse: create a reversed copy of an integer array.
/// Input: x0 = array pointer
/// Output: x0 = pointer to new reversed array
pub fn emit_array_reverse(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_reverse ---");
    emitter.label("__rt_array_reverse");

    // -- set up stack frame, save source array info --
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save source array pointer
    emitter.instruction("ldr x9, [x0]");                                        // x9 = source array length
    emitter.instruction("str x9, [sp, #8]");                                    // save length to stack

    // -- create new array with same capacity --
    emitter.instruction("mov x0, x9");                                          // x0 = capacity = source length
    emitter.instruction("mov x1, #8");                                          // x1 = elem_size = 8 (integers)
    emitter.instruction("bl __rt_array_new");                                   // allocate new array, x0 = new array ptr
    emitter.instruction("str x0, [sp, #16]");                                   // save new array pointer

    // -- set up copy loop: copy elements in reverse --
    emitter.instruction("ldr x1, [sp, #0]");                                    // x1 = source array pointer
    emitter.instruction("ldr x9, [sp, #8]");                                    // x9 = length
    emitter.instruction("add x2, x1, #24");                                     // x2 = source data base
    emitter.instruction("add x3, x0, #24");                                     // x3 = dest data base
    emitter.instruction("sub x4, x9, #1");                                      // x4 = src_index = length - 1 (start from end)
    emitter.instruction("mov x5, #0");                                          // x5 = dst_index = 0

    // -- copy loop: read from end, write to start --
    emitter.label("__rt_array_reverse_loop");
    emitter.instruction("cmp x4, #0");                                          // check if src_index < 0
    emitter.instruction("b.lt __rt_array_reverse_done");                        // if so, copying is complete
    emitter.instruction("ldr x6, [x2, x4, lsl #3]");                            // x6 = source[src_index]
    emitter.instruction("str x6, [x3, x5, lsl #3]");                            // dest[dst_index] = x6
    emitter.instruction("sub x4, x4, #1");                                      // src_index -= 1
    emitter.instruction("add x5, x5, #1");                                      // dst_index += 1
    emitter.instruction("b __rt_array_reverse_loop");                           // continue loop

    // -- set length on new array and return --
    emitter.label("__rt_array_reverse_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // x0 = new array pointer
    emitter.instruction("ldr x9, [sp, #8]");                                    // x9 = length
    emitter.instruction("str x9, [x0]");                                        // set new array length = source length

    // -- tear down stack frame and return --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = new reversed array
}
