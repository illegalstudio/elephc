use crate::codegen::emit::Emitter;

/// str_eq: compare two strings for equality.
/// Input:  x1=ptr_a, x2=len_a, x3=ptr_b, x4=len_b
/// Output: x0 = 1 if equal, 0 if not
pub fn emit_str_eq(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_eq ---");
    emitter.label_global("__rt_str_eq");

    // -- quick length check: different lengths means not equal --
    emitter.instruction("cmp x2, x4");                                          // compare lengths of both strings
    emitter.instruction("b.ne __rt_str_eq_false");                              // if lengths differ, strings can't be equal

    // -- byte-by-byte comparison --
    emitter.instruction("cbz x2, __rt_str_eq_true");                            // if both empty (len=0), they're equal
    emitter.label("__rt_str_eq_loop");
    emitter.instruction("ldrb w5, [x1], #1");                                   // load byte from string A, advance pointer
    emitter.instruction("ldrb w6, [x3], #1");                                   // load byte from string B, advance pointer
    emitter.instruction("cmp w5, w6");                                          // compare the two bytes
    emitter.instruction("b.ne __rt_str_eq_false");                              // mismatch found, strings not equal
    emitter.instruction("sub x2, x2, #1");                                      // decrement remaining byte count
    emitter.instruction("cbnz x2, __rt_str_eq_loop");                           // if bytes remain, continue comparing

    // -- strings are equal --
    emitter.label("__rt_str_eq_true");
    emitter.instruction("mov x0, #1");                                          // return 1 (true: strings are equal)
    emitter.instruction("ret");                                                 // return to caller

    // -- strings are not equal --
    emitter.label("__rt_str_eq_false");
    emitter.instruction("mov x0, #0");                                          // return 0 (false: strings differ)
    emitter.instruction("ret");                                                 // return to caller
}
