use crate::codegen::emit::Emitter;

pub fn emit_exception_cleanup_frames(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: exception_cleanup_frames ---");
    emitter.label("__rt_exception_cleanup_frames");

    // -- save callee-saved state used by the cleanup walk --
    emitter.instruction("sub sp, sp, #48");                                      // reserve stack space for x19/x20 plus frame linkage
    emitter.instruction("stp x29, x30, [sp, #32]");                              // save frame pointer and return address for the cleanup walker
    emitter.instruction("stp x19, x20, [sp, #16]");                              // preserve callee-saved registers that track the walk state
    emitter.instruction("add x29, sp, #32");                                     // install the cleanup walker's frame pointer
    emitter.instruction("mov x19, x0");                                          // x19 = activation record that should remain on top after unwinding
    emitter.instruction("adrp x9, _exc_call_frame_top@PAGE");                    // load page of the call-frame stack top
    emitter.instruction("add x9, x9, _exc_call_frame_top@PAGEOFF");              // resolve the call-frame stack top address
    emitter.instruction("ldr x20, [x9]");                                        // x20 = current activation record being examined

    // -- walk and clean every activation above the target stop frame --
    emitter.label("__rt_exception_cleanup_frames_loop");
    emitter.instruction("cmp x20, x19");                                         // have we reached the activation record that should survive the catch?
    emitter.instruction("b.eq __rt_exception_cleanup_frames_done");               // stop once the surviving activation is on top
    emitter.instruction("cbz x20, __rt_exception_cleanup_frames_done");           // stop defensively if the stack unexpectedly bottoms out
    emitter.instruction("ldr x10, [x20, #8]");                                   // load the cleanup callback pointer for this activation
    emitter.instruction("ldr x11, [x20, #16]");                                  // load the saved frame pointer for this activation
    emitter.instruction("cbz x10, __rt_exception_cleanup_frames_next");           // skip callbacks for activations that have no cleanup work
    emitter.instruction("mov x0, x11");                                          // pass the unwound activation's frame pointer to its cleanup callback
    emitter.instruction("blr x10");                                              // run the per-function cleanup callback for this activation

    emitter.label("__rt_exception_cleanup_frames_next");
    emitter.instruction("ldr x20, [x20]");                                       // advance to the previous activation record in the cleanup stack
    emitter.instruction("b __rt_exception_cleanup_frames_loop");                  // continue unwinding older activations until the target is reached

    // -- publish the surviving activation record as the new top --
    emitter.label("__rt_exception_cleanup_frames_done");
    emitter.instruction("adrp x9, _exc_call_frame_top@PAGE");                    // reload page of the call-frame stack top after callback calls
    emitter.instruction("add x9, x9, _exc_call_frame_top@PAGEOFF");              // resolve the call-frame stack top address again
    emitter.instruction("str x19, [x9]");                                        // store the surviving activation record as the new call-frame top
    emitter.instruction("ldp x19, x20, [sp, #16]");                              // restore the callee-saved walk-state registers
    emitter.instruction("ldp x29, x30, [sp, #32]");                              // restore the caller frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                      // release the cleanup walker's stack frame
    emitter.instruction("ret");                                                  // return to the throw helper after unwound-frame cleanup
}
