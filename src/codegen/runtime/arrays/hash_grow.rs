use crate::codegen::emit::Emitter;

/// hash_grow: double the capacity of a hash table while preserving insertion order.
/// Allocates a new table with 2x capacity, reinserts all owned entries in their
/// original insertion sequence, frees the old table struct, and returns the new pointer.
/// Input:  x0 = old hash table pointer
/// Output: x0 = new hash table pointer (with doubled capacity)
pub fn emit_hash_grow(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_grow ---");
    emitter.label_global("__rt_hash_grow");

    // -- set up stack frame --
    // Stack layout:
    //   [sp, #0]  = insertion-order iterator cursor
    //   [sp, #32] = saved x19 (callee-saved)
    //   [sp, #40] = saved x20 (callee-saved)
    //   [sp, #48] = saved x29
    //   [sp, #56] = saved x30
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up new frame pointer
    emitter.instruction("stp x19, x20, [sp, #32]");                             // save callee-saved registers
    emitter.instruction("bl __rt_hash_ensure_unique");                          // split shared hash tables before rehashing into new storage
    emitter.instruction("mov x20, x0");                                         // x20 = unique old table pointer

    // -- read old table header --
    emitter.instruction("ldr x9, [x20, #8]");                                   // x9 = old capacity
    emitter.instruction("ldr x1, [x20, #16]");                                  // x1 = runtime value_type

    // -- create new table with 2x capacity --
    emitter.instruction("lsl x0, x9, #1");                                      // x0 = old_capacity * 2
                                           // x1 = value_type (already set)
    emitter.instruction("bl __rt_hash_new");                                    // allocate new table → x0
    emitter.instruction("mov x19, x0");                                         // x19 = new table (callee-saved)

    // -- iterate old entries in insertion order and reinsert them --
    emitter.instruction("str xzr, [sp, #0]");                                   // iterator cursor = 0 (start from header.head)

    emitter.label("__rt_hash_grow_loop");
    emitter.instruction("mov x0, x20");                                         // x0 = old table pointer
    emitter.instruction("ldr x1, [sp, #0]");                                    // x1 = current insertion-order cursor
    emitter.instruction("bl __rt_hash_iter_next");                              // get next owned entry in insertion order, including its per-entry value tag
    emitter.instruction("cmn x0, #1");                                          // did the iterator signal end-of-walk?
    emitter.instruction("b.eq __rt_hash_grow_free");                            // finish once every entry has been moved
    emitter.instruction("str x0, [sp, #0]");                                    // save the next insertion-order cursor
    emitter.instruction("mov x0, x19");                                         // x0 = destination table
    emitter.instruction("bl __rt_hash_insert_owned");                           // rehash and move existing key/value ownership with the original per-entry tag
    emitter.instruction("mov x19, x0");                                         // update new table ptr (hash_set returns it)
    emitter.instruction("b __rt_hash_grow_loop");                               // continue iterating

    // -- free old table --
    emitter.label("__rt_hash_grow_free");
    emitter.instruction("mov x0, x20");                                         // old table pointer
    emitter.instruction("bl __rt_heap_free");                                   // free old table

    // -- return new table pointer --
    emitter.instruction("mov x0, x19");                                         // x0 = new table pointer

    // -- restore frame and return --
    emitter.instruction("ldp x19, x20, [sp, #32]");                             // restore callee-saved registers
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = new table
}
