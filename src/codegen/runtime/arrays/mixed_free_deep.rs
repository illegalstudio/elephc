use crate::codegen::emit::Emitter;

/// mixed_free_deep: free a mixed cell and release its owned child payload.
/// Input: x0 = mixed cell pointer
/// Output: none
pub fn emit_mixed_free_deep(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_free_deep ---");
    emitter.label_global("__rt_mixed_free_deep");

    emitter.instruction("cbz x0, __rt_mixed_free_deep_done");                   // skip null mixed cells immediately
    emitter.instruction("sub sp, sp, #32");                                     // allocate a small frame to preserve the mixed pointer
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // set up the new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the mixed pointer across child release
    emitter.instruction("ldr x9, [x0]");                                        // load the boxed runtime value_tag
    emitter.instruction("cmp x9, #1");                                          // is the boxed payload a string?
    emitter.instruction("b.eq __rt_mixed_free_deep_string");                    // strings release through heap_free_safe
    emitter.instruction("cmp x9, #4");                                          // does the boxed payload hold a heap-backed child?
    emitter.instruction("b.lo __rt_mixed_free_deep_box");                       // scalars/bools/floats/null need no nested release
    emitter.instruction("cmp x9, #7");                                          // do boxed heap-backed tags stay within the supported range?
    emitter.instruction("b.hi __rt_mixed_free_deep_box");                       // unknown tags are ignored by mixed deep-free
    emitter.instruction("ldr x0, [x0, #8]");                                    // load the boxed heap child pointer
    emitter.instruction("bl __rt_decref_any");                                  // release the boxed child through the uniform dispatcher
    emitter.instruction("b __rt_mixed_free_deep_box");                          // free the mixed cell storage after releasing the child

    emitter.label("__rt_mixed_free_deep_string");
    emitter.instruction("ldr x0, [x0, #8]");                                    // load the boxed string pointer
    emitter.instruction("bl __rt_heap_free_safe");                              // release the boxed string payload

    emitter.label("__rt_mixed_free_deep_box");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the mixed pointer after child release
    emitter.instruction("bl __rt_heap_free");                                   // free the mixed cell storage itself
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate the mixed-free frame

    emitter.label("__rt_mixed_free_deep_done");
    emitter.instruction("ret");                                                 // return to caller
}
