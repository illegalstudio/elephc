use crate::codegen::emit::Emitter;

/// array_slice_refcounted: extract a slice of a refcounted array into a new array.
/// Input: x0 = array pointer, x1 = offset, x2 = length (-1 means to end)
/// Output: x0 = pointer to new sliced array
pub fn emit_array_slice_refcounted(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_slice_refcounted ---");
    emitter.label("__rt_array_slice_refcounted");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #64");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save source array pointer
    emitter.instruction("ldr x9, [x0]");                                        // load source array length
    emitter.instruction("str x9, [sp, #8]");                                    // save source length

    // -- handle negative offset: convert to positive --
    emitter.instruction("cmp x1, #0");                                          // test whether offset is negative
    emitter.instruction("b.ge __rt_array_slice_ref_pos_off");                   // skip adjustment for non-negative offsets
    emitter.instruction("add x1, x9, x1");                                      // convert negative offset into positive index
    emitter.instruction("cmp x1, #0");                                          // clamp converted offset against zero
    emitter.instruction("csel x1, xzr, x1, lt");                                // use zero when converted offset is still negative

    emitter.label("__rt_array_slice_ref_pos_off");
    emitter.instruction("cmp x1, x9");                                          // compare offset with source length
    emitter.instruction("b.ge __rt_array_slice_ref_empty");                     // return empty array when offset is out of range
    emitter.instruction("sub x3, x9, x1");                                      // compute maximum possible slice length
    emitter.instruction("cmn x2, #1");                                          // check whether requested length is -1
    emitter.instruction("csel x2, x3, x2, eq");                                 // use remaining length when caller requested -1
    emitter.instruction("cmp x2, x3");                                          // compare requested length with remaining length
    emitter.instruction("csel x2, x3, x2, gt");                                 // clamp requested length to remaining length
    emitter.instruction("str x1, [sp, #16]");                                   // save normalized offset
    emitter.instruction("str x2, [sp, #24]");                                   // save normalized slice length

    // -- create destination array --
    emitter.instruction("mov x0, x2");                                          // move slice length into destination capacity
    emitter.instruction("mov x1, #8");                                          // use 8-byte slots for heap pointers
    emitter.instruction("bl __rt_array_new");                                   // allocate destination array
    emitter.instruction("str x0, [sp, #32]");                                   // save destination array pointer

    // -- copy the requested range with retains --
    emitter.instruction("mov x6, #0");                                          // initialize loop index
    emitter.label("__rt_array_slice_ref_loop");
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload slice length
    emitter.instruction("cmp x6, x4");                                          // compare loop index with slice length
    emitter.instruction("b.ge __rt_array_slice_ref_done");                       // finish after copying every requested element
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload source array pointer
    emitter.instruction("add x2, x1, #24");                                     // compute source data base
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload normalized offset
    emitter.instruction("add x7, x3, x6");                                      // compute source index = offset + loop index
    emitter.instruction("ldr x1, [x2, x7, lsl #3]");                            // load borrowed source payload
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload destination array pointer
    emitter.instruction("bl __rt_array_push_refcounted");                       // append retained payload into destination array
    emitter.instruction("str x0, [sp, #32]");                                   // persist destination pointer after possible growth
    emitter.instruction("add x6, x6, #1");                                      // increment loop index
    emitter.instruction("b __rt_array_slice_ref_loop");                         // continue copying

    emitter.label("__rt_array_slice_ref_done");
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload destination array pointer
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return sliced array

    emitter.label("__rt_array_slice_ref_empty");
    emitter.instruction("mov x0, #0");                                          // request zero-capacity destination array
    emitter.instruction("mov x1, #8");                                          // use 8-byte slots for heap pointers
    emitter.instruction("bl __rt_array_new");                                   // allocate empty destination array
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return empty sliced array
}
