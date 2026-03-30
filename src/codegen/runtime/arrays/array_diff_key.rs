use crate::codegen::emit::Emitter;

/// array_diff_key: return entries from hash1 whose keys are NOT in hash2.
/// Input:  x0=hash1, x1=hash2
/// Output: x0=new hash table with entries from hash1 not found in hash2
pub fn emit_array_diff_key(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_diff_key ---");
    emitter.label("__rt_array_diff_key");

    // -- set up stack frame, save arguments --
    // Stack layout:
    //   [sp, #0]  = hash1 pointer
    //   [sp, #8]  = hash2 pointer
    //   [sp, #16] = result hash table pointer
    //   [sp, #24] = iterator cursor
    //   [sp, #32] = saved x29
    //   [sp, #40] = saved x30
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save hash1 pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save hash2 pointer

    // -- create result hash table with same capacity as hash1 --
    emitter.instruction("ldr x0, [x0, #8]");                                    // x0 = hash1 capacity
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload hash1 pointer
    emitter.instruction("ldr x1, [x9, #16]");                                   // x1 = hash1 value_type
    emitter.instruction("bl __rt_hash_new");                                    // create result hash table, x0 = result ptr
    emitter.instruction("str x0, [sp, #16]");                                   // save result hash table pointer

    // -- iterate over hash1 entries --
    emitter.instruction("str xzr, [sp, #24]");                                  // iterator cursor = 0 (start from hash header head)

    emitter.label("__rt_array_diff_key_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // x0 = hash1 pointer
    emitter.instruction("ldr x1, [sp, #24]");                                   // x1 = current iterator cursor
    emitter.instruction("bl __rt_hash_iter_next");                              // get next entry, x0=next_cursor, x1=key_ptr, x2=key_len, x3=val_lo, x4=val_hi, x5=val_tag

    // -- check if iteration is done --
    emitter.instruction("cmn x0, #1");                                          // check if x0 == -1 (end of iteration)
    emitter.instruction("b.eq __rt_array_diff_key_done");                       // if done, return result

    // -- save iterator state and entry data --
    emitter.instruction("str x0, [sp, #24]");                                   // save next iterator cursor
                                              // x1=key_ptr, x2=key_len, x3=val_lo, x4=val_hi are live

    // -- check if this key exists in hash2 --
    // Save entry values on stack temporarily (reuse existing slots carefully)
    emitter.instruction("sub sp, sp, #48");                                     // allocate temp space for entry data and value_tag
    emitter.instruction("str x1, [sp, #0]");                                    // save key_ptr
    emitter.instruction("str x2, [sp, #8]");                                    // save key_len
    emitter.instruction("str x3, [sp, #16]");                                   // save value_lo
    emitter.instruction("str x4, [sp, #24]");                                   // save value_hi
    emitter.instruction("str x5, [sp, #32]");                                   // save value_tag

    emitter.instruction("ldr x0, [sp, #56]");                                   // load hash2 pointer (sp+8 shifted by 48)
                                              // x1=key_ptr, x2=key_len already set
    emitter.instruction("bl __rt_hash_get");                                    // check if key exists in hash2, x0=found

    // -- if key NOT found in hash2, add to result --
    emitter.instruction("cbnz x0, __rt_array_diff_key_skip");                   // if found in hash2, skip this entry

    // -- copied hash values stay borrowed until we retain them for the result --
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload this entry's runtime value_tag
    emitter.instruction("cmp x9, #1");                                          // is the borrowed value a string?
    emitter.instruction("b.eq __rt_array_diff_key_retain");                     // strings need retain via the uniform dispatcher
    emitter.instruction("cmp x9, #4");                                          // is the borrowed value heap-backed?
    emitter.instruction("b.lt __rt_array_diff_key_copy");                       // scalar values need no retain
    emitter.instruction("cmp x9, #7");                                          // do heap-backed tags stay within range?
    emitter.instruction("b.gt __rt_array_diff_key_copy");                       // unknown tags are ignored here
    emitter.label("__rt_array_diff_key_retain");
    emitter.instruction("ldr x0, [sp, #16]");                                   // load borrowed heap pointer from saved value_lo
    emitter.instruction("bl __rt_incref");                                      // retain copied heap value for the result hash

    // -- key not in hash2: add to result hash table --
    emitter.label("__rt_array_diff_key_copy");
    emitter.instruction("ldr x0, [sp, #64]");                                   // load result hash table (sp+16 shifted by 48)
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload key_ptr
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload key_len
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload value_lo
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload value_hi
    emitter.instruction("ldr x5, [sp, #32]");                                   // reload value_tag
    emitter.instruction("bl __rt_hash_set");                                    // insert into result hash table
    emitter.instruction("str x0, [sp, #64]");                                   // update result hash table pointer after possible growth

    emitter.label("__rt_array_diff_key_skip");
    emitter.instruction("add sp, sp, #48");                                     // deallocate temp space
    emitter.instruction("b __rt_array_diff_key_loop");                          // continue iterating

    // -- return result hash table --
    emitter.label("__rt_array_diff_key_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // x0 = result hash table pointer
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = result hash table
}
