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
    emitter.instruction("bl __rt_array_push_int");                              // append retained heap pointer into the array

    // -- tear down stack frame and return --
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return array pointer from __rt_array_push_int
}
