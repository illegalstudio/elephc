use crate::codegen::emit::Emitter;

pub fn emit_throw_current(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: throw_current ---");
    emitter.label_global("__rt_throw_current");

    // -- save callee-saved state while the throw helper inspects handler stacks --
    emitter.instruction("sub sp, sp, #48");                                     // reserve stack space for handler state and frame linkage
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address for the throw helper
    emitter.instruction("stp x19, x20, [sp, #16]");                             // preserve callee-saved registers that hold handler metadata
    emitter.instruction("add x29, sp, #32");                                    // install the throw helper's frame pointer
    emitter.adrp("x9", "_exc_handler_top");                      // load page of the exception-handler stack top
    emitter.add_lo12("x9", "x9", "_exc_handler_top");                // resolve the exception-handler stack top address
    emitter.instruction("ldr x19, [x9]");                                       // x19 = current top-most exception handler
    emitter.instruction("cbz x19, __rt_throw_current_uncaught");                // fall back to a fatal uncaught-exception path when no handler exists
    emitter.instruction("ldr x0, [x19, #8]");                                   // x0 = activation record that should survive this catch
    emitter.instruction("bl __rt_exception_cleanup_frames");                    // run cleanup callbacks for every unwound activation frame
    emitter.adrp("x9", "_concat_off");                           // load page of the concat cursor before resuming via longjmp
    emitter.add_lo12("x9", "x9", "_concat_off");                     // resolve the concat cursor address
    emitter.instruction("str xzr, [x9]");                                       // clear any partially-built concat state before catch/finally code resumes
    emitter.instruction("add x0, x19, #16");                                    // x0 = jmp_buf base stored inside the active handler record
    emitter.instruction("mov x1, #1");                                          // longjmp return value = 1 to indicate exceptional control flow
    emitter.bl_c("longjmp");                                         // transfer control directly back to the saved catch resume point

    // -- uncaught exceptions terminate the process with a fatal message --
    emitter.label("__rt_throw_current_uncaught");
    emitter.adrp("x1", "_uncaught_exc_msg");                     // load page of the uncaught-exception error message
    emitter.add_lo12("x1", "x1", "_uncaught_exc_msg");               // resolve the uncaught-exception error message address
    emitter.instruction("mov x2, #32");                                         // uncaught exception message length in bytes
    emitter.instruction("mov x0, #2");                                          // fd = stderr for fatal runtime diagnostics
    emitter.syscall(4);
    emitter.instruction("mov x0, #1");                                          // exit status 1 indicates abnormal termination
    emitter.syscall(1);
}
