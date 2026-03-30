use crate::codegen::emit::Emitter;

/// hash_iter_next: iterate over hash table entries in insertion order.
/// Input:  x0=hash_table_ptr, x1=cursor (start with 0)
/// Output: x0=next_cursor (or -1 if done), x1=key_ptr, x2=key_len, x3=value_lo, x4=value_hi
pub fn emit_hash_iter(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_iter_next ---");
    emitter.label("__rt_hash_iter_next");

    // -- cursor protocol --
    // 0   = start from header.head
    // >0  = slot index + 1 of the next entry to return
    // -2  = post-last cursor returned with the final yielded entry
    // -1  = no more entries
    emitter.instruction("cmp x1, #-1");                                         // has the caller already consumed the end sentinel?
    emitter.instruction("b.eq __rt_hash_iter_end");                              // repeated end probes stay at done
    emitter.instruction("cmp x1, #-2");                                         // was the previous yielded entry the tail?
    emitter.instruction("b.eq __rt_hash_iter_end");                              // convert the post-last cursor into the final done signal
    emitter.instruction("cbnz x1, __rt_hash_iter_resume");                       // non-zero cursors already encode the next slot to return

    // -- start a fresh insertion-order walk from the head slot --
    emitter.instruction("ldr x6, [x0, #24]");                                    // x6 = head slot index from the hash header
    emitter.instruction("cmp x6, #-1");                                          // does the hash contain any entries?
    emitter.instruction("b.eq __rt_hash_iter_end");                              // empty hashes are immediately done
    emitter.instruction("b __rt_hash_iter_entry");                               // load and return the head entry

    // -- resume iteration from the encoded next slot --
    emitter.label("__rt_hash_iter_resume");
    emitter.instruction("sub x6, x1, #1");                                       // decode slot index = cursor - 1

    // -- compute entry address: base + 40 + index * 56 --
    emitter.label("__rt_hash_iter_entry");
    emitter.instruction("mov x7, #56");                                          // x7 = hash entry size in bytes
    emitter.instruction("mul x8, x6, x7");                                       // x8 = slot index * 56
    emitter.instruction("add x8, x0, x8");                                       // advance from the hash base to the selected slot
    emitter.instruction("add x8, x8, #40");                                      // skip the 40-byte hash header

    // -- return the selected entry and encode the next cursor --
    emitter.instruction("ldr x9, [x8, #48]");                                    // x9 = next slot index from the insertion-order chain
    emitter.instruction("cmp x9, #-1");                                          // is this the tail entry?
    emitter.instruction("b.eq __rt_hash_iter_tail");                             // tail entries return the post-last cursor
    emitter.instruction("add x0, x9, #1");                                       // x0 = next cursor (slot index + 1)
    emitter.instruction("b __rt_hash_iter_return");                              // emit the current entry payload
    emitter.label("__rt_hash_iter_tail");
    emitter.instruction("mov x0, #-2");                                          // x0 = post-last cursor for the next probe
    emitter.label("__rt_hash_iter_return");
    emitter.instruction("ldr x1, [x8, #8]");                                     // x1 = key_ptr
    emitter.instruction("ldr x2, [x8, #16]");                                    // x2 = key_len
    emitter.instruction("ldr x3, [x8, #24]");                                    // x3 = value_lo
    emitter.instruction("ldr x4, [x8, #32]");                                    // x4 = value_hi
    emitter.instruction("ret");                                                  // return the current entry payload

    // -- no more entries --
    emitter.label("__rt_hash_iter_end");
    emitter.instruction("mov x0, #-1");                                         // return -1 to signal end of iteration
    emitter.instruction("ret");                                                 // return to caller
}
