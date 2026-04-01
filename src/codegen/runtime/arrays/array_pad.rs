use crate::codegen::emit::Emitter;

/// array_pad: pad an integer array to a specified size with a value.
/// Input: x0 = array pointer, x1 = size (negative = pad left), x2 = pad value
/// Output: x0 = pointer to new padded array
/// If abs(size) <= current length, returns a copy of the original array.
pub fn emit_array_pad(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_pad ---");
    emitter.label_global("__rt_array_pad");

    // -- set up stack frame, save arguments --
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save source array pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save size argument
    emitter.instruction("str x2, [sp, #16]");                                   // save pad value
    emitter.instruction("ldr x9, [x0]");                                        // x9 = source array length
    emitter.instruction("str x9, [sp, #24]");                                   // save source length

    // -- determine absolute size and pad direction --
    emitter.instruction("cmp x1, #0");                                          // check if size is negative
    emitter.instruction("b.ge __rt_array_pad_positive");                        // if non-negative, pad right
    emitter.instruction("neg x3, x1");                                          // x3 = abs(size) for negative case
    emitter.instruction("mov x4, #1");                                          // x4 = 1 (flag: pad left)
    emitter.instruction("b __rt_array_pad_check");                              // continue to size check

    emitter.label("__rt_array_pad_positive");
    emitter.instruction("mov x3, x1");                                          // x3 = abs(size) = size (already positive)
    emitter.instruction("mov x4, #0");                                          // x4 = 0 (flag: pad right)

    // -- check if padding is needed --
    emitter.label("__rt_array_pad_check");
    emitter.instruction("cmp x3, x9");                                          // compare abs(size) with current length
    emitter.instruction("b.le __rt_array_pad_copy");                            // if abs(size) <= length, just copy
    emitter.instruction("str x3, [sp, #32]");                                   // save abs(size) = new array size
    emitter.instruction("str x4, [sp, #40]");                                   // save pad direction flag

    // -- create new array with capacity = abs(size) --
    emitter.instruction("mov x0, x3");                                          // x0 = capacity = abs(size)
    emitter.instruction("mov x1, #8");                                          // x1 = elem_size = 8 (integers)
    emitter.instruction("bl __rt_array_new");                                   // allocate new array
    emitter.instruction("str x0, [sp, #32]");                                   // reuse slot to save new array ptr temporarily

    // -- determine pad count and data offset --
    emitter.instruction("ldr x9, [sp, #24]");                                   // x9 = source length
    emitter.instruction("ldr x4, [sp, #40]");                                   // x4 = pad direction (0=right, 1=left)
    emitter.instruction("ldr x3, [sp, #8]");                                    // x3 = original size argument
    emitter.instruction("cmp x3, #0");                                          // recheck sign for abs
    emitter.instruction("b.ge __rt_array_pad_calc_right");                      // positive = pad right
    emitter.instruction("neg x3, x3");                                          // x3 = abs(size)

    // -- pad left: fill pad values first, then copy source --
    emitter.instruction("sub x5, x3, x9");                                      // x5 = pad_count = abs(size) - length
    emitter.instruction("add x10, x0, #24");                                    // x10 = dest data base
    emitter.instruction("ldr x11, [sp, #16]");                                  // x11 = pad value
    emitter.instruction("mov x6, #0");                                          // x6 = i = 0

    emitter.label("__rt_array_pad_fill_left");
    emitter.instruction("cmp x6, x5");                                          // compare i with pad_count
    emitter.instruction("b.ge __rt_array_pad_copy_left");                       // if done padding, copy source data
    emitter.instruction("str x11, [x10, x6, lsl #3]");                          // dest[i] = pad value
    emitter.instruction("add x6, x6, #1");                                      // i += 1
    emitter.instruction("b __rt_array_pad_fill_left");                          // continue loop

    // -- copy source elements after pad values --
    emitter.label("__rt_array_pad_copy_left");
    emitter.instruction("ldr x1, [sp, #0]");                                    // x1 = source array pointer
    emitter.instruction("add x2, x1, #24");                                     // x2 = source data base
    emitter.instruction("mov x7, #0");                                          // x7 = j = 0

    emitter.label("__rt_array_pad_copy_left_loop");
    emitter.instruction("cmp x7, x9");                                          // compare j with source length
    emitter.instruction("b.ge __rt_array_pad_finish");                          // if done, finish up
    emitter.instruction("ldr x8, [x2, x7, lsl #3]");                            // x8 = source[j]
    emitter.instruction("add x12, x5, x7");                                     // x12 = pad_count + j (dest index)
    emitter.instruction("str x8, [x10, x12, lsl #3]");                          // dest[pad_count + j] = source[j]
    emitter.instruction("add x7, x7, #1");                                      // j += 1
    emitter.instruction("b __rt_array_pad_copy_left_loop");                     // continue loop

    // -- pad right: copy source first, then fill pad values --
    emitter.label("__rt_array_pad_calc_right");
    emitter.instruction("sub x5, x3, x9");                                      // x5 = pad_count = size - length
    emitter.instruction("add x10, x0, #24");                                    // x10 = dest data base
    emitter.instruction("ldr x1, [sp, #0]");                                    // x1 = source array pointer
    emitter.instruction("add x2, x1, #24");                                     // x2 = source data base
    emitter.instruction("mov x6, #0");                                          // x6 = i = 0

    emitter.label("__rt_array_pad_copy_right");
    emitter.instruction("cmp x6, x9");                                          // compare i with source length
    emitter.instruction("b.ge __rt_array_pad_fill_right_setup");                // if done copying, start padding
    emitter.instruction("ldr x8, [x2, x6, lsl #3]");                            // x8 = source[i]
    emitter.instruction("str x8, [x10, x6, lsl #3]");                           // dest[i] = source[i]
    emitter.instruction("add x6, x6, #1");                                      // i += 1
    emitter.instruction("b __rt_array_pad_copy_right");                         // continue loop

    emitter.label("__rt_array_pad_fill_right_setup");
    emitter.instruction("ldr x11, [sp, #16]");                                  // x11 = pad value
    emitter.instruction("mov x7, #0");                                          // x7 = j = 0

    emitter.label("__rt_array_pad_fill_right");
    emitter.instruction("cmp x7, x5");                                          // compare j with pad_count
    emitter.instruction("b.ge __rt_array_pad_finish");                          // if done padding, finish up
    emitter.instruction("add x12, x9, x7");                                     // x12 = length + j (dest index after source)
    emitter.instruction("str x11, [x10, x12, lsl #3]");                         // dest[length + j] = pad value
    emitter.instruction("add x7, x7, #1");                                      // j += 1
    emitter.instruction("b __rt_array_pad_fill_right");                         // continue loop

    // -- set total length and return --
    emitter.label("__rt_array_pad_finish");
    emitter.instruction("ldr x0, [sp, #32]");                                   // x0 = new array pointer
    emitter.instruction("ldr x3, [sp, #8]");                                    // x3 = original size argument
    emitter.instruction("cmp x3, #0");                                          // check sign
    emitter.instruction("cneg x3, x3, lt");                                     // x3 = abs(size)
    emitter.instruction("str x3, [x0]");                                        // set array length = abs(size)
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = padded array

    // -- no padding needed: just create a copy --
    emitter.label("__rt_array_pad_copy");
    emitter.instruction("mov x0, x9");                                          // x0 = capacity = source length
    emitter.instruction("mov x1, #8");                                          // x1 = elem_size = 8
    emitter.instruction("bl __rt_array_new");                                   // allocate new array
    emitter.instruction("ldr x1, [sp, #0]");                                    // x1 = source array pointer
    emitter.instruction("ldr x9, [sp, #24]");                                   // x9 = source length
    emitter.instruction("add x2, x1, #24");                                     // x2 = source data base
    emitter.instruction("add x3, x0, #24");                                     // x3 = dest data base
    emitter.instruction("mov x4, #0");                                          // x4 = i = 0

    emitter.label("__rt_array_pad_copy_loop");
    emitter.instruction("cmp x4, x9");                                          // compare i with source length
    emitter.instruction("b.ge __rt_array_pad_copy_done");                       // if done, finish
    emitter.instruction("ldr x5, [x2, x4, lsl #3]");                            // x5 = source[i]
    emitter.instruction("str x5, [x3, x4, lsl #3]");                            // dest[i] = source[i]
    emitter.instruction("add x4, x4, #1");                                      // i += 1
    emitter.instruction("b __rt_array_pad_copy_loop");                          // continue loop

    emitter.label("__rt_array_pad_copy_done");
    emitter.instruction("str x9, [x0]");                                        // set array length = source length
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = copied array
}
