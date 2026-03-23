use crate::codegen::emit::Emitter;

/// array_splice: remove a portion of an array and return removed elements.
/// Input:  x0=array_ptr, x1=offset, x2=length (number of elements to remove)
/// Output: x0=new array containing removed elements
/// The original array is modified in-place (remaining elements shifted left).
pub fn emit_array_splice(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_splice ---");
    emitter.label("__rt_array_splice");

    // -- set up stack frame, save arguments --
    // Stack layout:
    //   [sp, #0]  = source array pointer
    //   [sp, #8]  = offset
    //   [sp, #16] = removal length
    //   [sp, #24] = result array pointer
    //   [sp, #32] = saved x29
    //   [sp, #40] = saved x30
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save source array pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save offset
    emitter.instruction("str x2, [sp, #16]");                                   // save removal length

    // -- clamp removal length to not exceed array bounds --
    emitter.instruction("ldr x3, [x0]");                                        // x3 = source array length
    emitter.instruction("sub x4, x3, x1");                                      // x4 = length - offset (max removable)
    emitter.instruction("cmp x2, x4");                                          // compare requested length with max
    emitter.instruction("csel x2, x4, x2, gt");                                 // clamp to max if too large
    emitter.instruction("str x2, [sp, #16]");                                   // save clamped removal length

    // -- create result array for removed elements --
    emitter.instruction("mov x0, x2");                                          // x0 = capacity = removal length
    emitter.instruction("mov x1, #8");                                          // x1 = elem_size (8 bytes per int)
    emitter.instruction("bl __rt_array_new");                                   // create result array, x0 = result ptr
    emitter.instruction("str x0, [sp, #24]");                                   // save result array pointer

    // -- copy removed elements to result array --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload source array pointer
    emitter.instruction("add x5, x0, #24");                                     // x5 = source data base
    emitter.instruction("ldr x6, [sp, #8]");                                    // x6 = offset
    emitter.instruction("ldr x7, [sp, #16]");                                   // x7 = removal length
    emitter.instruction("mov x8, #0");                                          // x8 = j = 0

    emitter.label("__rt_array_splice_copy");
    emitter.instruction("cmp x8, x7");                                          // compare j with removal length
    emitter.instruction("b.ge __rt_array_splice_shift");                        // if j >= length, start shifting

    emitter.instruction("add x9, x6, x8");                                      // x9 = offset + j (source index)
    emitter.instruction("ldr x1, [x5, x9, lsl #3]");                            // x1 = source[offset + j]
    emitter.instruction("ldr x0, [sp, #24]");                                   // x0 = result array pointer
    emitter.instruction("bl __rt_array_push_int");                              // push to result array

    emitter.instruction("add x8, x8, #1");                                      // j += 1
    emitter.instruction("b __rt_array_splice_copy");                            // continue copying

    // -- shift remaining elements left to fill the gap --
    emitter.label("__rt_array_splice_shift");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload source array pointer
    emitter.instruction("ldr x3, [x0]");                                        // x3 = original source length
    emitter.instruction("add x5, x0, #24");                                     // x5 = source data base
    emitter.instruction("ldr x6, [sp, #8]");                                    // x6 = offset (destination start)
    emitter.instruction("ldr x7, [sp, #16]");                                   // x7 = removal length
    emitter.instruction("add x8, x6, x7");                                      // x8 = offset + removal_length (source start)

    emitter.label("__rt_array_splice_shift_loop");
    emitter.instruction("cmp x8, x3");                                          // compare source index with array length
    emitter.instruction("b.ge __rt_array_splice_update");                       // if past end, update length

    emitter.instruction("ldr x9, [x5, x8, lsl #3]");                            // x9 = source[source_idx]
    emitter.instruction("str x9, [x5, x6, lsl #3]");                            // source[dest_idx] = source[source_idx]
    emitter.instruction("add x6, x6, #1");                                      // dest_idx += 1
    emitter.instruction("add x8, x8, #1");                                      // source_idx += 1
    emitter.instruction("b __rt_array_splice_shift_loop");                      // continue shifting

    // -- update source array length --
    emitter.label("__rt_array_splice_update");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload source array pointer
    emitter.instruction("ldr x3, [x0]");                                        // x3 = original length
    emitter.instruction("ldr x7, [sp, #16]");                                   // x7 = removal length
    emitter.instruction("sub x3, x3, x7");                                      // x3 = new length
    emitter.instruction("str x3, [x0]");                                        // store new length in header

    // -- return result array --
    emitter.instruction("ldr x0, [sp, #24]");                                   // x0 = result array pointer
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = removed elements array
}
