use crate::codegen::emit::Emitter;

/// trim: strip whitespace from both ends. Returns adjusted ptr+len (no copy needed).
pub fn emit_trim(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: trim ---");
    // ltrim first, then rtrim
    emitter.label("__rt_trim");

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
