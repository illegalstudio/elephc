use crate::codegen::emit::Emitter;

/// array_fill: create an array filled with a specified integer value.
/// Input: x0 = start_index (ignored for indexed arrays), x1 = count, x2 = value
/// Output: x0 = pointer to new array with count elements all set to value
pub fn emit_array_fill(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_fill ---");
    emitter.label("__rt_array_fill");

    // -- set up stack frame, save arguments --
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up new frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save count
    emitter.instruction("str x2, [sp, #8]");                                    // save fill value

    // -- create new array with capacity = count --
    emitter.instruction("mov x0, x1");                                          // x0 = capacity = count
    emitter.instruction("mov x1, #8");                                          // x1 = elem_size = 8 (integers)
    emitter.instruction("bl __rt_array_new");                                   // allocate new array
    emitter.instruction("str x0, [sp, #16]");                                   // save new array pointer

    // -- fill array with the value --
    emitter.instruction("add x3, x0, #24");                                     // x3 = data base of new array
    emitter.instruction("ldr x4, [sp, #0]");                                    // x4 = count
    emitter.instruction("ldr x5, [sp, #8]");                                    // x5 = fill value
    emitter.instruction("mov x6, #0");                                          // x6 = i = 0

    emitter.label("__rt_array_fill_loop");
    emitter.instruction("cmp x6, x4");                                          // compare i with count
    emitter.instruction("b.ge __rt_array_fill_done");                           // if i >= count, filling complete
    emitter.instruction("str x5, [x3, x6, lsl #3]");                            // data[i] = fill value
    emitter.instruction("add x6, x6, #1");                                      // i += 1
    emitter.instruction("b __rt_array_fill_loop");                              // continue loop

    // -- set length and return --
    emitter.label("__rt_array_fill_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // x0 = new array pointer
    emitter.instruction("ldr x9, [sp, #0]");                                    // x9 = count
    emitter.instruction("str x9, [x0]");                                        // set array length = count

    // -- tear down stack frame and return --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = filled array
}
