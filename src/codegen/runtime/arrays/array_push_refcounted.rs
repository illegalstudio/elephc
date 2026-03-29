use crate::codegen::emit::Emitter;

/// array_push_refcounted: push a borrowed refcounted payload into an array.
/// Input:  x0 = array pointer, x1 = borrowed heap pointer
/// Output: x0 = array pointer (may differ if array was reallocated)
pub fn emit_array_push_refcounted(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_push_refcounted ---");
    emitter.label("__rt_array_push_refcounted");

    // -- preserve arguments across incref --
    emitter.instruction("sub sp, sp, #32");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save destination array pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save borrowed heap pointer

    // -- retain borrowed payload before destination takes ownership --
    emitter.instruction("mov x0, x1");                                          // move borrowed heap pointer into incref argument register
    emitter.instruction("bl __rt_incref");                                      // retain borrowed payload for the destination array

    // -- delegate the actual append to the ordinary push helper --
    emitter.instruction("ldr x0, [sp, #0]");                                    // restore destination array pointer
    emitter.instruction("ldr x1, [sp, #8]");                                    // restore retained heap pointer
    emitter.instruction("ldr x9, [x0, #-8]");                                   // load the current packed array kind word
    emitter.instruction("ldr x10, [x1, #-8]");                                  // load the child heap kind word
    emitter.instruction("and x10, x10, #0xff");                                 // isolate the child's low-byte heap kind tag
    emitter.instruction("cmp x10, #2");                                         // is the child an indexed array?
    emitter.instruction("b.eq __rt_array_push_refcounted_kind_array");           // encode value_type 4 for nested arrays
    emitter.instruction("cmp x10, #3");                                         // is the child an associative array / hash?
    emitter.instruction("b.eq __rt_array_push_refcounted_kind_hash");            // encode value_type 5 for nested hashes
    emitter.instruction("cmp x10, #4");                                         // is the child an object instance?
    emitter.instruction("b.ne __rt_array_push_refcounted_push");                 // unexpected/non-refcounted children leave the existing tag unchanged
    emitter.instruction("mov x10, #6");                                         // encode value_type 6 for nested objects
    emitter.instruction("b __rt_array_push_refcounted_kind_store");              // store the packed array value_type tag
    emitter.label("__rt_array_push_refcounted_kind_array");
    emitter.instruction("mov x10, #4");                                         // encode value_type 4 for nested indexed arrays
    emitter.instruction("b __rt_array_push_refcounted_kind_store");              // store the packed array value_type tag
    emitter.label("__rt_array_push_refcounted_kind_hash");
    emitter.instruction("mov x10, #5");                                         // encode value_type 5 for nested associative arrays
    emitter.label("__rt_array_push_refcounted_kind_store");
    emitter.instruction("and x9, x9, #0xff");                                   // keep only the low-byte indexed-array heap kind
    emitter.instruction("lsl x10, x10, #8");                                    // move the value_type tag into the packed kind-word byte lane
    emitter.instruction("orr x9, x9, x10");                                     // combine heap kind + array value_type tag
    emitter.instruction("str x9, [x0, #-8]");                                   // persist the packed kind word on the destination array
    emitter.label("__rt_array_push_refcounted_push");
    emitter.instruction("bl __rt_array_push_int");                              // append retained heap pointer into the array

    // -- tear down stack frame and return --
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return array pointer from __rt_array_push_int
}
