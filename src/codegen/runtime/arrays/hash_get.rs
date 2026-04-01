use crate::codegen::emit::Emitter;

/// hash_get: look up a value by string key in the hash table.
/// Input:  x0=hash_table_ptr, x1=key_ptr, x2=key_len
/// Output: x0=found (1 or 0), x1=value_lo, x2=value_hi, x3=value_tag
pub fn emit_hash_get(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_get ---");
    emitter.label_global("__rt_hash_get");

    // -- set up stack frame, save inputs --
    // Stack layout:
    //   [sp, #0]  = hash_table_ptr
    //   [sp, #8]  = key_ptr
    //   [sp, #16] = key_len
    //   [sp, #24] = current probe index
    //   [sp, #32] = probe count
    //   [sp, #40] = saved x29
    //   [sp, #48] = saved x30
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save hash_table_ptr
    emitter.instruction("str x1, [sp, #8]");                                    // save key_ptr
    emitter.instruction("str x2, [sp, #16]");                                   // save key_len

    // -- hash the key --
    emitter.instruction("bl __rt_hash_fnv1a");                                  // compute hash, result in x0

    // -- compute slot index: hash % capacity --
    emitter.instruction("ldr x5, [sp, #0]");                                    // reload hash_table_ptr
    emitter.instruction("ldr x6, [x5, #8]");                                    // x6 = capacity from header
    emitter.instruction("udiv x7, x0, x6");                                     // x7 = hash / capacity
    emitter.instruction("msub x8, x7, x6, x0");                                 // x8 = hash % capacity
    emitter.instruction("str x8, [sp, #24]");                                   // save initial probe index
    emitter.instruction("str xzr, [sp, #32]");                                  // probe count = 0

    // -- linear probe loop --
    emitter.label("__rt_hash_get_probe");
    emitter.instruction("ldr x10, [sp, #32]");                                  // load probe count
    emitter.instruction("ldr x5, [sp, #0]");                                    // reload hash_table_ptr
    emitter.instruction("ldr x6, [x5, #8]");                                    // reload capacity
    emitter.instruction("cmp x10, x6");                                         // check if we've probed all slots
    emitter.instruction("b.ge __rt_hash_get_not_found");                        // if probed all, key not found

    // -- compute entry address: base + 40 + index * 64 --
    emitter.instruction("ldr x9, [sp, #24]");                                   // load current probe index
    emitter.instruction("mov x11, #64");                                        // entry size = 64 bytes with per-entry tags and insertion-order links
    emitter.instruction("mul x12, x9, x11");                                    // x12 = index * 64
    emitter.instruction("add x12, x5, x12");                                    // x12 = table_ptr + index * 64
    emitter.instruction("add x12, x12, #40");                                   // x12 = entry address (skip header)

    // -- check occupied field --
    emitter.instruction("ldr x13, [x12]");                                      // x13 = occupied flag
    emitter.instruction("cbz x13, __rt_hash_get_not_found");                    // if empty (0), key not in table
    emitter.instruction("cmp x13, #2");                                         // check for tombstone
    emitter.instruction("b.eq __rt_hash_get_next");                             // if tombstone, skip and continue probing

    // -- slot is occupied: compare keys --
    emitter.instruction("ldr x1, [sp, #8]");                                    // x1 = our key_ptr
    emitter.instruction("ldr x2, [sp, #16]");                                   // x2 = our key_len
    emitter.instruction("ldr x3, [x12, #8]");                                   // x3 = entry's key_ptr
    emitter.instruction("ldr x4, [x12, #16]");                                  // x4 = entry's key_len
    emitter.instruction("bl __rt_str_eq");                                      // compare keys, x0=1 if equal
    emitter.instruction("cbnz x0, __rt_hash_get_found");                        // if keys match, we found it

    // -- advance to next slot --
    emitter.label("__rt_hash_get_next");
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload current probe index
    emitter.instruction("add x9, x9, #1");                                      // index += 1
    emitter.instruction("ldr x5, [sp, #0]");                                    // reload hash_table_ptr
    emitter.instruction("ldr x6, [x5, #8]");                                    // reload capacity
    emitter.instruction("udiv x7, x9, x6");                                     // x7 = index / capacity
    emitter.instruction("msub x9, x7, x6, x9");                                 // x9 = index % capacity (wrap around)
    emitter.instruction("str x9, [sp, #24]");                                   // save updated probe index
    emitter.instruction("ldr x10, [sp, #32]");                                  // load probe count
    emitter.instruction("add x10, x10, #1");                                    // probe count += 1
    emitter.instruction("str x10, [sp, #32]");                                  // save updated probe count
    emitter.instruction("b __rt_hash_get_probe");                               // try next slot

    // -- key found: return entry's value --
    emitter.label("__rt_hash_get_found");

    // -- recompute entry address (registers were clobbered by str_eq) --
    emitter.instruction("ldr x5, [sp, #0]");                                    // reload hash_table_ptr
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload probe index
    emitter.instruction("mov x11, #64");                                        // entry size = 64 bytes with per-entry tags and insertion-order links
    emitter.instruction("mul x12, x9, x11");                                    // x12 = index * 64
    emitter.instruction("add x12, x5, x12");                                    // x12 = table_ptr + index * 64
    emitter.instruction("add x12, x12, #40");                                   // x12 = entry address

    emitter.instruction("mov x0, #1");                                          // found = 1
    emitter.instruction("ldr x1, [x12, #24]");                                  // x1 = value_lo
    emitter.instruction("ldr x2, [x12, #32]");                                  // x2 = value_hi
    emitter.instruction("ldr x3, [x12, #40]");                                  // x3 = value_tag
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller

    // -- key not found --
    emitter.label("__rt_hash_get_not_found");
    emitter.instruction("mov x0, #0");                                          // found = 0
    emitter.instruction("mov x1, #0");                                          // value_lo = 0
    emitter.instruction("mov x2, #0");                                          // value_hi = 0
    emitter.instruction("mov x3, #8");                                          // value_tag = null when lookup misses
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
