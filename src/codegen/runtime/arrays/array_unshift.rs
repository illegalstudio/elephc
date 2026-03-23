use crate::codegen::emit::Emitter;

/// array_unshift: prepend an integer value to the front of an array.
/// Input: x0 = array pointer, x1 = value to prepend
/// Output: x0 = new array length
/// Mutates the array in place: shifts all elements right, inserts at index 0.
pub fn emit_array_unshift(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_unshift ---");
    emitter.label("__rt_array_unshift");

    // -- load array metadata --
    emitter.instruction("ldr x9, [x0]");                                        // x9 = current array length
    emitter.instruction("add x10, x0, #24");                                    // x10 = base of data region

    // -- shift all elements right by one, starting from the end --
    emitter.instruction("sub x11, x9, #1");                                     // x11 = src_index = length - 1 (last element)

    emitter.label("__rt_array_unshift_loop");
    emitter.instruction("cmp x11, #0");                                         // check if src_index < 0
    emitter.instruction("b.lt __rt_array_unshift_insert");                      // if so, shifting complete
    emitter.instruction("ldr x12, [x10, x11, lsl #3]");                         // x12 = data[src_index]
    emitter.instruction("add x13, x11, #1");                                    // x13 = dst_index = src_index + 1
    emitter.instruction("str x12, [x10, x13, lsl #3]");                         // data[dst_index] = data[src_index]
    emitter.instruction("sub x11, x11, #1");                                    // src_index -= 1
    emitter.instruction("b __rt_array_unshift_loop");                           // continue loop

    // -- insert value at index 0 and update length --
    emitter.label("__rt_array_unshift_insert");
    emitter.instruction("str x1, [x10]");                                       // data[0] = value
    emitter.instruction("add x9, x9, #1");                                      // length += 1
    emitter.instruction("str x9, [x0]");                                        // write updated length to header
    emitter.instruction("mov x0, x9");                                          // return new length
    emitter.instruction("ret");                                                 // return to caller
}
