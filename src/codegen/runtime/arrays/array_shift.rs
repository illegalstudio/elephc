use crate::codegen::emit::Emitter;

/// array_shift: remove and return the first element of an integer array.
/// Input: x0 = array pointer
/// Output: x0 = removed first element value
/// Mutates the array in place: shifts all elements left, decrements length.
pub fn emit_array_shift(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_shift ---");
    emitter.label_global("__rt_array_shift");

    // -- check if array is empty --
    emitter.instruction("ldr x9, [x0]");                                        // x9 = current array length
    emitter.instruction("cbnz x9, __rt_array_shift_notempty");                  // if length != 0, proceed normally

    // -- empty array: return null sentinel --
    emitter.instruction("movz x0, #0xFFFE");                                    // load null sentinel bits [15:0]
    emitter.instruction("movk x0, #0xFFFF, lsl #16");                           // load null sentinel bits [31:16]
    emitter.instruction("movk x0, #0xFFFF, lsl #32");                           // load null sentinel bits [47:32]
    emitter.instruction("movk x0, #0x7FFF, lsl #48");                           // load null sentinel bits [63:48] = 0x7FFFFFFFFFFFFFFE
    emitter.instruction("ret");                                                 // return null to caller

    // -- array is not empty, proceed --
    emitter.label("__rt_array_shift_notempty");
    emitter.instruction("add x10, x0, #24");                                    // x10 = base of data region

    // -- save the first element --
    emitter.instruction("ldr x11, [x10]");                                      // x11 = data[0] (element to return)

    // -- shift all elements left by one position --
    emitter.instruction("mov x12, #1");                                         // x12 = src_index = 1

    emitter.label("__rt_array_shift_loop");
    emitter.instruction("cmp x12, x9");                                         // compare src_index with length
    emitter.instruction("b.ge __rt_array_shift_done");                          // if src_index >= length, shifting complete
    emitter.instruction("ldr x13, [x10, x12, lsl #3]");                         // x13 = data[src_index]
    emitter.instruction("sub x14, x12, #1");                                    // x14 = dst_index = src_index - 1
    emitter.instruction("str x13, [x10, x14, lsl #3]");                         // data[dst_index] = data[src_index]
    emitter.instruction("add x12, x12, #1");                                    // src_index += 1
    emitter.instruction("b __rt_array_shift_loop");                             // continue loop

    // -- decrement length and return removed element --
    emitter.label("__rt_array_shift_done");
    emitter.instruction("sub x9, x9, #1");                                      // length -= 1
    emitter.instruction("str x9, [x0]");                                        // write updated length to header
    emitter.instruction("mov x0, x11");                                         // return the removed first element
    emitter.instruction("ret");                                                 // return to caller
}
