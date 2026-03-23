use crate::codegen::emit::Emitter;

/// array_slice: extract a slice of an integer array into a new array.
/// Input: x0 = array pointer, x1 = offset, x2 = length (-1 means to end)
/// Output: x0 = pointer to new sliced array
/// Handles negative offset (counts from end of array).
pub fn emit_array_slice(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_slice ---");
    emitter.label("__rt_array_slice");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save source array pointer
    emitter.instruction("ldr x9, [x0]");                                        // x9 = source array length
    emitter.instruction("str x9, [sp, #8]");                                    // save source length

    // -- handle negative offset: convert to positive --
    emitter.instruction("cmp x1, #0");                                          // check if offset is negative
    emitter.instruction("b.ge __rt_array_slice_pos_off");                       // if non-negative, skip adjustment
    emitter.instruction("add x1, x9, x1");                                      // offset = length + offset (e.g., -2 → length-2)
    emitter.instruction("cmp x1, #0");                                          // clamp to 0 if still negative
    emitter.instruction("csel x1, xzr, x1, lt");                               // if offset < 0, set to 0

    // -- compute actual slice length --
    emitter.label("__rt_array_slice_pos_off");
    emitter.instruction("cmp x1, x9");                                          // check if offset >= array length
    emitter.instruction("b.ge __rt_array_slice_empty");                         // if so, result is empty array
    emitter.instruction("sub x3, x9, x1");                                      // x3 = max possible length = array_len - offset
    emitter.instruction("cmn x2, #1");                                          // check if length == -1 (to end)
    emitter.instruction("csel x2, x3, x2, eq");                                // if length == -1, use remaining length
    emitter.instruction("cmp x2, x3");                                          // clamp length to max possible
    emitter.instruction("csel x2, x3, x2, gt");                                // if length > remaining, use remaining
    emitter.instruction("str x1, [sp, #16]");                                   // save computed offset
    emitter.instruction("str x2, [sp, #24]");                                   // save computed slice length

    // -- create new array --
    emitter.instruction("mov x0, x2");                                          // x0 = capacity = slice length
    emitter.instruction("mov x1, #8");                                          // x1 = elem_size = 8 (integers)
    emitter.instruction("bl __rt_array_new");                                   // allocate new array
    emitter.instruction("str x0, [sp, #32]");                                   // save new array pointer

    // -- copy slice elements --
    emitter.instruction("ldr x1, [sp, #0]");                                    // x1 = source array pointer
    emitter.instruction("add x2, x1, #24");                                     // x2 = source data base
    emitter.instruction("ldr x3, [sp, #16]");                                   // x3 = offset
    emitter.instruction("ldr x4, [sp, #24]");                                   // x4 = slice length
    emitter.instruction("add x5, x0, #24");                                     // x5 = dest data base
    emitter.instruction("mov x6, #0");                                          // x6 = i = 0

    emitter.label("__rt_array_slice_copy");
    emitter.instruction("cmp x6, x4");                                          // compare i with slice length
    emitter.instruction("b.ge __rt_array_slice_done");                          // if done, finish up
    emitter.instruction("add x7, x3, x6");                                      // x7 = offset + i (source index)
    emitter.instruction("ldr x8, [x2, x7, lsl #3]");                            // x8 = source[offset + i]
    emitter.instruction("str x8, [x5, x6, lsl #3]");                            // dest[i] = source[offset + i]
    emitter.instruction("add x6, x6, #1");                                      // i += 1
    emitter.instruction("b __rt_array_slice_copy");                             // continue loop

    // -- set length and return --
    emitter.label("__rt_array_slice_done");
    emitter.instruction("ldr x0, [sp, #32]");                                   // x0 = new array pointer
    emitter.instruction("ldr x9, [sp, #24]");                                   // x9 = slice length
    emitter.instruction("str x9, [x0]");                                        // set new array length
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = sliced array

    // -- empty result: offset was beyond array bounds --
    emitter.label("__rt_array_slice_empty");
    emitter.instruction("mov x0, #0");                                          // x0 = capacity = 0
    emitter.instruction("mov x1, #8");                                          // x1 = elem_size = 8
    emitter.instruction("bl __rt_array_new");                                   // allocate empty array
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = empty array
}
