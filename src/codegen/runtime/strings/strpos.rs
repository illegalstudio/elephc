use crate::codegen::emit::Emitter;

/// strpos: find needle in haystack. Returns position in x0, or -1 if not found.
/// Input: x1=haystack_ptr, x2=haystack_len, x3=needle_ptr, x4=needle_len
/// Output: x0 = position (or -1)
pub fn emit_strpos(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strpos ---");
    emitter.label_global("__rt_strpos");

    // -- edge cases --
    emitter.instruction("cbz x4, __rt_strpos_empty");                           // empty needle always matches at position 0
    emitter.instruction("cmp x4, x2");                                          // compare needle length with haystack length
    emitter.instruction("b.gt __rt_strpos_notfound");                           // needle longer than haystack, can't match
    emitter.instruction("mov x5, #0");                                          // initialize search position to 0

    // -- outer loop: try matching needle at each position --
    emitter.label("__rt_strpos_outer");
    emitter.instruction("sub x9, x2, x4");                                      // last valid start = haystack_len - needle_len
    emitter.instruction("cmp x5, x9");                                          // check if position exceeds last valid start
    emitter.instruction("b.gt __rt_strpos_notfound");                           // past end, needle not found

    // -- inner loop: compare needle bytes at current position --
    emitter.instruction("mov x6, #0");                                          // needle comparison index = 0
    emitter.label("__rt_strpos_inner");
    emitter.instruction("cmp x6, x4");                                          // check if all needle bytes matched
    emitter.instruction("b.ge __rt_strpos_found");                              // all matched, found at position x5
    emitter.instruction("add x7, x5, x6");                                      // compute haystack index = pos + needle_idx
    emitter.instruction("ldrb w8, [x1, x7]");                                   // load haystack byte at computed index
    emitter.instruction("ldrb w9, [x3, x6]");                                   // load needle byte at current index
    emitter.instruction("cmp w8, w9");                                          // compare haystack and needle bytes
    emitter.instruction("b.ne __rt_strpos_next");                               // mismatch, try next position
    emitter.instruction("add x6, x6, #1");                                      // advance needle index
    emitter.instruction("b __rt_strpos_inner");                                 // continue comparing

    // -- advance to next haystack position --
    emitter.label("__rt_strpos_next");
    emitter.instruction("add x5, x5, #1");                                      // increment search position
    emitter.instruction("b __rt_strpos_outer");                                 // retry from new position

    // -- return results --
    emitter.label("__rt_strpos_found");
    emitter.instruction("mov x0, x5");                                          // return match position
    emitter.instruction("ret");                                                 // return to caller
    emitter.label("__rt_strpos_empty");
    emitter.instruction("mov x0, #0");                                          // empty needle found at position 0
    emitter.instruction("ret");                                                 // return to caller
    emitter.label("__rt_strpos_notfound");
    emitter.instruction("mov x0, #-1");                                         // return -1 (not found)
    emitter.instruction("ret");                                                 // return to caller
}
