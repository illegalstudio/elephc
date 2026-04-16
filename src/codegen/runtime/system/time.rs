use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// __rt_time: get current Unix timestamp via gettimeofday syscall.
/// Output: x0 = seconds since epoch
pub(crate) fn emit_time(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_time_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: time ---");
    emitter.label_global("__rt_time");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes (16 for timeval + 16 for frame + padding)
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set new frame pointer

    // -- call gettimeofday syscall --
    emitter.instruction("add x0, sp, #0");                                      // x0 = pointer to timeval struct on stack
    emitter.instruction("mov x1, #0");                                          // x1 = NULL (timezone not needed)
    emitter.syscall(116);

    // -- extract tv_sec from timeval struct --
    emitter.instruction("ldr x0, [sp, #0]");                                    // x0 = tv_sec (first 8 bytes of timeval)

    // -- tear down stack frame --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_time_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: time ---");
    emitter.label_global("__rt_time");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before allocating the temporary timeval storage for libc gettimeofday()
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the temporary timeval storage used by libc gettimeofday()
    emitter.instruction("sub rsp, 32");                                         // reserve aligned stack storage for one timeval struct plus scratch padding before the libc call
    emitter.instruction("lea rdi, [rsp]");                                      // pass the temporary timeval storage as the first SysV integer argument to libc gettimeofday()
    emitter.instruction("xor esi, esi");                                        // pass NULL as the timezone pointer because elephc only needs the current Unix timestamp
    emitter.bl_c("gettimeofday");                                               // fill the temporary timeval with the current wall-clock time through libc
    emitter.instruction("mov rax, QWORD PTR [rsp]");                            // return tv_sec from the temporary timeval as the current Unix timestamp in the native integer result register
    emitter.instruction("leave");                                               // release the temporary timeval storage and restore the caller frame pointer in one step
    emitter.instruction("ret");                                                 // return the current Unix timestamp to generated code
}
