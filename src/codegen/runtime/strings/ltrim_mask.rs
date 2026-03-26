use crate::codegen::emit::Emitter;

/// ltrim_mask: strip characters in mask from left of string.
/// Input: x1=str_ptr, x2=str_len, x3=mask_ptr, x4=mask_len
/// Output: x1=adjusted_ptr, x2=adjusted_len
pub fn emit_ltrim_mask(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ltrim_mask ---");
    emitter.label("__rt_ltrim_mask");

    // -- loop: check first character against mask --
    emitter.label("__rt_ltrim_mask_loop");
    emitter.instruction("cbz x2, __rt_ltrim_mask_done");                        // if string is empty, nothing to trim
    emitter.instruction("ldrb w10, [x1]");                                      // load first byte of string

    // -- inner loop: check if w10 is in mask --
    emitter.instruction("mov x11, #0");                                         // mask index = 0
    emitter.label("__rt_ltrim_mask_cmp");
    emitter.instruction("cmp x11, x4");                                         // check if we've exhausted the mask
    emitter.instruction("b.ge __rt_ltrim_mask_done");                           // char not in mask, stop trimming
    emitter.instruction("ldrb w12, [x3, x11]");                                 // load mask character at index x11
    emitter.instruction("cmp w10, w12");                                        // compare string char with mask char
    emitter.instruction("b.eq __rt_ltrim_mask_skip");                           // match found, skip this character
    emitter.instruction("add x11, x11, #1");                                    // advance mask index
    emitter.instruction("b __rt_ltrim_mask_cmp");                               // check next mask character

    // -- skip: advance pointer, shrink length, re-check --
    emitter.label("__rt_ltrim_mask_skip");
    emitter.instruction("add x1, x1, #1");                                      // advance string pointer past matched char
    emitter.instruction("sub x2, x2, #1");                                      // decrement string length
    emitter.instruction("b __rt_ltrim_mask_loop");                              // check next first character

    emitter.label("__rt_ltrim_mask_done");
    emitter.instruction("ret");                                                 // return with adjusted x1 and x2
}
