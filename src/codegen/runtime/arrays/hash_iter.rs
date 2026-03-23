use crate::codegen::emit::Emitter;

/// hash_iter_next: iterate over hash table entries.
/// Input:  x0=hash_table_ptr, x1=current_index (start with 0)
/// Output: x0=new_index (or -1 if done), x1=key_ptr, x2=key_len, x3=value_lo, x4=value_hi
pub fn emit_hash_iter(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_iter_next ---");
    emitter.label("__rt_hash_iter_next");

    // -- load capacity from header --
    emitter.instruction("ldr x5, [x0, #8]");                                    // x5 = capacity from header

    // -- scan forward from current_index for next occupied entry --
    emitter.label("__rt_hash_iter_scan");
    emitter.instruction("cmp x1, x5");                                          // check if index >= capacity
    emitter.instruction("b.ge __rt_hash_iter_end");                             // if past end, iteration is done

    // -- compute entry address: base + 24 + index * 40 --
    emitter.instruction("mov x6, #40");                                         // entry size = 40 bytes
    emitter.instruction("mul x7, x1, x6");                                      // x7 = index * 40
    emitter.instruction("add x7, x0, x7");                                      // x7 = table_ptr + index * 40
    emitter.instruction("add x7, x7, #24");                                     // x7 = entry address (skip header)

    // -- check if this entry is occupied --
    emitter.instruction("ldr x8, [x7]");                                        // x8 = occupied flag
    emitter.instruction("cmp x8, #1");                                          // check if occupied
    emitter.instruction("b.ne __rt_hash_iter_skip");                            // if not occupied, skip this entry

    // -- entry is occupied: return its data --
    emitter.instruction("add x0, x1, #1");                                      // x0 = next index (for subsequent call)
    emitter.instruction("ldr x1, [x7, #8]");                                    // x1 = key_ptr
    emitter.instruction("ldr x2, [x7, #16]");                                   // x2 = key_len
    emitter.instruction("ldr x3, [x7, #24]");                                   // x3 = value_lo
    emitter.instruction("ldr x4, [x7, #32]");                                   // x4 = value_hi
    emitter.instruction("ret");                                                 // return to caller

    // -- skip non-occupied entry and try next --
    emitter.label("__rt_hash_iter_skip");
    emitter.instruction("add x1, x1, #1");                                      // index += 1
    emitter.instruction("b __rt_hash_iter_scan");                               // check next entry

    // -- no more entries --
    emitter.label("__rt_hash_iter_end");
    emitter.instruction("mov x0, #-1");                                         // return -1 to signal end of iteration
    emitter.instruction("ret");                                                 // return to caller
}
