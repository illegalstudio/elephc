use crate::codegen::emit::Emitter;

/// hash_new: create a new hash table on the heap.
/// Input:  x0=initial_capacity, x1=value_type_tag
///         (0=int, 1=str, 2=float, 3=bool, 4=array, 5=assoc, 6=object)
/// Output: x0=pointer to hash table
/// Layout: [count:8][capacity:8][value_type:8][head:8][tail:8][entries...]
///         where each entry is 56 bytes:
///         [occupied:8][key_ptr:8][key_len:8][value_lo:8][value_hi:8][prev:8][next:8]
pub fn emit_hash_new(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_new ---");
    emitter.label("__rt_hash_new");

    // -- set up stack frame, save arguments --
    emitter.instruction("sub sp, sp, #32");                                     // allocate 32 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save capacity to stack
    emitter.instruction("str x1, [sp, #8]");                                    // save value_type to stack

    // -- calculate total size: 40 + capacity * 56 --
    emitter.instruction("mov x9, #56");                                         // entry size = 56 bytes with insertion-order links
    emitter.instruction("mul x2, x0, x9");                                      // x2 = capacity * 56 = entries region size
    emitter.instruction("add x0, x2, #40");                                     // x0 = total size (40-byte header + entries)
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate memory, x0 = pointer to hash table
    emitter.instruction("mov x9, #3");                                          // heap kind 3 = associative array / hash table
    emitter.instruction("mov x10, #0x8000");                                    // bit 15 marks heap containers that participate in copy-on-write
    emitter.instruction("orr x9, x9, x10");                                     // preserve the persistent copy-on-write container flag in the kind word
    emitter.instruction("str x9, [x0, #-8]");                                   // store hash-table kind in the uniform heap header

    // -- initialize header fields --
    emitter.instruction("str xzr, [x0]");                                       // header[0]: count = 0
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload capacity from stack
    emitter.instruction("str x9, [x0, #8]");                                    // header[8]: capacity
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload value_type from stack
    emitter.instruction("str x10, [x0, #16]");                                  // header[16]: value_type
    emitter.instruction("mov x15, #-1");                                        // sentinel index for an empty insertion-order chain
    emitter.instruction("str x15, [x0, #24]");                                  // header[24]: head = none
    emitter.instruction("str x15, [x0, #32]");                                  // header[32]: tail = none

    // -- zero all entry slots (set occupied=0 for each entry) --
    emitter.instruction("add x11, x0, #40");                                    // x11 = base of entries region after the extended header
    emitter.instruction("mov x12, #56");                                        // x12 = entry size
    emitter.instruction("mul x13, x9, x12");                                    // x13 = total bytes in entries region
    emitter.instruction("add x14, x11, x13");                                   // x14 = end of entries region

    emitter.label("__rt_hash_new_zero");
    emitter.instruction("cmp x11, x14");                                        // check if we've reached end of entries
    emitter.instruction("b.ge __rt_hash_new_done");                             // if past end, zeroing is complete
    emitter.instruction("str xzr, [x11]");                                      // set occupied field to 0 (empty)
    emitter.instruction("add x11, x11, #56");                                   // advance to next entry
    emitter.instruction("b __rt_hash_new_zero");                                // continue zeroing

    // -- tear down stack frame and return --
    emitter.label("__rt_hash_new_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = hash table pointer
}
