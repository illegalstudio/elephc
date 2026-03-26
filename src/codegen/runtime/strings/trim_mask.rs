use crate::codegen::emit::Emitter;

/// trim_mask: strip characters in mask from both ends of string.
/// Input: x1=str_ptr, x2=str_len, x3=mask_ptr, x4=mask_len
/// Output: x1=adjusted_ptr, x2=adjusted_len
pub fn emit_trim_mask(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: trim_mask ---");
    emitter.label("__rt_trim_mask");

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
