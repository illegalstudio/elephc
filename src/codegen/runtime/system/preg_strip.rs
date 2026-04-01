use crate::codegen::emit::Emitter;

/// __rt_preg_strip: strip regex delimiters (e.g., "/pattern/i" → "pattern").
/// Input:  x1=pattern ptr, x2=pattern len
/// Output: x1=stripped pattern ptr, x2=stripped len, x3=flags (bit 0=icase)
pub(crate) fn emit_preg_strip(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: preg_strip_delimiters ---");
    emitter.label_global("__rt_preg_strip");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set new frame pointer

    emitter.instruction("str x1, [sp, #0]");                                    // save pattern ptr
    emitter.instruction("str x2, [sp, #8]");                                    // save pattern len
    emitter.instruction("mov x3, #0");                                          // flags = 0
    emitter.instruction("str x3, [sp, #16]");                                   // save flags

    // -- check if pattern starts with '/' --
    emitter.instruction("ldrb w9, [x1]");                                       // load first byte
    emitter.instruction("cmp w9, #47");                                         // compare with '/'
    emitter.instruction("b.ne __rt_preg_strip_done");                           // not delimited, return as-is

    // -- find closing delimiter by scanning from the end --
    emitter.instruction("sub x10, x2, #1");                                     // start from last char
    emitter.label("__rt_preg_strip_scan");
    emitter.instruction("cmp x10, #1");                                         // must have at least 1 char between delimiters
    emitter.instruction("b.lt __rt_preg_strip_done");                           // no closing delimiter found
    emitter.instruction("ldrb w9, [x1, x10]");                                  // load byte at position
    emitter.instruction("cmp w9, #47");                                         // check for closing '/'
    emitter.instruction("b.eq __rt_preg_strip_found");                          // found it

    // -- check for 'i' flag --
    emitter.instruction("cmp w9, #105");                                        // check for 'i'
    emitter.instruction("b.ne __rt_preg_strip_skip_flag");                      // not 'i'
    emitter.instruction("ldr x3, [sp, #16]");                                   // load flags
    emitter.instruction("orr x3, x3, #1");                                      // set icase flag
    emitter.instruction("str x3, [sp, #16]");                                   // save flags
    emitter.label("__rt_preg_strip_skip_flag");
    emitter.instruction("sub x10, x10, #1");                                    // move backward
    emitter.instruction("b __rt_preg_strip_scan");                              // continue scanning

    // -- found closing delimiter at x10 --
    emitter.label("__rt_preg_strip_found");
    emitter.instruction("add x1, x1, #1");                                      // skip opening delimiter
    emitter.instruction("sub x2, x10, #1");                                     // length = closing_pos - 1

    emitter.label("__rt_preg_strip_done");
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload flags

    // -- tear down and return --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return
}
