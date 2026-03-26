use crate::codegen::emit::Emitter;

/// hash_set: insert or update a key-value pair in the hash table.
/// Input:  x0=hash_table_ptr, x1=key_ptr, x2=key_len, x3=value_lo, x4=value_hi
/// Output: none (modifies table in place)
pub fn emit_hash_set(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_set ---");
    emitter.label("__rt_hash_set");

    // -- set up stack frame, save all inputs --
    // Stack layout:
    //   [sp, #0]  = hash_table_ptr (x0)
    //   [sp, #8]  = key_ptr (x1)
    //   [sp, #16] = key_len (x2)
    //   [sp, #24] = value_lo (x3)
    //   [sp, #32] = value_hi (x4)
    //   [sp, #40] = probe index (saved across calls)
    //   [sp, #48] = saved x29
    //   [sp, #56] = saved x30
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save hash_table_ptr
    emitter.instruction("str x1, [sp, #8]");                                    // save key_ptr
    emitter.instruction("str x2, [sp, #16]");                                   // save key_len
    emitter.instruction("str x3, [sp, #24]");                                   // save value_lo
    emitter.instruction("str x4, [sp, #32]");                                   // save value_hi

    // -- copy the key to persistent storage on heap --
    // x1=ptr, x2=len already set from inputs
    emitter.instruction("bl __rt_str_persist");                                 // copy key to heap, x1=new_ptr, x2=len
    emitter.instruction("str x1, [sp, #8]");                                    // update key_ptr to persistent copy
    // x2 (key_len) unchanged

    // -- hash the key --
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload persistent key_ptr
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload key_len
    emitter.instruction("bl __rt_hash_fnv1a");                                  // compute hash, result in x0

    // -- compute slot index: hash % capacity --
    emitter.instruction("ldr x5, [sp, #0]");                                    // reload hash_table_ptr
    emitter.instruction("ldr x6, [x5, #8]");                                    // x6 = capacity from header
    emitter.instruction("udiv x7, x0, x6");                                     // x7 = hash / capacity
    emitter.instruction("msub x8, x7, x6, x0");                                 // x8 = hash - (hash/capacity)*capacity = hash % capacity

    // -- linear probe loop --
    emitter.instruction("str x8, [sp, #40]");                                   // save initial probe index
    emitter.instruction("mov x10, #0");                                         // x10 = probe count (to detect full table)

    emitter.label("__rt_hash_set_probe");
    emitter.instruction("cmp x10, x6");                                         // check if we've probed all slots
    emitter.instruction("b.ge __rt_hash_set_done");                             // if probed all, table is full (shouldn't happen)

    // -- compute entry address: base + 24 + index * 40 --
    emitter.instruction("ldr x9, [sp, #40]");                                   // load current probe index
    emitter.instruction("mov x11, #40");                                        // entry size = 40 bytes
    emitter.instruction("mul x12, x9, x11");                                    // x12 = index * 40
    emitter.instruction("add x12, x5, x12");                                    // x12 = table_ptr + index * 40
    emitter.instruction("add x12, x12, #24");                                   // x12 = entry address (skip 24-byte header)

    // -- check occupied field --
    emitter.instruction("ldr x13, [x12]");                                      // x13 = occupied flag of this entry
    emitter.instruction("cmp x13, #1");                                         // check if slot is occupied
    emitter.instruction("b.ne __rt_hash_set_insert");                           // if empty (0) or tombstone (2), insert here

    // -- slot is occupied: compare keys to check for update --
    emitter.instruction("ldr x1, [sp, #8]");                                    // x1 = our key_ptr
    emitter.instruction("ldr x2, [sp, #16]");                                   // x2 = our key_len
    emitter.instruction("ldr x3, [x12, #8]");                                   // x3 = existing key_ptr in entry
    emitter.instruction("ldr x4, [x12, #16]");                                  // x4 = existing key_len in entry
    emitter.instruction("bl __rt_str_eq");                                      // compare keys, x0=1 if equal

    // -- restore probe state after str_eq --
    emitter.instruction("ldr x5, [sp, #0]");                                    // reload hash_table_ptr
    emitter.instruction("ldr x6, [x5, #8]");                                    // reload capacity
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload probe index

    // -- recompute entry address after call clobbered registers --
    emitter.instruction("mov x11, #40");                                        // entry size = 40 bytes
    emitter.instruction("mul x12, x9, x11");                                    // x12 = index * 40
    emitter.instruction("add x12, x5, x12");                                    // x12 = table_ptr + index * 40
    emitter.instruction("add x12, x12, #24");                                   // x12 = entry address

    emitter.instruction("cbnz x0, __rt_hash_set_update");                       // if keys match, update existing entry

    // -- keys don't match, advance to next slot --
    emitter.instruction("add x9, x9, #1");                                      // index += 1
    emitter.instruction("udiv x7, x9, x6");                                     // x7 = index / capacity
    emitter.instruction("msub x9, x7, x6, x9");                                 // x9 = index % capacity (wrap around)
    emitter.instruction("str x9, [sp, #40]");                                   // save updated probe index

    // -- increment probe count and continue --
    emitter.instruction("add x10, x10, #1");                                    // probe_count += 1
    emitter.instruction("b __rt_hash_set_probe");                               // try next slot

    // -- insert into empty/tombstone slot --
    emitter.label("__rt_hash_set_insert");
    emitter.instruction("mov x13, #1");                                         // occupied = 1
    emitter.instruction("str x13, [x12]");                                      // set entry as occupied
    emitter.instruction("ldr x13, [sp, #8]");                                   // load persistent key_ptr
    emitter.instruction("str x13, [x12, #8]");                                  // store key_ptr in entry
    emitter.instruction("ldr x13, [sp, #16]");                                  // load key_len
    emitter.instruction("str x13, [x12, #16]");                                 // store key_len in entry
    emitter.instruction("ldr x13, [sp, #24]");                                  // load value_lo
    emitter.instruction("str x13, [x12, #24]");                                 // store value_lo in entry
    emitter.instruction("ldr x13, [sp, #32]");                                  // load value_hi
    emitter.instruction("str x13, [x12, #32]");                                 // store value_hi in entry

    // -- increment count --
    emitter.instruction("ldr x5, [sp, #0]");                                    // reload hash_table_ptr
    emitter.instruction("ldr x13, [x5]");                                       // load current count
    emitter.instruction("add x13, x13, #1");                                    // count += 1
    emitter.instruction("str x13, [x5]");                                       // store updated count
    emitter.instruction("b __rt_hash_set_done");                                // done inserting

    // -- update existing entry's value --
    emitter.label("__rt_hash_set_update");
    emitter.instruction("ldr x13, [sp, #24]");                                  // load value_lo
    emitter.instruction("str x13, [x12, #24]");                                 // update value_lo in entry
    emitter.instruction("ldr x13, [sp, #32]");                                  // load value_hi
    emitter.instruction("str x13, [x12, #32]");                                 // update value_hi in entry

    // -- tear down stack frame and return --
    emitter.label("__rt_hash_set_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
