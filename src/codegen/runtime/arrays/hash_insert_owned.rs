use crate::codegen::emit::Emitter;

/// hash_insert_owned: insert a key-value pair whose key/value ownership already
/// belongs to the destination table. Used by hash_grow when moving entries.
/// Input:  x0=hash_table_ptr, x1=key_ptr, x2=key_len, x3=value_lo, x4=value_hi
/// Output: x0=hash_table_ptr
pub fn emit_hash_insert_owned(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_insert_owned ---");
    emitter.label("__rt_hash_insert_owned");

    // -- set up stack frame, save all inputs --
    // Stack layout:
    //   [sp, #0]  = hash_table_ptr
    //   [sp, #8]  = key_ptr
    //   [sp, #16] = key_len
    //   [sp, #24] = value_lo
    //   [sp, #32] = value_hi
    //   [sp, #40] = probe index
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

    // -- hash the existing owned key --
    emitter.instruction("bl __rt_hash_fnv1a");                                  // compute hash of the moved key

    // -- compute slot index: hash % capacity --
    emitter.instruction("ldr x5, [sp, #0]");                                    // reload destination table pointer
    emitter.instruction("ldr x6, [x5, #8]");                                    // load table capacity
    emitter.instruction("udiv x7, x0, x6");                                     // divide hash by capacity
    emitter.instruction("msub x8, x7, x6, x0");                                 // compute hash % capacity
    emitter.instruction("str x8, [sp, #40]");                                   // save initial probe index
    emitter.instruction("mov x10, #0");                                         // probe count = 0

    // -- linear probe until we find an empty slot --
    emitter.label("__rt_hash_insert_owned_probe");
    emitter.instruction("cmp x10, x6");                                         // have we probed every slot?
    emitter.instruction("b.ge __rt_hash_insert_owned_done");                    // stop if the table is unexpectedly full

    emitter.instruction("ldr x9, [sp, #40]");                                   // reload current probe index
    emitter.instruction("mov x11, #40");                                        // entry size = 40 bytes
    emitter.instruction("mul x12, x9, x11");                                    // compute byte offset for this slot
    emitter.instruction("add x12, x5, x12");                                    // advance from table base to slot
    emitter.instruction("add x12, x12, #24");                                   // skip hash header to entry storage
    emitter.instruction("ldr x13, [x12]");                                      // load occupied flag
    emitter.instruction("cmp x13, #1");                                         // is the slot already occupied?
    emitter.instruction("b.ne __rt_hash_insert_owned_write");                   // empty or tombstone means we can write here

    // -- occupied: compare keys so duplicate keys still overwrite safely --
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload incoming key pointer
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload incoming key length
    emitter.instruction("ldr x3, [x12, #8]");                                   // load stored key pointer
    emitter.instruction("ldr x4, [x12, #16]");                                  // load stored key length
    emitter.instruction("bl __rt_str_eq");                                      // compare moved key against existing key

    emitter.instruction("ldr x5, [sp, #0]");                                    // reload destination table after call clobbers regs
    emitter.instruction("ldr x6, [x5, #8]");                                    // reload capacity after call clobbers regs
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload probe index after call clobbers regs
    emitter.instruction("mov x11, #40");                                        // entry size = 40 bytes
    emitter.instruction("mul x12, x9, x11");                                    // recompute byte offset for this slot
    emitter.instruction("add x12, x5, x12");                                    // advance from table base to slot
    emitter.instruction("add x12, x12, #24");                                   // skip hash header to entry storage
    emitter.instruction("cbnz x0, __rt_hash_insert_owned_overwrite");           // overwrite if this key already exists

    // -- advance to the next probe slot --
    emitter.instruction("add x9, x9, #1");                                      // increment probe index
    emitter.instruction("udiv x7, x9, x6");                                     // divide updated index by capacity
    emitter.instruction("msub x9, x7, x6, x9");                                 // wrap index with modulo capacity
    emitter.instruction("str x9, [sp, #40]");                                   // save wrapped probe index
    emitter.instruction("add x10, x10, #1");                                    // increment probe count
    emitter.instruction("b __rt_hash_insert_owned_probe");                      // continue probing

    // -- write a fresh entry into an empty/tombstone slot --
    emitter.label("__rt_hash_insert_owned_write");
    emitter.instruction("mov x13, #1");                                         // mark slot as occupied
    emitter.instruction("str x13, [x12]");                                      // store occupied flag
    emitter.instruction("ldr x13, [sp, #8]");                                   // reload moved key pointer
    emitter.instruction("str x13, [x12, #8]");                                  // store key pointer in slot
    emitter.instruction("ldr x13, [sp, #16]");                                  // reload moved key length
    emitter.instruction("str x13, [x12, #16]");                                 // store key length in slot
    emitter.instruction("ldr x13, [sp, #24]");                                  // reload moved value_lo
    emitter.instruction("str x13, [x12, #24]");                                 // store value_lo in slot
    emitter.instruction("ldr x13, [sp, #32]");                                  // reload moved value_hi
    emitter.instruction("str x13, [x12, #32]");                                 // store value_hi in slot
    emitter.instruction("ldr x5, [sp, #0]");                                    // reload destination table pointer
    emitter.instruction("ldr x13, [x5]");                                       // load current entry count
    emitter.instruction("add x13, x13, #1");                                    // count the newly inserted slot
    emitter.instruction("str x13, [x5]");                                       // store updated entry count
    emitter.instruction("b __rt_hash_insert_owned_done");                       // insertion is complete

    // -- duplicate key during grow: overwrite value in place --
    emitter.label("__rt_hash_insert_owned_overwrite");
    emitter.instruction("ldr x13, [sp, #24]");                                  // reload moved value_lo
    emitter.instruction("str x13, [x12, #24]");                                 // overwrite value_lo in existing slot
    emitter.instruction("ldr x13, [sp, #32]");                                  // reload moved value_hi
    emitter.instruction("str x13, [x12, #32]");                                 // overwrite value_hi in existing slot

    emitter.label("__rt_hash_insert_owned_done");
    emitter.instruction("ldr x0, [sp, #0]");                                    // return destination table pointer
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
