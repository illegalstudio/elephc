use crate::codegen::emit::Emitter;

/// array_fill_keys: create associative array from keys with the same value.
/// Input:  x0=keys_array (string array), x1=value (int)
/// Output: x0=new hash table
pub fn emit_array_fill_keys(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_fill_keys ---");
    emitter.label("__rt_array_fill_keys");

    // -- set up stack frame, save arguments --
    // Stack layout:
    //   [sp, #0]  = keys array pointer
    //   [sp, #8]  = fill value
    //   [sp, #16] = hash table pointer (result)
    //   [sp, #24] = loop index i
    //   [sp, #32] = saved x29
    //   [sp, #40] = saved x30
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save keys array pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save fill value

    // -- create hash table with capacity = length * 2 --
    emitter.instruction("ldr x0, [x0]");                                        // x0 = keys array length
    emitter.instruction("lsl x0, x0, #1");                                      // x0 = length * 2 (capacity with headroom)
    emitter.instruction("mov x9, #16");                                         // x9 = minimum capacity
    emitter.instruction("cmp x0, x9");                                          // compare with minimum
    emitter.instruction("csel x0, x9, x0, lt");                                 // if length*2 < 16, use 16
    emitter.instruction("mov x1, #0");                                          // value_type = 0 (int)
    emitter.instruction("bl __rt_hash_new");                                    // create hash table, x0 = hash ptr
    emitter.instruction("str x0, [sp, #16]");                                   // save hash table pointer

    // -- loop over keys --
    emitter.instruction("str xzr, [sp, #24]");                                  // i = 0

    emitter.label("__rt_array_fill_keys_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload keys array pointer
    emitter.instruction("ldr x3, [x0]");                                        // x3 = keys array length
    emitter.instruction("ldr x4, [sp, #24]");                                   // x4 = i
    emitter.instruction("cmp x4, x3");                                          // compare i with length
    emitter.instruction("b.ge __rt_array_fill_keys_done");                      // if i >= length, done

    // -- load key string from keys[i] (16 bytes per string element) --
    emitter.instruction("lsl x5, x4, #4");                                      // x5 = i * 16 (byte offset for string element)
    emitter.instruction("add x5, x0, x5");                                      // x5 = keys_array + byte offset
    emitter.instruction("add x5, x5, #24");                                     // x5 = skip header to data region
    emitter.instruction("ldr x1, [x5]");                                        // x1 = key_ptr = keys[i].ptr
    emitter.instruction("ldr x2, [x5, #8]");                                    // x2 = key_len = keys[i].len

    // -- call hash_set with fill value --
    emitter.instruction("ldr x0, [sp, #16]");                                   // x0 = hash table pointer
    emitter.instruction("ldr x3, [sp, #8]");                                    // x3 = value_lo = fill value
    emitter.instruction("mov x4, #0");                                          // x4 = value_hi = 0
    emitter.instruction("bl __rt_hash_set");                                    // insert key-value pair

    // -- advance loop --
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload i
    emitter.instruction("add x4, x4, #1");                                      // i += 1
    emitter.instruction("str x4, [sp, #24]");                                   // save updated i
    emitter.instruction("b __rt_array_fill_keys_loop");                         // continue loop

    // -- return hash table --
    emitter.label("__rt_array_fill_keys_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // x0 = hash table pointer
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = hash table
}
