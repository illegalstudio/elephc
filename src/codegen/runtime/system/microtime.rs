use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// __rt_microtime: get current time as float seconds via gettimeofday syscall.
/// Output: d0 = seconds.microseconds as float
pub(crate) fn emit_microtime(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_microtime_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: microtime ---");
    emitter.label_global("__rt_microtime");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes (16 for timeval + 16 for frame + padding)
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set new frame pointer

    // -- call gettimeofday syscall --
    emitter.instruction("add x0, sp, #0");                                      // x0 = pointer to timeval struct on stack
    emitter.instruction("mov x1, #0");                                          // x1 = NULL (timezone not needed)
    emitter.syscall(116);

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

fn emit_microtime_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: microtime ---");
    emitter.label_global("__rt_microtime");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before allocating the temporary timeval storage for libc gettimeofday()
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the temporary timeval storage used by libc gettimeofday()
    emitter.instruction("sub rsp, 32");                                         // reserve aligned stack storage for one timeval struct plus scratch padding before the libc call
    emitter.instruction("lea rdi, [rsp]");                                      // pass the temporary timeval storage as the first SysV integer argument to libc gettimeofday()
    emitter.instruction("xor esi, esi");                                        // pass NULL as the timezone pointer because elephc only needs the current Unix timestamp
    emitter.bl_c("gettimeofday");                                               // fill the temporary timeval with the current wall-clock time through libc
    emitter.instruction("cvtsi2sd xmm0, QWORD PTR [rsp]");                      // convert tv_sec from the temporary timeval into the base double-precision second count
    emitter.instruction("cvtsi2sd xmm1, QWORD PTR [rsp + 8]");                  // convert tv_usec from the temporary timeval into a double-precision microsecond count
    emitter.instruction("mov r10, 1000000");                                    // materialize the number of microseconds per second before converting it into a floating divisor
    emitter.instruction("cvtsi2sd xmm2, r10");                                  // convert the microseconds-per-second divisor into double precision for the fractional-second normalization
    emitter.instruction("divsd xmm1, xmm2");                                    // normalize the microsecond count into the fractional-second component of the final result
    emitter.instruction("addsd xmm0, xmm1");                                    // combine the whole-second and fractional-second components into the final double-precision timestamp
    emitter.instruction("leave");                                               // release the temporary timeval storage and restore the caller frame pointer in one step
    emitter.instruction("ret");                                                 // return the floating-point Unix timestamp to generated code
}
