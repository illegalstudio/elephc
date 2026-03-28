use crate::codegen::emit::Emitter;

/// array_unique_refcounted: create a new array with duplicate pointer-identical refcounted values removed.
/// Input: x0 = array pointer
/// Output: x0 = pointer to new deduplicated array
pub fn emit_array_unique_refcounted(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_unique_refcounted ---");
    emitter.label("__rt_array_unique_refcounted");

    // -- set up stack frame, save source array --
    emitter.instruction("sub sp, sp, #48");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save source array pointer
    emitter.instruction("ldr x9, [x0]");                                        // load source array length
    emitter.instruction("str x9, [sp, #8]");                                    // save source array length

    // -- create destination array with worst-case capacity --
    emitter.instruction("mov x0, x9");                                          // use source length as destination capacity
    emitter.instruction("mov x1, #8");                                          // use 8-byte slots for heap pointers
    emitter.instruction("bl __rt_array_new");                                   // allocate destination array
    emitter.instruction("str x0, [sp, #16]");                                   // save destination array pointer
    emitter.instruction("mov x4, #0");                                          // initialize source index

    emitter.label("__rt_array_unique_ref_outer");
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload source array length
    emitter.instruction("cmp x4, x9");                                          // compare source index with source length
    emitter.instruction("b.ge __rt_array_unique_ref_done");                      // finish once every source element has been considered
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload source array pointer
    emitter.instruction("add x2, x0, #24");                                     // compute source data base
    emitter.instruction("ldr x6, [x2, x4, lsl #3]");                            // load candidate payload
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload destination array pointer
    emitter.instruction("ldr x5, [x0]");                                        // load current destination length
    emitter.instruction("add x3, x0, #24");                                     // compute destination data base
    emitter.instruction("mov x7, #0");                                          // initialize destination scan index

    emitter.label("__rt_array_unique_ref_inner");
    emitter.instruction("cmp x7, x5");                                          // compare scan index with destination length
    emitter.instruction("b.ge __rt_array_unique_ref_add");                       // candidate is unique when scan reaches the end
    emitter.instruction("ldr x8, [x3, x7, lsl #3]");                            // load existing destination payload
    emitter.instruction("cmp x8, x6");                                          // compare pointer identity
    emitter.instruction("b.eq __rt_array_unique_ref_skip");                      // skip candidate when it already exists in the destination
    emitter.instruction("add x7, x7, #1");                                      // increment destination scan index
    emitter.instruction("b __rt_array_unique_ref_inner");                       // continue scanning destination array

    emitter.label("__rt_array_unique_ref_add");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload destination array pointer
    emitter.instruction("mov x1, x6");                                          // move borrowed candidate payload into push argument register
    emitter.instruction("bl __rt_array_push_refcounted");                       // append retained candidate into destination array
    emitter.instruction("str x0, [sp, #16]");                                   // persist destination pointer after possible growth

    emitter.label("__rt_array_unique_ref_skip");
    emitter.instruction("add x4, x4, #1");                                      // increment source index
    emitter.instruction("b __rt_array_unique_ref_outer");                       // continue deduplicating

    emitter.label("__rt_array_unique_ref_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload destination array pointer
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return deduplicated array
}
