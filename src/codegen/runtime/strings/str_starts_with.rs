use crate::codegen::emit::Emitter;

/// str_starts_with: check if haystack starts with needle.
/// Input: x1/x2=haystack, x3/x4=needle
/// Output: x0 = 1 if starts with, 0 otherwise
pub fn emit_str_starts_with(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_starts_with ---");
    emitter.label_global("__rt_str_starts_with");

    // -- check if needle fits in haystack --
    emitter.instruction("cmp x4, x2");                                          // compare needle length with haystack length
    emitter.instruction("b.gt __rt_str_starts_with_no");                        // needle longer than haystack, can't match
    emitter.instruction("mov x5, #0");                                          // initialize comparison index

    // -- compare prefix bytes --
    emitter.label("__rt_str_starts_with_loop");
    emitter.instruction("cmp x5, x4");                                          // check if all needle bytes compared
    emitter.instruction("b.ge __rt_str_starts_with_yes");                       // all matched, haystack starts with needle
    emitter.instruction("ldrb w6, [x1, x5]");                                   // load haystack byte at index
    emitter.instruction("ldrb w7, [x3, x5]");                                   // load needle byte at index
    emitter.instruction("cmp w6, w7");                                          // compare the two bytes
    emitter.instruction("b.ne __rt_str_starts_with_no");                        // mismatch, does not start with needle
    emitter.instruction("add x5, x5, #1");                                      // advance to next byte
    emitter.instruction("b __rt_str_starts_with_loop");                         // continue comparing

    // -- return results --
    emitter.label("__rt_str_starts_with_yes");
    emitter.instruction("mov x0, #1");                                          // return 1 (true: starts with needle)
    emitter.instruction("ret");                                                 // return to caller
    emitter.label("__rt_str_starts_with_no");
    emitter.instruction("mov x0, #0");                                          // return 0 (false: does not start with)
    emitter.instruction("ret");                                                 // return to caller
}
