use crate::codegen::emit::Emitter;

/// array_key_exists: check if an integer key exists in an indexed array.
/// Input: x0 = array pointer, x1 = key (integer index)
/// Output: x0 = 1 if key exists, 0 if not
pub fn emit_array_key_exists(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_key_exists ---");
    emitter.label("__rt_array_key_exists");

    // -- check if key is in bounds [0, length) --
    emitter.instruction("ldr x9, [x0]");                                        // x9 = current array length from header
    emitter.instruction("cmp x1, #0");                                          // check if key is negative
    emitter.instruction("b.lt __rt_array_key_exists_no");                       // negative keys don't exist
    emitter.instruction("cmp x1, x9");                                          // compare key with array length
    emitter.instruction("b.ge __rt_array_key_exists_no");                       // if key >= length, does not exist

    // -- key exists --
    emitter.instruction("mov x0, #1");                                          // return true
    emitter.instruction("ret");                                                 // return to caller

    // -- key does not exist --
    emitter.label("__rt_array_key_exists_no");
    emitter.instruction("mov x0, #0");                                          // return false
    emitter.instruction("ret");                                                 // return to caller
}
