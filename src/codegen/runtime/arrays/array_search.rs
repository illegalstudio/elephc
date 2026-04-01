use crate::codegen::emit::Emitter;

/// array_search: find a value in an integer array and return its index.
/// Input: x0 = array pointer, x1 = needle (integer value)
/// Output: x0 = index of first match, or -1 if not found
pub fn emit_array_search(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_search ---");
    emitter.label_global("__rt_array_search");

    // -- set up loop variables --
    emitter.instruction("ldr x9, [x0]");                                        // x9 = array length from header
    emitter.instruction("add x10, x0, #24");                                    // x10 = base of data region (skip 24-byte header)
    emitter.instruction("mov x11, #0");                                         // x11 = i = 0 (loop counter)

    // -- iterate through elements looking for needle --
    emitter.label("__rt_array_search_loop");
    emitter.instruction("cmp x11, x9");                                         // compare i with array length
    emitter.instruction("b.ge __rt_array_search_notfound");                     // if i >= length, value not found
    emitter.instruction("ldr x12, [x10, x11, lsl #3]");                         // x12 = data[i] (load element at index i)
    emitter.instruction("cmp x12, x1");                                         // compare element with needle
    emitter.instruction("b.eq __rt_array_search_found");                        // if equal, we found it
    emitter.instruction("add x11, x11, #1");                                    // i += 1
    emitter.instruction("b __rt_array_search_loop");                            // continue loop

    // -- value found at index x11 --
    emitter.label("__rt_array_search_found");
    emitter.instruction("mov x0, x11");                                         // return the index
    emitter.instruction("ret");                                                 // return to caller

    // -- value not found --
    emitter.label("__rt_array_search_notfound");
    emitter.instruction("mov x0, #-1");                                         // return -1 (not found sentinel)
    emitter.instruction("ret");                                                 // return to caller
}
