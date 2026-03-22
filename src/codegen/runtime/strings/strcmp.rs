use crate::codegen::emit::Emitter;

/// strcmp: compare two strings lexicographically.
/// Input: x1/x2 = str_a, x3/x4 = str_b
/// Output: x0 = <0, 0, or >0
pub fn emit_strcmp(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strcmp ---");
    emitter.label("__rt_strcmp");

    // -- determine minimum length for comparison --
    emitter.instruction("cmp x2, x4");                                          // compare lengths of both strings
    emitter.instruction("csel x5, x2, x4, lt");                                 // x5 = min(len_a, len_b)
    emitter.instruction("mov x6, #0");                                          // initialize byte index to 0

    // -- compare bytes up to minimum length --
    emitter.label("__rt_strcmp_loop");
    emitter.instruction("cmp x6, x5");                                          // check if we've compared all min-length bytes
    emitter.instruction("b.ge __rt_strcmp_len");                                // if done, compare by string lengths
    emitter.instruction("ldrb w7, [x1, x6]");                                   // load byte from string A at index
    emitter.instruction("ldrb w8, [x3, x6]");                                   // load byte from string B at index
    emitter.instruction("cmp w7, w8");                                          // compare the two bytes
    emitter.instruction("b.ne __rt_strcmp_diff");                               // if different, return their difference
    emitter.instruction("add x6, x6, #1");                                      // advance to next byte
    emitter.instruction("b __rt_strcmp_loop");                                  // continue comparing

    // -- bytes differ: return difference --
    emitter.label("__rt_strcmp_diff");
    emitter.instruction("sub x0, x7, x8");                                      // return char_a - char_b (negative, 0, or positive)
    emitter.instruction("ret");                                                 // return to caller

    // -- all shared bytes equal: compare by length --
    emitter.label("__rt_strcmp_len");
    emitter.instruction("sub x0, x2, x4");                                      // return len_a - len_b as tiebreaker
    emitter.instruction("ret");                                                 // return to caller
}
