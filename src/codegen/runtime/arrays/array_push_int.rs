use crate::codegen::emit::Emitter;

/// array_push_int: push an integer element to an array.
/// Input: x0 = array pointer, x1 = value
pub fn emit_array_push_int(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_push_int ---");
    emitter.label("__rt_array_push_int");

    // -- store the integer at the next available slot --
    emitter.instruction("ldr x9, [x0]");                                        // x9 = current array length
    emitter.instruction("add x10, x0, #24");                                    // x10 = base of data region (skip 24-byte header)
    emitter.instruction("str x1, [x10, x9, lsl #3]");                           // store value at data[length * 8] (8 bytes per int)

    // -- increment the array length --
    emitter.instruction("add x9, x9, #1");                                      // length += 1
    emitter.instruction("str x9, [x0]");                                        // write updated length back to header
    emitter.instruction("ret");                                                 // return to caller
}
