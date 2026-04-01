use crate::codegen::emit::Emitter;

/// array_unique: create a new array with duplicate integer values removed.
/// Input: x0 = array pointer
/// Output: x0 = pointer to new deduplicated array
/// Uses O(n^2) comparison — simple but correct for small arrays.
pub fn emit_array_unique(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_unique ---");
    emitter.label_global("__rt_array_unique");

    // -- set up stack frame, save source array --
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save source array pointer
    emitter.instruction("ldr x9, [x0]");                                        // x9 = source array length
    emitter.instruction("str x9, [sp, #8]");                                    // save source length

    // -- create new array with same capacity (worst case: all unique) --
    emitter.instruction("mov x0, x9");                                          // x0 = capacity = source length
    emitter.instruction("mov x1, #8");                                          // x1 = elem_size = 8 (integers)
    emitter.instruction("bl __rt_array_new");                                   // allocate new array
    emitter.instruction("str x0, [sp, #16]");                                   // save new array pointer

    // -- iterate source array, add each element if not already in new array --
    emitter.instruction("ldr x1, [sp, #0]");                                    // x1 = source array pointer
    emitter.instruction("ldr x9, [sp, #8]");                                    // x9 = source length
    emitter.instruction("add x2, x1, #24");                                     // x2 = source data base
    emitter.instruction("add x3, x0, #24");                                     // x3 = dest data base
    emitter.instruction("mov x4, #0");                                          // x4 = src_i = 0 (source index)
    emitter.instruction("mov x5, #0");                                          // x5 = dst_len = 0 (dest length so far)

    emitter.label("__rt_array_unique_outer");
    emitter.instruction("cmp x4, x9");                                          // compare src_i with source length
    emitter.instruction("b.ge __rt_array_unique_done");                         // if src_i >= length, we're done
    emitter.instruction("ldr x6, [x2, x4, lsl #3]");                            // x6 = source[src_i] (candidate element)

    // -- check if candidate already exists in dest array --
    emitter.instruction("mov x7, #0");                                          // x7 = check_i = 0

    emitter.label("__rt_array_unique_inner");
    emitter.instruction("cmp x7, x5");                                          // compare check_i with dest length
    emitter.instruction("b.ge __rt_array_unique_add");                          // if checked all dest, element is unique
    emitter.instruction("ldr x8, [x3, x7, lsl #3]");                            // x8 = dest[check_i]
    emitter.instruction("cmp x8, x6");                                          // compare with candidate
    emitter.instruction("b.eq __rt_array_unique_skip");                         // if equal, it's a duplicate — skip
    emitter.instruction("add x7, x7, #1");                                      // check_i += 1
    emitter.instruction("b __rt_array_unique_inner");                           // continue inner loop

    // -- element is unique, add to dest array --
    emitter.label("__rt_array_unique_add");
    emitter.instruction("str x6, [x3, x5, lsl #3]");                            // dest[dst_len] = candidate
    emitter.instruction("add x5, x5, #1");                                      // dst_len += 1

    // -- advance to next source element --
    emitter.label("__rt_array_unique_skip");
    emitter.instruction("add x4, x4, #1");                                      // src_i += 1
    emitter.instruction("b __rt_array_unique_outer");                           // continue outer loop

    // -- set final length and return --
    emitter.label("__rt_array_unique_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // x0 = new array pointer
    emitter.instruction("str x5, [x0]");                                        // set array length = number of unique elements

    // -- tear down stack frame and return --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = deduplicated array
}
