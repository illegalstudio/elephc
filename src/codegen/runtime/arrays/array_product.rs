use crate::codegen::emit::Emitter;

/// array_product: compute the product of all integer elements in an array.
/// Input: x0 = array pointer
/// Output: x0 = product of all elements (1 for empty arrays)
pub fn emit_array_product(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_product ---");
    emitter.label_global("__rt_array_product");

    // -- set up loop variables --
    emitter.instruction("ldr x9, [x0]");                                        // x9 = array length from header
    emitter.instruction("add x10, x0, #24");                                    // x10 = base of data region (skip 24-byte header)
    emitter.instruction("mov x11, #0");                                         // x11 = i = 0 (loop counter)
    emitter.instruction("mov x12, #1");                                         // x12 = accumulator = 1 (multiplicative identity)

    // -- iterate and accumulate product --
    emitter.label("__rt_array_product_loop");
    emitter.instruction("cmp x11, x9");                                         // compare i with array length
    emitter.instruction("b.ge __rt_array_product_done");                        // if i >= length, we're done
    emitter.instruction("ldr x13, [x10, x11, lsl #3]");                         // x13 = data[i]
    emitter.instruction("mul x12, x12, x13");                                   // accumulator *= data[i]
    emitter.instruction("add x11, x11, #1");                                    // i += 1
    emitter.instruction("b __rt_array_product_loop");                           // continue loop

    // -- return the product --
    emitter.label("__rt_array_product_done");
    emitter.instruction("mov x0, x12");                                         // return product in x0
    emitter.instruction("ret");                                                 // return to caller
}
