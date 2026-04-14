use crate::codegen::{emit::Emitter, platform::Arch};

/// trim: strip whitespace from both ends. Returns adjusted ptr+len (no copy needed).
pub fn emit_trim(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_trim_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: trim ---");
    // ltrim first, then rtrim
    emitter.label_global("__rt_trim");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #16");                                     // allocate 16 bytes on the stack
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish new frame pointer

    // -- delegate to ltrim then rtrim --
    emitter.instruction("bl __rt_ltrim");                                       // strip leading whitespace (adjusts x1, x2)
    emitter.instruction("bl __rt_rtrim");                                       // strip trailing whitespace (adjusts x2)

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_trim_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: trim ---");
    emitter.label_global("__rt_trim");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while trim() delegates to the x86_64 ltrim/rtrim helpers
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base even though trim() only forwards the borrowed string pair
    emitter.instruction("call __rt_ltrim");                                     // strip leading whitespace from the borrowed elephc string slice first
    emitter.instruction("call __rt_rtrim");                                     // strip trailing whitespace from the borrowed elephc string slice after the left trim
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the trim helper chain completes
    emitter.instruction("ret");                                                 // return the adjusted borrowed string slice in rax/rdx
}
