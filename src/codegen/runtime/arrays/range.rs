use crate::codegen::emit::Emitter;

/// range: create an integer array from start to end (inclusive).
/// Input: x0 = start, x1 = end
/// Output: x0 = pointer to new array containing values from start to end
/// Supports both ascending (start <= end) and descending (start > end) ranges.
pub fn emit_range(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: range ---");
    emitter.label("__rt_range");

    // -- set up stack frame, save arguments --
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save start value
    emitter.instruction("str x1, [sp, #8]");                                    // save end value

    // -- determine direction and calculate count --
    emitter.instruction("cmp x0, x1");                                          // compare start with end
    emitter.instruction("b.gt __rt_range_descending");                          // if start > end, use descending path

    // -- ascending: count = end - start + 1 --
    emitter.instruction("sub x2, x1, x0");                                      // x2 = end - start
    emitter.instruction("add x2, x2, #1");                                      // x2 = count = end - start + 1
    emitter.instruction("mov x7, #1");                                          // x7 = step = +1 (ascending)
    emitter.instruction("b __rt_range_alloc");                                  // jump to allocation

    // -- descending: count = start - end + 1 --
    emitter.label("__rt_range_descending");
    emitter.instruction("sub x2, x0, x1");                                      // x2 = start - end
    emitter.instruction("add x2, x2, #1");                                      // x2 = count = start - end + 1
    emitter.instruction("mov x7, #-1");                                         // x7 = step = -1 (descending)

    // -- allocate array --
    emitter.label("__rt_range_alloc");
    emitter.instruction("str x2, [sp, #16]");                                   // save count
    emitter.instruction("str x7, [sp, #8]");                                    // save step (reuse end slot, no longer needed)
    emitter.instruction("mov x0, x2");                                          // x0 = capacity = count
    emitter.instruction("mov x1, #8");                                          // x1 = elem_size = 8 (integers)
    emitter.instruction("bl __rt_array_new");                                   // allocate new array
    emitter.instruction("str x0, [sp, #24]");                                   // save new array pointer

    // -- fill array with values from start, stepping by +1 or -1 --
    emitter.instruction("add x3, x0, #24");                                     // x3 = data base of new array
    emitter.instruction("ldr x4, [sp, #0]");                                    // x4 = current value = start
    emitter.instruction("ldr x5, [sp, #16]");                                   // x5 = count
    emitter.instruction("ldr x7, [sp, #8]");                                    // x7 = step (+1 or -1)
    emitter.instruction("mov x6, #0");                                          // x6 = i = 0

    emitter.label("__rt_range_loop");
    emitter.instruction("cmp x6, x5");                                          // compare i with count
    emitter.instruction("b.ge __rt_range_done");                                // if i >= count, filling complete
    emitter.instruction("str x4, [x3, x6, lsl #3]");                            // data[i] = current value
    emitter.instruction("add x4, x4, x7");                                      // current value += step (+1 or -1)
    emitter.instruction("add x6, x6, #1");                                      // i += 1
    emitter.instruction("b __rt_range_loop");                                   // continue loop

    // -- set length and return --
    emitter.label("__rt_range_done");
    emitter.instruction("ldr x0, [sp, #24]");                                   // x0 = new array pointer
    emitter.instruction("ldr x9, [sp, #16]");                                   // x9 = count
    emitter.instruction("str x9, [x0]");                                        // set array length = count

    // -- tear down stack frame and return --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = array [start..end]
}
