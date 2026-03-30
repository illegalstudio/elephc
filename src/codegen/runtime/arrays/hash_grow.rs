use crate::codegen::emit::Emitter;

/// hash_grow: double the capacity of a hash table, moving all owned entries.
/// Allocates a new table with 2x capacity, reinserts all occupied entries
/// without duplicating owned keys/values, frees the old table struct, and
/// returns the new pointer.
/// Input:  x0 = old hash table pointer
/// Output: x0 = new hash table pointer (with doubled capacity)
pub fn emit_hash_grow(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_grow ---");
    emitter.label("__rt_hash_grow");

    // -- set up stack frame --
    // Stack layout:
    //   [sp, #0]  = old table pointer
    //   [sp, #8]  = new table pointer
    //   [sp, #16] = old capacity
    //   [sp, #24] = loop index
    //   [sp, #32] = saved x19 (callee-saved)
    //   [sp, #40] = saved x20 (callee-saved)
    //   [sp, #48] = saved x29
    //   [sp, #56] = saved x30
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up new frame pointer
    emitter.instruction("stp x19, x20, [sp, #32]");                             // save callee-saved registers
    emitter.instruction("bl __rt_hash_ensure_unique");                           // split shared hash tables before rehashing into new storage
    emitter.instruction("str x0, [sp, #0]");                                    // save the unique old table pointer

    // -- read old table header --
    emitter.instruction("ldr x9, [x0, #8]");                                    // x9 = old capacity
    emitter.instruction("str x9, [sp, #16]");                                   // save old capacity
    emitter.instruction("ldr x1, [x0, #16]");                                   // x1 = value_type

    // -- create new table with 2x capacity --
    emitter.instruction("lsl x0, x9, #1");                                      // x0 = old_capacity * 2
                                           // x1 = value_type (already set)
    emitter.instruction("bl __rt_hash_new");                                    // allocate new table → x0
    emitter.instruction("str x0, [sp, #8]");                                    // save new table pointer
    emitter.instruction("mov x19, x0");                                         // x19 = new table (callee-saved)

    // -- iterate old entries and reinsert occupied ones --
    emitter.instruction("mov x20, #0");                                         // x20 = loop index = 0
    emitter.instruction("str x20, [sp, #24]");                                  // save loop index

    emitter.label("__rt_hash_grow_loop");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload old capacity
    emitter.instruction("cmp x20, x9");                                         // index >= old capacity?
    emitter.instruction("b.ge __rt_hash_grow_free");                            // done iterating

    // -- compute old entry address: old_table + 24 + index * 40 --
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload old table pointer
    emitter.instruction("mov x11, #40");                                        // entry size
    emitter.instruction("mul x12, x20, x11");                                   // index * 40
    emitter.instruction("add x12, x10, x12");                                   // old_table + index * 40
    emitter.instruction("add x12, x12, #24");                                   // skip header → entry address

    // -- check if entry is occupied --
    emitter.instruction("ldr x13, [x12]");                                      // occupied flag
    emitter.instruction("cmp x13, #1");                                         // is it occupied?
    emitter.instruction("b.ne __rt_hash_grow_next");                            // skip if empty or tombstone

    // -- read key and value from old entry --
    emitter.instruction("mov x0, x19");                                         // x0 = new table
    emitter.instruction("ldr x1, [x12, #8]");                                   // x1 = key_ptr
    emitter.instruction("ldr x2, [x12, #16]");                                  // x2 = key_len
    emitter.instruction("ldr x3, [x12, #24]");                                  // x3 = value_lo
    emitter.instruction("ldr x4, [x12, #32]");                                  // x4 = value_hi

    // -- move owned entry into the new table without duplicating ownership --
    emitter.instruction("bl __rt_hash_insert_owned");                           // rehash and move existing key/value ownership
    emitter.instruction("mov x19, x0");                                         // update new table ptr (hash_set returns it)

    emitter.label("__rt_hash_grow_next");
    emitter.instruction("add x20, x20, #1");                                    // index += 1
    emitter.instruction("b __rt_hash_grow_loop");                               // continue iterating

    // -- free old table --
    emitter.label("__rt_hash_grow_free");
    emitter.instruction("ldr x0, [sp, #0]");                                    // old table pointer
    emitter.instruction("bl __rt_heap_free");                                   // free old table

    // -- return new table pointer --
    emitter.instruction("mov x0, x19");                                         // x0 = new table pointer

    // -- restore frame and return --
    emitter.instruction("ldp x19, x20, [sp, #32]");                             // restore callee-saved registers
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = new table
}
