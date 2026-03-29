use crate::codegen::emit::Emitter;

/// array_combine_refcounted: create an associative array from string keys and refcounted values.
/// Input:  x0=keys_array (string array), x1=values_array (refcounted payload array)
/// Output: x0=new hash table
pub fn emit_array_combine_refcounted(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_combine_refcounted ---");
    emitter.label("__rt_array_combine_refcounted");

    // -- set up stack frame, save arguments --
    emitter.instruction("sub sp, sp, #80");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save keys array pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save values array pointer
    emitter.instruction("str x2, [sp, #32]");                                   // save result value_type tag

    // -- create hash table with capacity = max(length * 2, 16) --
    emitter.instruction("ldr x0, [x0]");                                        // load keys array length
    emitter.instruction("lsl x0, x0, #1");                                      // compute capacity with headroom
    emitter.instruction("mov x9, #16");                                         // load minimum capacity
    emitter.instruction("cmp x0, x9");                                          // compare requested capacity with minimum
    emitter.instruction("csel x0, x9, x0, lt");                                 // clamp capacity to at least 16
    emitter.instruction("ldr x1, [sp, #32]");                                   // x1 = requested result value_type tag
    emitter.instruction("bl __rt_hash_new");                                    // allocate result hash table
    emitter.instruction("str x0, [sp, #16]");                                   // save result hash pointer
    emitter.instruction("str xzr, [sp, #24]");                                  // initialize loop index

    emitter.label("__rt_array_combine_ref_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload keys array pointer
    emitter.instruction("ldr x3, [x0]");                                        // load keys array length
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload loop index
    emitter.instruction("cmp x4, x3");                                          // compare loop index with key count
    emitter.instruction("b.ge __rt_array_combine_ref_done");                    // finish once every key/value pair has been inserted
    emitter.instruction("lsl x5, x4, #4");                                      // compute byte offset for string key element
    emitter.instruction("add x5, x0, x5");                                      // move to keyed string element
    emitter.instruction("add x5, x5, #24");                                     // skip array header
    emitter.instruction("ldr x1, [x5]");                                        // load key pointer
    emitter.instruction("ldr x2, [x5, #8]");                                    // load key length
    emitter.instruction("str x1, [sp, #40]");                                   // preserve key pointer across incref
    emitter.instruction("str x2, [sp, #48]");                                   // preserve key length across incref
    emitter.instruction("ldr x5, [sp, #8]");                                    // reload values array pointer
    emitter.instruction("add x5, x5, #24");                                     // compute values data base
    emitter.instruction("ldr x3, [x5, x4, lsl #3]");                            // load borrowed refcounted value
    emitter.instruction("str x3, [sp, #56]");                                   // preserve borrowed value across incref
    emitter.instruction("mov x0, x3");                                          // move borrowed value into incref argument register
    emitter.instruction("bl __rt_incref");                                      // retain borrowed value before hash_set takes ownership
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload result hash pointer
    emitter.instruction("ldr x1, [sp, #40]");                                   // restore key pointer
    emitter.instruction("ldr x2, [sp, #48]");                                   // restore key length
    emitter.instruction("ldr x3, [sp, #56]");                                   // restore retained value pointer
    emitter.instruction("mov x4, #0");                                          // value_hi unused for 8-byte refcounted payloads
    emitter.instruction("bl __rt_hash_set");                                    // insert retained value into the result hash
    emitter.instruction("str x0, [sp, #16]");                                   // persist hash pointer after possible growth
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload loop index
    emitter.instruction("add x4, x4, #1");                                      // increment loop index
    emitter.instruction("str x4, [sp, #24]");                                   // persist updated loop index
    emitter.instruction("b __rt_array_combine_ref_loop");                       // continue combining

    emitter.label("__rt_array_combine_ref_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload result hash pointer
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return result hash
}
