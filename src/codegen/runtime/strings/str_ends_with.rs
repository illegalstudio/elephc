use crate::codegen::emit::Emitter;

/// str_ends_with: check if haystack ends with needle.
pub fn emit_str_ends_with(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_ends_with ---");
    emitter.label_global("__rt_str_ends_with");

    // -- check if needle fits in haystack --
    emitter.instruction("cmp x4, x2");                                          // compare needle length with haystack length
    emitter.instruction("b.gt __rt_str_ends_with_no");                          // needle longer than haystack, can't match
    emitter.instruction("sub x5, x2, x4");                                      // compute offset where suffix starts
    emitter.instruction("mov x6, #0");                                          // initialize comparison index

    // -- compare suffix bytes --
    emitter.label("__rt_str_ends_with_loop");
    emitter.instruction("cmp x6, x4");                                          // check if all needle bytes compared
    emitter.instruction("b.ge __rt_str_ends_with_yes");                         // all matched, haystack ends with needle
    emitter.instruction("add x7, x5, x6");                                      // compute haystack index = offset + idx
    emitter.instruction("ldrb w8, [x1, x7]");                                   // load haystack byte at suffix position
    emitter.instruction("ldrb w9, [x3, x6]");                                   // load needle byte at index
    emitter.instruction("cmp w8, w9");                                          // compare the two bytes
    emitter.instruction("b.ne __rt_str_ends_with_no");                          // mismatch, does not end with needle
    emitter.instruction("add x6, x6, #1");                                      // advance to next byte
    emitter.instruction("b __rt_str_ends_with_loop");                           // continue comparing

    // -- return results --
    emitter.label("__rt_str_ends_with_yes");
    emitter.instruction("mov x0, #1");                                          // return 1 (true: ends with needle)
    emitter.instruction("ret");                                                 // return to caller
    emitter.label("__rt_str_ends_with_no");
    emitter.instruction("mov x0, #0");                                          // return 0 (false: does not end with)
    emitter.instruction("ret");                                                 // return to caller
}
