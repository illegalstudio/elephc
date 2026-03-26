use crate::codegen::emit::Emitter;

/// array_flip: swap keys and values for an indexed int array.
/// Creates a hash table where str(value) becomes the key and the index becomes the value.
/// Input:  x0=array_ptr (indexed int array)
/// Output: x0=new hash table
pub fn emit_array_flip(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_flip ---");
    emitter.label("__rt_array_flip");

    // -- set up stack frame, save arguments --
    // Stack layout:
    //   [sp, #0]  = source array pointer
    //   [sp, #8]  = hash table pointer (result)
    //   [sp, #16] = loop index i
    //   [sp, #24] = saved x29
    //   [sp, #32] = saved x30
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save source array pointer

    // -- create hash table with capacity = array length * 2 (load factor) --
    emitter.instruction("ldr x0, [x0]");                                        // x0 = array length
    emitter.instruction("lsl x0, x0, #1");                                      // x0 = length * 2 (capacity with headroom)
    emitter.instruction("mov x9, #16");                                         // x9 = minimum capacity
    emitter.instruction("cmp x0, x9");                                          // compare with minimum
    emitter.instruction("csel x0, x9, x0, lt");                                 // if length*2 < 16, use 16
    emitter.instruction("mov x1, #0");                                          // value_type = 0 (int)
    emitter.instruction("bl __rt_hash_new");                                    // create hash table, x0 = hash ptr
    emitter.instruction("str x0, [sp, #8]");                                    // save hash table pointer

    // -- loop over array elements --
    emitter.instruction("str xzr, [sp, #16]");                                  // i = 0

    emitter.label("__rt_array_flip_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload source array pointer
    emitter.instruction("ldr x3, [x0]");                                        // x3 = array length
    emitter.instruction("ldr x4, [sp, #16]");                                   // x4 = i
    emitter.instruction("cmp x4, x3");                                          // compare i with length
    emitter.instruction("b.ge __rt_array_flip_done");                           // if i >= length, done

    // -- load array[i] and convert to string key --
    emitter.instruction("add x5, x0, #24");                                     // x5 = data base
    emitter.instruction("ldr x0, [x5, x4, lsl #3]");                            // x0 = array[i] (the integer value)
    emitter.instruction("bl __rt_itoa");                                        // convert int to string, x1=ptr, x2=len

    // -- call hash_set: key=str(value), value=index --
    emitter.instruction("ldr x0, [sp, #8]");                                    // x0 = hash table pointer
    // x1 = key_ptr (from itoa)
    // x2 = key_len (from itoa)
    emitter.instruction("ldr x3, [sp, #16]");                                   // x3 = value_lo = index i
    emitter.instruction("mov x4, #0");                                          // x4 = value_hi = 0
    emitter.instruction("bl __rt_hash_set");                                    // insert key-value pair
    emitter.instruction("str x0, [sp, #8]");                                    // update hash table pointer after possible growth

    // -- advance loop --
    emitter.instruction("ldr x4, [sp, #16]");                                   // reload i
    emitter.instruction("add x4, x4, #1");                                      // i += 1
    emitter.instruction("str x4, [sp, #16]");                                   // save updated i
    emitter.instruction("b __rt_array_flip_loop");                              // continue loop

    // -- return hash table --
    emitter.label("__rt_array_flip_done");
    emitter.instruction("ldr x0, [sp, #8]");                                    // x0 = hash table pointer
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = hash table
}
