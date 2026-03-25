use crate::codegen::emit::Emitter;

/// __rt_time: get current Unix timestamp via gettimeofday syscall.
/// Output: x0 = seconds since epoch
pub fn emit_time(emitter: &mut Emitter) {
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

/// __rt_microtime: get current time as float seconds via gettimeofday syscall.
/// Output: d0 = seconds.microseconds as float
pub fn emit_microtime(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: microtime ---");
    emitter.label("__rt_microtime");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes (16 for timeval + 16 for frame + padding)
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set new frame pointer

    // -- call gettimeofday syscall --
    emitter.instruction("add x0, sp, #0");                                      // x0 = pointer to timeval struct on stack
    emitter.instruction("mov x1, #0");                                          // x1 = NULL (timezone not needed)
    emitter.instruction("mov x16, #116");                                       // syscall 116 = gettimeofday
    emitter.instruction("svc #0x80");                                           // invoke macOS kernel

    // -- extract tv_sec and tv_usec --
    emitter.instruction("ldr x0, [sp, #0]");                                    // x0 = tv_sec (seconds)
    emitter.instruction("ldr x1, [sp, #8]");                                    // x1 = tv_usec (microseconds)

    // -- convert to float: d0 = tv_sec + tv_usec / 1000000.0 --
    emitter.instruction("scvtf d0, x0");                                        // d0 = (double)tv_sec
    emitter.instruction("scvtf d1, x1");                                        // d1 = (double)tv_usec
    emitter.instruction("movz x9, #0x4240");                                    // x9 = lower 16 bits of 1000000 (0x0F4240)
    emitter.instruction("movk x9, #0x000F, lsl #16");                           // x9 = 1000000 (microseconds per second)
    emitter.instruction("scvtf d2, x9");                                        // d2 = 1000000.0
    emitter.instruction("fdiv d1, d1, d2");                                     // d1 = tv_usec / 1000000.0
    emitter.instruction("fadd d0, d0, d1");                                     // d0 = tv_sec + tv_usec/1000000.0

    // -- tear down stack frame --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
