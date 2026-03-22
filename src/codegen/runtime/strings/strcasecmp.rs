use crate::codegen::emit::Emitter;

/// strcasecmp: case-insensitive string comparison.
pub fn emit_strcasecmp(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: strcasecmp ---");
    emitter.label("__rt_strcasecmp");

    // -- determine minimum length for comparison --
    emitter.instruction("cmp x2, x4");                                          // compare lengths of both strings
    emitter.instruction("csel x5, x2, x4, lt");                                 // x5 = min(len_a, len_b)
    emitter.instruction("mov x6, #0");                                          // initialize byte index to 0

    // -- compare bytes (case-insensitive) --
    emitter.label("__rt_strcasecmp_loop");
    emitter.instruction("cmp x6, x5");                                          // check if we've compared all min-length bytes
    emitter.instruction("b.ge __rt_strcasecmp_len");                            // if done, compare by string lengths
    emitter.instruction("ldrb w7, [x1, x6]");                                   // load byte from string A at index
    emitter.instruction("ldrb w8, [x3, x6]");                                   // load byte from string B at index

    // -- convert byte A to lowercase if uppercase --
    emitter.instruction("cmp w7, #65");                                         // compare with 'A'
    emitter.instruction("b.lt __rt_strcasecmp_b");                              // if below 'A', skip conversion
    emitter.instruction("cmp w7, #90");                                         // compare with 'Z'
    emitter.instruction("b.gt __rt_strcasecmp_b");                              // if above 'Z', skip conversion
    emitter.instruction("add w7, w7, #32");                                     // convert A-Z to a-z

    // -- convert byte B to lowercase if uppercase --
    emitter.label("__rt_strcasecmp_b");
    emitter.instruction("cmp w8, #65");                                         // compare with 'A'
    emitter.instruction("b.lt __rt_strcasecmp_cmp");                            // if below 'A', skip conversion
    emitter.instruction("cmp w8, #90");                                         // compare with 'Z'
    emitter.instruction("b.gt __rt_strcasecmp_cmp");                            // if above 'Z', skip conversion
    emitter.instruction("add w8, w8, #32");                                     // convert A-Z to a-z

    // -- compare lowered bytes --
    emitter.label("__rt_strcasecmp_cmp");
    emitter.instruction("cmp w7, w8");                                          // compare the two lowered bytes
    emitter.instruction("b.ne __rt_strcasecmp_diff");                           // if different, return their difference
    emitter.instruction("add x6, x6, #1");                                      // advance to next byte
    emitter.instruction("b __rt_strcasecmp_loop");                              // continue comparing

    // -- bytes differ: return difference --
    emitter.label("__rt_strcasecmp_diff");
    emitter.instruction("sub x0, x7, x8");                                      // return lowered_a - lowered_b
    emitter.instruction("ret");                                                 // return to caller

    // -- all shared bytes equal: compare by length --
    emitter.label("__rt_strcasecmp_len");
    emitter.instruction("sub x0, x2, x4");                                      // return len_a - len_b as tiebreaker
    emitter.instruction("ret");                                                 // return to caller
}
