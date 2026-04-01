use crate::codegen::emit::Emitter;

/// array_sum: compute the sum of all integer elements in an array.
/// Input: x0 = array pointer
/// Output: x0 = sum of all elements
pub fn emit_array_sum(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_sum ---");
    emitter.label_global("__rt_array_sum");

    // -- set up loop variables --
    emitter.instruction("ldr x9, [x0]");                                        // x9 = array length from header
    emitter.instruction("add x10, x0, #24");                                    // x10 = base of data region (skip 24-byte header)
    emitter.instruction("mov x11, #0");                                         // x11 = i = 0 (loop counter)
    emitter.instruction("mov x12, #0");                                         // x12 = accumulator = 0

    // -- iterate and accumulate sum --
    emitter.label("__rt_array_sum_loop");
    emitter.instruction("cmp x11, x9");                                         // compare i with array length
    emitter.instruction("b.ge __rt_array_sum_done");                            // if i >= length, we're done
    emitter.instruction("ldr x13, [x10, x11, lsl #3]");                         // x13 = data[i]
    emitter.instruction("add x12, x12, x13");                                   // accumulator += data[i]
    emitter.instruction("add x11, x11, #1");                                    // i += 1
    emitter.instruction("b __rt_array_sum_loop");                               // continue loop

    // -- return the sum --
    emitter.label("__rt_array_sum_done");
    emitter.instruction("mov x0, x12");                                         // return sum in x0
    emitter.instruction("ret");                                                 // return to caller
}
