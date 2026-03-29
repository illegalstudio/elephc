use crate::codegen::emit::Emitter;

/// array_merge_into_refcounted: append all elements from source array to dest array (in-place).
/// Input: x0 = dest array pointer, x1 = source array pointer
/// Both arrays must contain 8-byte refcounted payloads (array/hash/object pointers).
pub fn emit_array_merge_into_refcounted(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_merge_into_refcounted ---");
    emitter.label("__rt_array_merge_into_refcounted");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #64");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save dest array pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save source array pointer
    emitter.instruction("str xzr, [sp, #16]");                                  // initialize loop index to 0

    // -- check if source is empty --
    emitter.instruction("ldr x9, [x1]");                                        // load source array length
    emitter.instruction("cbz x9, __rt_amir_done");                              // return early when source is empty
    emitter.instruction("ldr x10, [x0, #-8]");                                  // load the destination packed array kind word
    emitter.instruction("ldr x11, [x1, #-8]");                                  // load the source packed array kind word
    emitter.instruction("and x10, x10, #0xff");                                 // keep only the destination low-byte heap kind
    emitter.instruction("and x11, x11, #0xff00");                               // keep only the source packed array value_type lane
    emitter.instruction("orr x10, x10, x11");                                   // combine the destination heap kind with the source value_type tag
    emitter.instruction("str x10, [x0, #-8]");                                  // persist the propagated packed array value_type tag

    // -- ensure dest has enough capacity --
    emitter.instruction("ldr x10, [x0]");                                       // load dest array length
    emitter.instruction("ldr x11, [x0, #8]");                                   // load dest array capacity
    emitter.instruction("add x12, x10, x9");                                    // compute needed total capacity
    emitter.label("__rt_amir_grow_check");
    emitter.instruction("cmp x12, x11");                                        // compare required capacity with current capacity
    emitter.instruction("b.le __rt_amir_loop");                                 // skip resize when dest already has enough room
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload current dest array pointer before growth
    emitter.instruction("bl __rt_array_grow");                                  // grow dest array storage until it can hold the merge result
    emitter.instruction("str x0, [sp, #0]");                                    // persist the possibly-moved dest array pointer
    emitter.instruction("ldr x11, [x0, #8]");                                   // reload dest capacity after growth
    emitter.instruction("b __rt_amir_grow_check");                              // keep growing until the required capacity fits

    emitter.label("__rt_amir_loop");
    emitter.instruction("ldr x4, [sp, #16]");                                   // reload loop index
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload source array pointer
    emitter.instruction("ldr x9, [x1]");                                        // reload source length
    emitter.instruction("cmp x4, x9");                                          // compare index with source length
    emitter.instruction("b.ge __rt_amir_set_len");                              // finish once every source element has been copied
    emitter.instruction("add x2, x1, #24");                                     // compute source data base address
    emitter.instruction("ldr x5, [x2, x4, lsl #3]");                            // load source element pointer
    emitter.instruction("str x5, [sp, #24]");                                   // save copied pointer across incref call
    emitter.instruction("mov x0, x5");                                          // move element pointer into incref argument register
    emitter.instruction("bl __rt_incref");                                      // retain borrowed heap payload before destination takes ownership
    emitter.instruction("ldr x5, [sp, #24]");                                   // restore retained pointer after incref
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload dest array pointer
    emitter.instruction("ldr x10, [x0]");                                       // reload original dest length
    emitter.instruction("add x3, x0, #24");                                     // compute dest data base address
    emitter.instruction("add x6, x10, x4");                                     // compute destination index = dest_len + loop index
    emitter.instruction("str x5, [x3, x6, lsl #3]");                            // store retained pointer into destination array
    emitter.instruction("add x4, x4, #1");                                      // increment loop index
    emitter.instruction("str x4, [sp, #16]");                                   // persist updated loop index
    emitter.instruction("b __rt_amir_loop");                                    // continue copying elements

    emitter.label("__rt_amir_set_len");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload dest array pointer
    emitter.instruction("ldr x10, [x0]");                                       // load original dest length
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload source array pointer
    emitter.instruction("ldr x9, [x1]");                                        // load source length
    emitter.instruction("add x10, x10, x9");                                    // compute new total dest length
    emitter.instruction("str x10, [x0]");                                       // store updated dest length

    emitter.label("__rt_amir_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
