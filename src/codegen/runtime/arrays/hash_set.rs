use crate::codegen::emit::Emitter;

/// hash_set: insert or update a key-value pair in the hash table.
/// Grows the table automatically if load factor exceeds 75%, persists newly
/// inserted keys, and releases overwritten heap-backed values on update.
/// Input:  x0=hash_table_ptr, x1=key_ptr, x2=key_len, x3=value_lo, x4=value_hi
/// Output: x0=hash_table_ptr (may differ if table was reallocated)
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
    emitter.instruction("str x1, [sp, #8]");                                    // save key_ptr
    emitter.instruction("str x2, [sp, #16]");                                   // save key_len
    emitter.instruction("str x3, [sp, #24]");                                   // save value_lo
    emitter.instruction("str x4, [sp, #32]");                                   // save value_hi
    emitter.instruction("bl __rt_hash_ensure_unique");                          // split shared hash tables before insert/update mutates storage
    emitter.instruction("str x0, [sp, #0]");                                    // save the unique hash_table_ptr

    // -- check load factor: grow if count * 4 >= capacity * 3 (75%) --
    emitter.instruction("ldr x5, [x0]");                                        // x5 = count
    emitter.instruction("ldr x6, [x0, #8]");                                    // x6 = capacity
    emitter.instruction("lsl x7, x5, #2");                                      // x7 = count * 4
    emitter.instruction("mov x8, #3");                                          // multiplier
    emitter.instruction("mul x8, x6, x8");                                      // x8 = capacity * 3
    emitter.instruction("cmp x7, x8");                                          // count*4 >= capacity*3?
    emitter.instruction("b.lt __rt_hash_set_no_grow");                          // skip growth if under threshold

    // -- grow the hash table --
    emitter.instruction("bl __rt_hash_grow");                                   // x0 = new table (doubled capacity)
    emitter.instruction("str x0, [sp, #0]");                                    // update saved table pointer

    emitter.label("__rt_hash_set_no_grow");

    // -- hash the key --
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload incoming key_ptr
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

    // -- compute entry address: base + 40 + index * 56 --
    emitter.instruction("ldr x9, [sp, #40]");                                   // load current probe index
    emitter.instruction("mov x11, #56");                                        // entry size = 56 bytes with insertion-order links
    emitter.instruction("mul x12, x9, x11");                                    // x12 = index * 56
    emitter.instruction("add x12, x5, x12");                                    // x12 = table_ptr + index * 56
    emitter.instruction("add x12, x12, #40");                                   // x12 = entry address (skip 40-byte header)

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
    emitter.instruction("mov x11, #56");                                        // entry size = 56 bytes with insertion-order links
    emitter.instruction("mul x12, x9, x11");                                    // x12 = index * 56
    emitter.instruction("add x12, x5, x12");                                    // x12 = table_ptr + index * 56
    emitter.instruction("add x12, x12, #40");                                   // x12 = entry address

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
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload inserted key_ptr
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload inserted key_len
    emitter.instruction("bl __rt_str_persist");                                 // persist inserted key into heap storage
    emitter.instruction("str x1, [sp, #8]");                                    // save persistent key pointer for slot write
    emitter.instruction("ldr x5, [sp, #0]");                                    // reload hash table pointer after helper call
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload probe index after helper call
    emitter.instruction("mov x11, #56");                                        // entry size = 56 bytes with insertion-order links
    emitter.instruction("mul x12, x9, x11");                                    // recompute byte offset for this slot
    emitter.instruction("add x12, x5, x12");                                    // advance from table base to slot
    emitter.instruction("add x12, x12, #40");                                   // skip hash header to entry storage
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
    emitter.instruction("ldr x14, [x5, #32]");                                  // load the previous tail slot for insertion-order linking
    emitter.instruction("str x14, [x12, #40]");                                 // store prev = old tail on the new entry
    emitter.instruction("mov x15, #-1");                                        // sentinel index for end of the insertion-order chain
    emitter.instruction("str x15, [x12, #48]");                                 // store next = none on the new tail entry
    emitter.instruction("ldr x15, [x5, #24]");                                  // load the current head slot
    emitter.instruction("cmp x15, #-1");                                         // is this the first insertion into the table?
    emitter.instruction("b.ne __rt_hash_set_link_tail");                         // existing tables append after the previous tail
    emitter.instruction("str x9, [x5, #24]");                                   // initialize head = inserted slot
    emitter.instruction("str x9, [x5, #32]");                                   // initialize tail = inserted slot
    emitter.instruction("b __rt_hash_set_insert_header");                        // skip the tail-link update for the first entry
    emitter.label("__rt_hash_set_link_tail");
    emitter.instruction("mov x16, #56");                                        // x16 = hash entry size for tail-slot addressing
    emitter.instruction("mul x17, x14, x16");                                   // x17 = previous tail slot byte offset
    emitter.instruction("add x17, x5, x17");                                    // advance from table base to the previous tail slot
    emitter.instruction("add x17, x17, #40");                                   // skip the hash header to the previous tail entry
    emitter.instruction("str x9, [x17, #48]");                                  // link old tail.next = inserted slot
    emitter.instruction("str x9, [x5, #32]");                                   // update tail = inserted slot
    emitter.label("__rt_hash_set_insert_header");
    emitter.instruction("ldr x13, [sp, #24]");                                  // reload value_lo for dynamic value_type maintenance
    emitter.instruction("cbz x13, __rt_hash_set_insert_header_done");           // null/scalar zero leaves the header value_type unchanged
    emitter.instruction("mov x0, x13");                                         // inspect the inserted payload through the uniform heap-kind helper
    emitter.instruction("bl __rt_heap_kind");                                   // x0 = heap kind for the inserted payload (0/1/2/3/4)
    emitter.instruction("ldr x5, [sp, #0]");                                    // reload hash table pointer after helper call
    emitter.instruction("cmp x0, #1");                                          // is the inserted payload a persisted string?
    emitter.instruction("b.eq __rt_hash_set_insert_header_str");                // keep the hash header aligned to string payloads
    emitter.instruction("cmp x0, #2");                                          // is the inserted payload an indexed array?
    emitter.instruction("b.eq __rt_hash_set_insert_header_array");              // keep the hash header aligned to indexed arrays
    emitter.instruction("cmp x0, #3");                                          // is the inserted payload an associative array?
    emitter.instruction("b.eq __rt_hash_set_insert_header_hash");               // keep the hash header aligned to associative arrays
    emitter.instruction("cmp x0, #4");                                          // is the inserted payload an object?
    emitter.instruction("b.ne __rt_hash_set_insert_header_done");               // scalars/non-heap payloads keep the current header value_type
    emitter.instruction("mov x13, #6");                                         // runtime value_type 6 = object
    emitter.instruction("b __rt_hash_set_insert_header_store");                 // store the upgraded hash header value_type
    emitter.label("__rt_hash_set_insert_header_str");
    emitter.instruction("mov x13, #1");                                         // runtime value_type 1 = string
    emitter.instruction("b __rt_hash_set_insert_header_store");                 // store the upgraded hash header value_type
    emitter.label("__rt_hash_set_insert_header_array");
    emitter.instruction("mov x13, #4");                                         // runtime value_type 4 = indexed array
    emitter.instruction("b __rt_hash_set_insert_header_store");                 // store the upgraded hash header value_type
    emitter.label("__rt_hash_set_insert_header_hash");
    emitter.instruction("mov x13, #5");                                         // runtime value_type 5 = associative array
    emitter.label("__rt_hash_set_insert_header_store");
    emitter.instruction("str x13, [x5, #16]");                                  // upgrade the hash header value_type to match the inserted heap payload
    emitter.label("__rt_hash_set_insert_header_done");

    // -- increment count --
    emitter.instruction("ldr x5, [sp, #0]");                                    // reload hash_table_ptr
    emitter.instruction("ldr x13, [x5]");                                       // load current count
    emitter.instruction("add x13, x13, #1");                                    // count += 1
    emitter.instruction("str x13, [x5]");                                       // store updated count
    emitter.instruction("b __rt_hash_set_done");                                // done inserting

    // -- update existing entry's value --
    emitter.label("__rt_hash_set_update");
    emitter.instruction("ldr x13, [x5, #16]");                                  // load runtime value_type tag from hash header
    emitter.instruction("cmp x13, #1");                                         // is the overwritten value a string?
    emitter.instruction("b.eq __rt_hash_set_release_str");                      // free old string payload before overwrite
    emitter.instruction("cmp x13, #4");                                         // is the overwritten value an indexed array?
    emitter.instruction("b.eq __rt_hash_set_release_array");                    // decref old indexed array payload
    emitter.instruction("cmp x13, #5");                                         // is the overwritten value an associative array?
    emitter.instruction("b.eq __rt_hash_set_release_hash");                     // decref old hash payload
    emitter.instruction("cmp x13, #6");                                         // is the overwritten value an object?
    emitter.instruction("b.eq __rt_hash_set_release_object");                   // decref old object payload
    emitter.instruction("b __rt_hash_set_write_value");                         // scalars do not need release before overwrite

    emitter.label("__rt_hash_set_release_str");
    emitter.instruction("ldr x0, [x12, #24]");                                  // load previous string pointer from entry
    emitter.instruction("bl __rt_heap_free_safe");                              // release old string storage before overwrite
    emitter.instruction("b __rt_hash_set_recompute_entry");                     // recompute slot after helper call clobbers regs

    emitter.label("__rt_hash_set_release_array");
    emitter.instruction("ldr x0, [x12, #24]");                                  // load previous indexed array pointer from entry
    emitter.instruction("bl __rt_decref_array");                                // release old indexed array ownership before overwrite
    emitter.instruction("b __rt_hash_set_recompute_entry");                     // recompute slot after helper call clobbers regs

    emitter.label("__rt_hash_set_release_hash");
    emitter.instruction("ldr x0, [x12, #24]");                                  // load previous hash pointer from entry
    emitter.instruction("bl __rt_decref_hash");                                 // release old associative array ownership before overwrite
    emitter.instruction("b __rt_hash_set_recompute_entry");                     // recompute slot after helper call clobbers regs

    emitter.label("__rt_hash_set_release_object");
    emitter.instruction("ldr x0, [x12, #24]");                                  // load previous object pointer from entry
    emitter.instruction("bl __rt_decref_object");                               // release old object ownership before overwrite

    emitter.label("__rt_hash_set_recompute_entry");
    emitter.instruction("ldr x5, [sp, #0]");                                    // reload hash table pointer after helper call
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload probe index after helper call
    emitter.instruction("mov x11, #56");                                        // entry size = 56 bytes with insertion-order links
    emitter.instruction("mul x12, x9, x11");                                    // recompute byte offset for this slot
    emitter.instruction("add x12, x5, x12");                                    // advance from table base to slot
    emitter.instruction("add x12, x12, #40");                                   // skip hash header to entry storage

    emitter.label("__rt_hash_set_write_value");
    emitter.instruction("ldr x13, [sp, #24]");                                  // load value_lo
    emitter.instruction("str x13, [x12, #24]");                                 // update value_lo in entry
    emitter.instruction("ldr x13, [sp, #32]");                                  // load value_hi
    emitter.instruction("str x13, [x12, #32]");                                 // update value_hi in entry
    emitter.instruction("ldr x13, [sp, #24]");                                  // reload value_lo for dynamic value_type maintenance
    emitter.instruction("cbz x13, __rt_hash_set_done");                         // null/scalar zero leaves the header value_type unchanged
    emitter.instruction("mov x0, x13");                                         // inspect the updated payload through the uniform heap-kind helper
    emitter.instruction("bl __rt_heap_kind");                                   // x0 = heap kind for the updated payload (0/1/2/3/4)
    emitter.instruction("ldr x5, [sp, #0]");                                    // reload hash table pointer after helper call
    emitter.instruction("cmp x0, #1");                                          // is the updated payload a persisted string?
    emitter.instruction("b.eq __rt_hash_set_update_header_str");                // keep the hash header aligned to string payloads
    emitter.instruction("cmp x0, #2");                                          // is the updated payload an indexed array?
    emitter.instruction("b.eq __rt_hash_set_update_header_array");              // keep the hash header aligned to indexed arrays
    emitter.instruction("cmp x0, #3");                                          // is the updated payload an associative array?
    emitter.instruction("b.eq __rt_hash_set_update_header_hash");               // keep the hash header aligned to associative arrays
    emitter.instruction("cmp x0, #4");                                          // is the updated payload an object?
    emitter.instruction("b.ne __rt_hash_set_done");                             // scalars/non-heap payloads keep the current header value_type
    emitter.instruction("mov x13, #6");                                         // runtime value_type 6 = object
    emitter.instruction("b __rt_hash_set_update_header_store");                 // store the upgraded hash header value_type
    emitter.label("__rt_hash_set_update_header_str");
    emitter.instruction("mov x13, #1");                                         // runtime value_type 1 = string
    emitter.instruction("b __rt_hash_set_update_header_store");                 // store the upgraded hash header value_type
    emitter.label("__rt_hash_set_update_header_array");
    emitter.instruction("mov x13, #4");                                         // runtime value_type 4 = indexed array
    emitter.instruction("b __rt_hash_set_update_header_store");                 // store the upgraded hash header value_type
    emitter.label("__rt_hash_set_update_header_hash");
    emitter.instruction("mov x13, #5");                                         // runtime value_type 5 = associative array
    emitter.label("__rt_hash_set_update_header_store");
    emitter.instruction("str x13, [x5, #16]");                                  // upgrade the hash header value_type to match the updated heap payload

    // -- tear down stack frame and return --
    emitter.label("__rt_hash_set_done");
    emitter.instruction("ldr x0, [sp, #0]");                                    // return table pointer (may be new after grow)
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
