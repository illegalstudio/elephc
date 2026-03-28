use crate::codegen::emit::Emitter;

/// array_splice_refcounted: remove a portion of a refcounted array and return removed elements.
/// Input:  x0=array_ptr, x1=offset, x2=length
/// Output: x0=new array containing retained removed elements
pub fn emit_array_splice_refcounted(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_splice_refcounted ---");
    emitter.label("__rt_array_splice_refcounted");

    // -- set up stack frame, save arguments --
    emitter.instruction("sub sp, sp, #48");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save source array pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save offset
    emitter.instruction("str x2, [sp, #16]");                                   // save removal length

    // -- clamp removal length to not exceed array bounds --
    emitter.instruction("ldr x3, [x0]");                                        // load source array length
    emitter.instruction("sub x4, x3, x1");                                      // compute maximum removable length
    emitter.instruction("cmp x2, x4");                                          // compare requested length with maximum removable length
    emitter.instruction("csel x2, x4, x2, gt");                                 // clamp length to the remaining number of elements
    emitter.instruction("str x2, [sp, #16]");                                   // save clamped removal length

    // -- create result array for removed elements --
    emitter.instruction("mov x0, x2");                                          // use removal length as result capacity
    emitter.instruction("mov x1, #8");                                          // use 8-byte slots for heap pointers
    emitter.instruction("bl __rt_array_new");                                   // allocate result array
    emitter.instruction("str x0, [sp, #24]");                                   // save result array pointer

    // -- copy removed elements into the result with retains --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload source array pointer
    emitter.instruction("add x5, x0, #24");                                     // compute source data base
    emitter.instruction("ldr x6, [sp, #8]");                                    // reload offset
    emitter.instruction("ldr x7, [sp, #16]");                                   // reload removal length
    emitter.instruction("mov x8, #0");                                          // initialize copy-loop index
    emitter.label("__rt_array_splice_ref_copy");
    emitter.instruction("cmp x8, x7");                                          // compare copy index with removal length
    emitter.instruction("b.ge __rt_array_splice_ref_shift");                     // move on to in-place shifting after copying removed elements
    emitter.instruction("add x9, x6, x8");                                      // compute source index = offset + copy index
    emitter.instruction("ldr x1, [x5, x9, lsl #3]");                            // load borrowed removed payload
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload result array pointer
    emitter.instruction("bl __rt_array_push_refcounted");                       // append retained removed payload into result array
    emitter.instruction("str x0, [sp, #24]");                                   // persist result pointer after possible growth
    emitter.instruction("add x8, x8, #1");                                      // increment copy-loop index
    emitter.instruction("b __rt_array_splice_ref_copy");                        // continue copying removed elements

    // -- shift remaining elements left inside the source array --
    emitter.label("__rt_array_splice_ref_shift");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload source array pointer
    emitter.instruction("ldr x3, [x0]");                                        // reload original source length
    emitter.instruction("add x5, x0, #24");                                     // compute source data base
    emitter.instruction("ldr x6, [sp, #8]");                                    // reload offset as destination start
    emitter.instruction("ldr x7, [sp, #16]");                                   // reload removal length
    emitter.instruction("add x8, x6, x7");                                      // initialize source read index
    emitter.label("__rt_array_splice_ref_shift_loop");
    emitter.instruction("cmp x8, x3");                                          // compare source read index with original length
    emitter.instruction("b.ge __rt_array_splice_ref_update");                    // stop shifting after exhausting the tail segment
    emitter.instruction("ldr x9, [x5, x8, lsl #3]");                            // load tail payload
    emitter.instruction("str x9, [x5, x6, lsl #3]");                            // move tail payload left in-place
    emitter.instruction("add x6, x6, #1");                                      // increment destination write index
    emitter.instruction("add x8, x8, #1");                                      // increment source read index
    emitter.instruction("b __rt_array_splice_ref_shift_loop");                  // continue shifting

    emitter.label("__rt_array_splice_ref_update");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload source array pointer
    emitter.instruction("ldr x3, [x0]");                                        // reload original source length
    emitter.instruction("ldr x7, [sp, #16]");                                   // reload removal length
    emitter.instruction("sub x3, x3, x7");                                      // compute new source length
    emitter.instruction("str x3, [x0]");                                        // store new source length

    // -- return removed-elements result array --
    emitter.instruction("ldr x0, [sp, #24]");                                   // reload result array pointer
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return result array
}
