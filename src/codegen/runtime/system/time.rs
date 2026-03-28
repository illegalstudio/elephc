use crate::codegen::emit::Emitter;

/// __rt_time: get current Unix timestamp via gettimeofday syscall.
/// Output: x0 = seconds since epoch
pub(crate) fn emit_time(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: time ---");
    emitter.label("__rt_time");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes (16 for timeval + 16 for frame + padding)
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set new frame pointer

    // -- call gettimeofday syscall --
    emitter.instruction("add x0, sp, #0");                                      // x0 = pointer to timeval struct on stack
    emitter.instruction("mov x1, #0");                                          // x1 = NULL (timezone not needed)
    emitter.instruction("mov x16, #116");                                       // syscall 116 = gettimeofday
    emitter.instruction("svc #0x80");                                           // invoke macOS kernel

    // -- extract tv_sec from timeval struct --
    emitter.instruction("ldr x0, [sp, #0]");                                    // x0 = tv_sec (first 8 bytes of timeval)

    // -- tear down stack frame --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
