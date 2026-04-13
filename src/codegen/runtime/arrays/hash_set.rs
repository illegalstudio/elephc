use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// hash_set: insert or update a key-value pair in the hash table.
/// Grows the table automatically if load factor exceeds 75%, persists newly
/// inserted keys, and releases overwritten heap-backed values on update.
/// Input:  x0=hash_table_ptr, x1=key_ptr, x2=key_len, x3=value_lo, x4=value_hi, x5=value_tag
/// Output: x0=hash_table_ptr (may differ if table was reallocated)
pub fn emit_hash_set(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hash_set_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: hash_set ---");
    emitter.label_global("__rt_hash_set");

    // -- set up stack frame, save all inputs --
    // Stack layout:
    //   [sp, #0]  = hash_table_ptr (x0)
    //   [sp, #8]  = key_ptr (x1)
    //   [sp, #16] = key_len (x2)
    //   [sp, #24] = value_lo (x3)
    //   [sp, #32] = value_hi (x4)
    //   [sp, #40] = value_tag (x5)
    //   [sp, #48] = probe index (saved across calls)
    //   [sp, #64] = saved x29
    //   [sp, #72] = saved x30
    emitter.instruction("sub sp, sp, #80");                                     // allocate 80 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // set up new frame pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save key_ptr
    emitter.instruction("str x2, [sp, #16]");                                   // save key_len
    emitter.instruction("str x3, [sp, #24]");                                   // save value_lo
    emitter.instruction("str x4, [sp, #32]");                                   // save value_hi
    emitter.instruction("str x5, [sp, #40]");                                   // save value_tag
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
    emitter.instruction("str x8, [sp, #48]");                                   // save initial probe index
    emitter.instruction("mov x10, #0");                                         // x10 = probe count (to detect full table)

    emitter.label("__rt_hash_set_probe");
    emitter.instruction("cmp x10, x6");                                         // check if we've probed all slots
    emitter.instruction("b.ge __rt_hash_set_done");                             // if probed all, table is full (shouldn't happen)

    // -- compute entry address: base + 40 + index * 64 --
    emitter.instruction("ldr x9, [sp, #48]");                                   // load current probe index
    emitter.instruction("mov x11, #64");                                        // entry size = 64 bytes with per-entry tags and insertion-order links
    emitter.instruction("mul x12, x9, x11");                                    // x12 = index * 64
    emitter.instruction("add x12, x5, x12");                                    // x12 = table_ptr + index * 64
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
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload probe index

    // -- recompute entry address after call clobbered registers --
    emitter.instruction("mov x11, #64");                                        // entry size = 64 bytes with per-entry tags and insertion-order links
    emitter.instruction("mul x12, x9, x11");                                    // x12 = index * 64
    emitter.instruction("add x12, x5, x12");                                    // x12 = table_ptr + index * 64
    emitter.instruction("add x12, x12, #40");                                   // x12 = entry address

    emitter.instruction("cbnz x0, __rt_hash_set_update");                       // if keys match, update existing entry

    // -- keys don't match, advance to next slot --
    emitter.instruction("add x9, x9, #1");                                      // index += 1
    emitter.instruction("udiv x7, x9, x6");                                     // x7 = index / capacity
    emitter.instruction("msub x9, x7, x6, x9");                                 // x9 = index % capacity (wrap around)
    emitter.instruction("str x9, [sp, #48]");                                   // save updated probe index

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
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload probe index after helper call
    emitter.instruction("mov x11, #64");                                        // entry size = 64 bytes with per-entry tags and insertion-order links
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
    emitter.instruction("ldr x13, [sp, #40]");                                  // load value_tag
    emitter.instruction("str x13, [x12, #40]");                                 // store value_tag in entry
    emitter.instruction("ldr x14, [x5, #32]");                                  // load the previous tail slot for insertion-order linking
    emitter.instruction("str x14, [x12, #48]");                                 // store prev = old tail on the new entry
    emitter.instruction("mov x15, #-1");                                        // sentinel index for end of the insertion-order chain
    emitter.instruction("str x15, [x12, #56]");                                 // store next = none on the new tail entry
    emitter.instruction("ldr x15, [x5, #24]");                                  // load the current head slot
    emitter.instruction("cmp x15, #-1");                                        // is this the first insertion into the table?
    emitter.instruction("b.ne __rt_hash_set_link_tail");                        // existing tables append after the previous tail
    emitter.instruction("str x9, [x5, #24]");                                   // initialize head = inserted slot
    emitter.instruction("str x9, [x5, #32]");                                   // initialize tail = inserted slot
    emitter.instruction("b __rt_hash_set_insert_header");                       // skip the tail-link update for the first entry
    emitter.label("__rt_hash_set_link_tail");
    emitter.instruction("mov x16, #64");                                        // x16 = hash entry size for tail-slot addressing
    emitter.instruction("mul x17, x14, x16");                                   // x17 = previous tail slot byte offset
    emitter.instruction("add x17, x5, x17");                                    // advance from table base to the previous tail slot
    emitter.instruction("add x17, x17, #40");                                   // skip the hash header to the previous tail entry
    emitter.instruction("str x9, [x17, #56]");                                  // link old tail.next = inserted slot
    emitter.instruction("str x9, [x5, #32]");                                   // update tail = inserted slot
    emitter.label("__rt_hash_set_insert_header");

    // -- increment count --
    emitter.instruction("ldr x5, [sp, #0]");                                    // reload hash_table_ptr
    emitter.instruction("ldr x13, [x5]");                                       // load current count
    emitter.instruction("add x13, x13, #1");                                    // count += 1
    emitter.instruction("str x13, [x5]");                                       // store updated count
    emitter.instruction("b __rt_hash_set_done");                                // done inserting

    // -- update existing entry's value --
    emitter.label("__rt_hash_set_update");
    emitter.instruction("ldr x13, [x12, #40]");                                 // load the overwritten entry's per-entry value_tag
    emitter.instruction("cmp x13, #8");                                         // is the overwritten value null?
    emitter.instruction("b.eq __rt_hash_set_write_value");                      // null has no heap pointer, skip release
    emitter.instruction("cmp x13, #1");                                         // is the overwritten value a string?
    emitter.instruction("b.eq __rt_hash_set_release_any");                      // strings release through the uniform dispatcher
    emitter.instruction("cmp x13, #4");                                         // is the overwritten value a heap-backed payload?
    emitter.instruction("b.hs __rt_hash_set_release_any");                      // tags 4-7 all release through the uniform dispatcher
    emitter.instruction("b __rt_hash_set_write_value");                         // scalars/bools/floats do not need release before overwrite

    emitter.label("__rt_hash_set_release_any");
    emitter.instruction("ldr x0, [x12, #24]");                                  // load the previous heap-backed value pointer from the entry
    emitter.instruction("bl __rt_decref_any");                                  // release the overwritten payload through the uniform dispatcher

    emitter.label("__rt_hash_set_recompute_entry");
    emitter.instruction("ldr x5, [sp, #0]");                                    // reload hash table pointer after helper call
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload probe index after helper call
    emitter.instruction("mov x11, #64");                                        // entry size = 64 bytes with per-entry tags and insertion-order links
    emitter.instruction("mul x12, x9, x11");                                    // recompute byte offset for this slot
    emitter.instruction("add x12, x5, x12");                                    // advance from table base to slot
    emitter.instruction("add x12, x12, #40");                                   // skip hash header to entry storage

    emitter.label("__rt_hash_set_write_value");
    emitter.instruction("ldr x13, [sp, #24]");                                  // load value_lo
    emitter.instruction("str x13, [x12, #24]");                                 // update value_lo in entry
    emitter.instruction("ldr x13, [sp, #32]");                                  // load value_hi
    emitter.instruction("str x13, [x12, #32]");                                 // update value_hi in entry
    emitter.instruction("ldr x13, [sp, #40]");                                  // load value_tag
    emitter.instruction("str x13, [x12, #40]");                                 // update value_tag in entry

    // -- tear down stack frame and return --
    emitter.label("__rt_hash_set_done");
    emitter.instruction("ldr x0, [sp, #0]");                                    // return table pointer (may be new after grow)
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_hash_set_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_set ---");
    emitter.label_global("__rt_hash_set");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving hash-insert spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved table/key/value tuple
    emitter.instruction("sub rsp, 64");                                         // reserve local storage for the hash insert/update state machine
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the hash-table pointer across helper calls and probe iterations
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the incoming key pointer across helper calls and probe iterations
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the incoming key length across helper calls and probe iterations
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the low payload word across the probe and optional update paths
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // save the high payload word across the probe and optional update paths
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // save the runtime value tag across the probe and optional update paths
    emitter.instruction("mov rdi, rsi");                                        // pass the key pointer to the x86_64 hash helper in the first SysV argument register
    emitter.instruction("mov rsi, rdx");                                        // pass the key length to the x86_64 hash helper in the second SysV argument register
    emitter.instruction("call __rt_hash_fnv1a");                                // compute the 64-bit FNV-1a hash for the inserted key
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the hash-table pointer after the hash helper returns
    emitter.instruction("mov r11, QWORD PTR [r10 + 8]");                        // load the table capacity for the modulo operation and linear-probe loop
    emitter.instruction("xor edx, edx");                                        // clear the high dividend half before dividing the 64-bit hash by the capacity
    emitter.instruction("div r11");                                             // compute hash % capacity using the SysV integer divide remainder register
    emitter.instruction("mov QWORD PTR [rbp - 56], rdx");                       // save the initial probe index so the loop can survive helper calls

    emitter.label("__rt_hash_set_probe");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the hash-table pointer at the top of every probe iteration
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the current probe index before deriving the slot address
    emitter.instruction("mov r12, r11");                                        // copy the current probe index before scaling it into a byte offset
    emitter.instruction("shl r12, 6");                                          // convert the probe index into a 64-byte entry offset
    emitter.instruction("add r12, r10");                                        // advance from the hash-table base pointer to the selected entry block
    emitter.instruction("add r12, 40");                                         // skip the fixed 40-byte hash header to land on the selected entry
    emitter.instruction("mov r13, QWORD PTR [r12]");                            // load the occupied marker for the probed hash-entry slot
    emitter.instruction("cmp r13, 1");                                          // check whether the current probe landed on an occupied entry
    emitter.instruction("jne __rt_hash_set_insert");                            // empty or tombstone entries can be claimed immediately for insertion
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // pass the incoming key pointer to the x86_64 string-equality helper
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // pass the incoming key length to the x86_64 string-equality helper
    emitter.instruction("mov rdx, QWORD PTR [r12 + 8]");                        // pass the stored entry key pointer to the equality helper
    emitter.instruction("mov rcx, QWORD PTR [r12 + 16]");                       // pass the stored entry key length to the equality helper
    emitter.instruction("call __rt_str_eq");                                    // compare the existing key with the inserted key before deciding between update and probe
    emitter.instruction("test rax, rax");                                       // check whether the probed slot already stores the same logical key
    emitter.instruction("jne __rt_hash_set_update");                            // overwrite the existing payload instead of probing further when the keys match
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the hash-table pointer after the equality helper clobbered caller-saved registers
    emitter.instruction("mov r11, QWORD PTR [r10 + 8]");                        // reload the table capacity before advancing the linear-probe cursor
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");                       // reload the current probe index before incrementing it
    emitter.instruction("add rdx, 1");                                          // advance to the next linear-probe slot after a key mismatch
    emitter.instruction("cmp rdx, r11");                                        // detect wraparound once the probe index reaches the table capacity
    emitter.instruction("jb __rt_hash_set_store_probe");                        // keep the incremented probe index when the cursor remains in bounds
    emitter.instruction("xor edx, edx");                                        // wrap the probe cursor back to slot zero once the end of the table is reached

    emitter.label("__rt_hash_set_store_probe");
    emitter.instruction("mov QWORD PTR [rbp - 56], rdx");                       // persist the updated probe index before the next loop iteration
    emitter.instruction("jmp __rt_hash_set_probe");                             // continue probing until an empty slot or existing matching key is found

    emitter.label("__rt_hash_set_insert");
    emitter.instruction("mov QWORD PTR [r12], 1");                              // mark the selected entry slot as occupied before filling its payload fields
    emitter.instruction("mov r13, QWORD PTR [rbp - 16]");                       // reload the inserted key pointer from the saved argument area
    emitter.instruction("mov QWORD PTR [r12 + 8], r13");                        // store the inserted key pointer directly into the selected hash entry
    emitter.instruction("mov r13, QWORD PTR [rbp - 24]");                       // reload the inserted key length from the saved argument area
    emitter.instruction("mov QWORD PTR [r12 + 16], r13");                       // store the inserted key length directly into the selected hash entry
    emitter.instruction("mov r13, QWORD PTR [rbp - 32]");                       // reload the low payload word that belongs to the inserted hash value
    emitter.instruction("mov QWORD PTR [r12 + 24], r13");                       // store the low payload word into the hash entry payload area
    emitter.instruction("mov r13, QWORD PTR [rbp - 40]");                       // reload the high payload word that belongs to the inserted hash value
    emitter.instruction("mov QWORD PTR [r12 + 32], r13");                       // store the high payload word into the hash entry payload area
    emitter.instruction("mov r13, QWORD PTR [rbp - 48]");                       // reload the runtime value tag that describes the inserted payload
    emitter.instruction("mov QWORD PTR [r12 + 40], r13");                       // store the runtime value tag into the selected hash entry
    emitter.instruction("mov r13, QWORD PTR [r10 + 32]");                       // load the previous insertion-order tail slot from the hash header
    emitter.instruction("mov QWORD PTR [r12 + 48], r13");                       // link the new entry back to the previous tail slot
    emitter.instruction("mov r14, -1");                                         // materialize the end-of-chain sentinel for the inserted tail entry
    emitter.instruction("mov QWORD PTR [r12 + 56], r14");                       // initialize the new entry next-pointer as the tail sentinel
    emitter.instruction("cmp r13, -1");                                         // detect the first insertion so the hash header head/tail can be initialized together
    emitter.instruction("jne __rt_hash_set_link_tail");                         // existing tables need an extra step to wire the previous tail forward to the inserted slot
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the inserted slot index to seed the insertion-order head pointer
    emitter.instruction("mov QWORD PTR [r10 + 24], r11");                       // header[24]: initialize the insertion-order head for the first occupied entry
    emitter.instruction("mov QWORD PTR [r10 + 32], r11");                       // header[32]: initialize the insertion-order tail for the first occupied entry
    emitter.instruction("jmp __rt_hash_set_bump_count");                        // skip the previous-tail forward-link update on the very first insertion

    emitter.label("__rt_hash_set_link_tail");
    emitter.instruction("mov r14, r13");                                        // copy the previous tail slot index before scaling it into a byte offset
    emitter.instruction("shl r14, 6");                                          // convert the previous tail slot index into a 64-byte entry offset
    emitter.instruction("add r14, r10");                                        // advance from the hash-table base pointer to the previous tail entry block
    emitter.instruction("add r14, 40");                                         // skip the fixed hash header to land on the previous tail entry
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the inserted slot index for the previous-tail forward-link store
    emitter.instruction("mov QWORD PTR [r14 + 56], r11");                       // update the previous tail entry to point at the inserted slot as its logical successor
    emitter.instruction("mov QWORD PTR [r10 + 32], r11");                       // publish the inserted slot as the new insertion-order tail in the hash header

    emitter.label("__rt_hash_set_bump_count");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the current live-entry count from the hash header before incrementing it
    emitter.instruction("add r11, 1");                                          // increment the live-entry count after claiming a previously empty hash slot
    emitter.instruction("mov QWORD PTR [r10], r11");                            // store the updated live-entry count back into the hash header
    emitter.instruction("mov rax, r10");                                        // return the original hash-table pointer after a successful insertion
    emitter.instruction("add rsp, 64");                                         // release the local spill area that held the saved table/key/value tuple
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to the insertion caller
    emitter.instruction("ret");                                                 // return to the caller with the hash-table pointer in rax

    emitter.label("__rt_hash_set_update");
    emitter.instruction("mov r13, QWORD PTR [rbp - 32]");                       // reload the replacement low payload word for the existing key slot
    emitter.instruction("mov QWORD PTR [r12 + 24], r13");                       // overwrite the stored low payload word in the existing hash entry
    emitter.instruction("mov r13, QWORD PTR [rbp - 40]");                       // reload the replacement high payload word for the existing key slot
    emitter.instruction("mov QWORD PTR [r12 + 32], r13");                       // overwrite the stored high payload word in the existing hash entry
    emitter.instruction("mov r13, QWORD PTR [rbp - 48]");                       // reload the replacement runtime value tag for the existing key slot
    emitter.instruction("mov QWORD PTR [r12 + 40], r13");                       // overwrite the stored runtime value tag in the existing hash entry
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the unchanged hash-table pointer after an in-place value update
    emitter.instruction("add rsp, 64");                                         // release the local spill area before leaving the update path
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to the insertion caller
    emitter.instruction("ret");                                                 // return to the caller with the existing hash-table pointer in rax
}
