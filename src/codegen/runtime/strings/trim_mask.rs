use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// trim_mask: strip characters in mask from both ends of string.
/// Input: x1=str_ptr, x2=str_len, x3=mask_ptr, x4=mask_len
/// Output: x1=adjusted_ptr, x2=adjusted_len
pub fn emit_trim_mask(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_trim_mask_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: trim_mask ---");
    emitter.label_global("__rt_trim_mask");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #32");                                     // allocate 32 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish new frame pointer

    // -- save mask args across calls (they survive in callee-saved-like slots) --
    emitter.instruction("stp x3, x4, [sp]");                                    // save mask pointer and length on stack

    // -- delegate to ltrim_mask then rtrim_mask --
    emitter.instruction("bl __rt_ltrim_mask");                                  // strip leading mask chars (adjusts x1, x2)
    emitter.instruction("ldp x3, x4, [sp]");                                    // restore mask pointer and length
    emitter.instruction("bl __rt_rtrim_mask");                                  // strip trailing mask chars (adjusts x2)

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_trim_mask_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: trim_mask ---");
    emitter.label_global("__rt_trim_mask");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving aligned spill space for the trim mask pair
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base so trim_mask() can preserve the mask pointer and length across two helper calls
    emitter.instruction("sub rsp, 16");                                         // reserve aligned spill slots for the trim-mask pointer and trim-mask length
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the trim-mask pointer across the nested ltrim_mask() and rtrim_mask() helper calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the trim-mask length across the nested ltrim_mask() and rtrim_mask() helper calls
    emitter.instruction("call __rt_ltrim_mask");                                // trim leading mask bytes first so the borrowed source slice is rebased before the trailing trim pass
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // restore the trim-mask pointer before invoking the trailing trim helper on the adjusted source slice
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // restore the trim-mask length before invoking the trailing trim helper on the adjusted source slice
    emitter.instruction("call __rt_rtrim_mask");                                // trim trailing mask bytes from the already-left-trimmed source slice
    emitter.instruction("add rsp, 16");                                         // release the trim-mask spill slots before returning the adjusted source slice
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the adjusted source slice
    emitter.instruction("ret");                                                 // return the adjusted borrowed source string slice in the standard x86_64 string result registers
}
