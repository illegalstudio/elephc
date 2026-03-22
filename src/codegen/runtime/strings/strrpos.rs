use crate::codegen::emit::Emitter;

/// strrpos: find last occurrence of needle. Returns position or -1.
pub fn emit_strrpos(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strrpos ---");
    emitter.label("__rt_strrpos");

    // -- edge cases --
    emitter.instruction("cbz x4, __rt_strrpos_empty");                          // empty needle returns last position
    emitter.instruction("cmp x4, x2");                                          // compare needle length with haystack length
    emitter.instruction("b.gt __rt_strrpos_notfound");                          // needle longer than haystack, can't match
    emitter.instruction("sub x5, x2, x4");                                      // start searching from rightmost valid position

    // -- outer loop: try matching needle from right to left --
    emitter.label("__rt_strrpos_outer");
    emitter.instruction("mov x6, #0");                                          // reset needle comparison index
    emitter.label("__rt_strrpos_inner");
    emitter.instruction("cmp x6, x4");                                          // check if all needle bytes matched
    emitter.instruction("b.ge __rt_strrpos_found");                             // all matched, found at position x5
    emitter.instruction("add x7, x5, x6");                                      // compute haystack index = pos + needle_idx
    emitter.instruction("ldrb w8, [x1, x7]");                                   // load haystack byte at computed index
    emitter.instruction("ldrb w9, [x3, x6]");                                   // load needle byte at current index
    emitter.instruction("cmp w8, w9");                                          // compare haystack and needle bytes
    emitter.instruction("b.ne __rt_strrpos_prev");                              // mismatch, try previous position
    emitter.instruction("add x6, x6, #1");                                      // advance needle index
    emitter.instruction("b __rt_strrpos_inner");                                // continue comparing

    // -- move to previous position (searching right to left) --
    emitter.label("__rt_strrpos_prev");
    emitter.instruction("cbz x5, __rt_strrpos_notfound");                       // if at position 0, nowhere left to search
    emitter.instruction("sub x5, x5, #1");                                      // decrement search position
    emitter.instruction("b __rt_strrpos_outer");                                // retry from new position

    // -- return results --
    emitter.label("__rt_strrpos_found");
    emitter.instruction("mov x0, x5");                                          // return last match position
    emitter.instruction("ret");                                                 // return to caller
    emitter.label("__rt_strrpos_empty");
    emitter.instruction("sub x0, x2, #0");                                      // empty needle returns haystack length
    emitter.instruction("ret");                                                 // return to caller
    emitter.label("__rt_strrpos_notfound");
    emitter.instruction("mov x0, #-1");                                         // return -1 (not found)
    emitter.instruction("ret");                                                 // return to caller
}
