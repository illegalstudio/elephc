use crate::codegen::{abi, emit::Emitter};
use crate::codegen::platform::Arch;

pub fn emit_exception_cleanup_frames(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_exception_cleanup_frames_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: exception_cleanup_frames ---");
    emitter.label_global("__rt_exception_cleanup_frames");

    // -- save callee-saved state used by the cleanup walk --
    emitter.instruction("sub sp, sp, #48");                                     // reserve stack space for x19/x20 plus frame linkage
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address for the cleanup walker
    emitter.instruction("stp x19, x20, [sp, #16]");                             // preserve callee-saved registers that track the walk state
    emitter.instruction("add x29, sp, #32");                                    // install the cleanup walker's frame pointer
    emitter.instruction("mov x19, x0");                                         // x19 = activation record that should remain on top after unwinding
    abi::emit_load_symbol_to_reg(emitter, "x20", "_exc_call_frame_top", 0);

    // -- walk and clean every activation above the target stop frame --
    emitter.label("__rt_exception_cleanup_frames_loop");
    emitter.instruction("cmp x20, x19");                                        // have we reached the activation record that should survive the catch?
    emitter.instruction("b.eq __rt_exception_cleanup_frames_done");             // stop once the surviving activation is on top
    emitter.instruction("cbz x20, __rt_exception_cleanup_frames_done");         // stop defensively if the stack unexpectedly bottoms out
    emitter.instruction("ldr x10, [x20, #8]");                                  // load the cleanup callback pointer for this activation
    emitter.instruction("ldr x11, [x20, #16]");                                 // load the saved frame pointer for this activation
    emitter.instruction("cbz x10, __rt_exception_cleanup_frames_next");         // skip callbacks for activations that have no cleanup work
    emitter.instruction("mov x0, x11");                                         // pass the unwound activation's frame pointer to its cleanup callback
    emitter.instruction("blr x10");                                             // run the per-function cleanup callback for this activation

    emitter.label("__rt_exception_cleanup_frames_next");
    emitter.instruction("ldr x20, [x20]");                                      // advance to the previous activation record in the cleanup stack
    emitter.instruction("b __rt_exception_cleanup_frames_loop");                // continue unwinding older activations until the target is reached

    // -- publish the surviving activation record as the new top --
    emitter.label("__rt_exception_cleanup_frames_done");
    abi::emit_store_reg_to_symbol(emitter, "x19", "_exc_call_frame_top", 0);
    emitter.instruction("ldp x19, x20, [sp, #16]");                             // restore the callee-saved walk-state registers
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore the caller frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the cleanup walker's stack frame
    emitter.instruction("ret");                                                 // return to the throw helper after unwound-frame cleanup
}

fn emit_exception_cleanup_frames_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: exception_cleanup_frames ---");
    emitter.label_global("__rt_exception_cleanup_frames");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while the cleanup walker runs
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the x86_64 cleanup walker
    emitter.instruction("push r12");                                            // preserve the target activation record across cleanup callback invocations
    emitter.instruction("push r13");                                            // preserve the current activation cursor across cleanup callback invocations
    emitter.instruction("mov r12, rdi");                                        // r12 = activation record that should remain on top after unwinding
    abi::emit_load_symbol_to_reg(emitter, "r13", "_exc_call_frame_top", 0);

    emitter.label("__rt_exception_cleanup_frames_loop");
    emitter.instruction("cmp r13, r12");                                        // have we reached the activation record that should survive the catch?
    emitter.instruction("je __rt_exception_cleanup_frames_done");                // stop once the surviving activation is on top
    emitter.instruction("test r13, r13");                                       // has the cleanup stack unexpectedly bottomed out?
    emitter.instruction("je __rt_exception_cleanup_frames_done");                // stop defensively when no more activation records remain
    emitter.instruction("mov r10, QWORD PTR [r13 + 8]");                        // load the cleanup callback pointer for the current unwound activation
    emitter.instruction("mov r11, QWORD PTR [r13 + 16]");                       // load the saved frame pointer for the current unwound activation
    emitter.instruction("test r10, r10");                                       // does this activation record have cleanup work to run?
    emitter.instruction("je __rt_exception_cleanup_frames_next");                // skip callback execution when the activation carries no cleanup hook
    emitter.instruction("mov rdi, r11");                                        // pass the unwound activation frame pointer into the cleanup callback ABI register
    emitter.instruction("call r10");                                            // run the per-function cleanup callback for this unwound activation record

    emitter.label("__rt_exception_cleanup_frames_next");
    emitter.instruction("mov r13, QWORD PTR [r13]");                            // advance to the previous activation record in the cleanup stack
    emitter.instruction("jmp __rt_exception_cleanup_frames_loop");               // continue unwinding older activation records until the survivor is reached

    emitter.label("__rt_exception_cleanup_frames_done");
    abi::emit_store_reg_to_symbol(emitter, "r12", "_exc_call_frame_top", 0);
    emitter.instruction("pop r13");                                             // restore the saved cleanup-walk cursor register before returning
    emitter.instruction("pop r12");                                             // restore the saved survivor activation register before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to the throw helper
    emitter.instruction("ret");                                                 // return to the throw helper after unwound-frame cleanup
}
