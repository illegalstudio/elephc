use crate::codegen::emit::Emitter;

/// rtrim_mask: strip characters in mask from right of string.
/// Input: x1=str_ptr, x2=str_len, x3=mask_ptr, x4=mask_len
/// Output: x1=str_ptr (unchanged), x2=adjusted_len
pub fn emit_rtrim_mask(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: rtrim_mask ---");
    emitter.label_global("__rt_rtrim_mask");

    // -- loop: check last character against mask --
    emitter.label("__rt_rtrim_mask_loop");
    emitter.instruction("cbz x2, __rt_rtrim_mask_done");                        // if string is empty, nothing to trim
    emitter.instruction("sub x9, x2, #1");                                      // compute index of last character
    emitter.instruction("ldrb w10, [x1, x9]");                                  // load last byte of string

    // -- inner loop: check if w10 is in mask --
    emitter.instruction("mov x11, #0");                                         // mask index = 0
    emitter.label("__rt_rtrim_mask_cmp");
    emitter.instruction("cmp x11, x4");                                         // check if we've exhausted the mask
    emitter.instruction("b.ge __rt_rtrim_mask_done");                           // char not in mask, stop trimming
    emitter.instruction("ldrb w12, [x3, x11]");                                 // load mask character at index x11
    emitter.instruction("cmp w10, w12");                                        // compare string char with mask char
    emitter.instruction("b.eq __rt_rtrim_mask_strip");                          // match found, strip this character
    emitter.instruction("add x11, x11, #1");                                    // advance mask index
    emitter.instruction("b __rt_rtrim_mask_cmp");                               // check next mask character

    // -- strip: shrink length and re-check --
    emitter.label("__rt_rtrim_mask_strip");
    emitter.instruction("sub x2, x2, #1");                                      // reduce length by 1
    emitter.instruction("b __rt_rtrim_mask_loop");                              // check new last character

    emitter.label("__rt_rtrim_mask_done");
    emitter.instruction("ret");                                                 // return with adjusted x2
}
