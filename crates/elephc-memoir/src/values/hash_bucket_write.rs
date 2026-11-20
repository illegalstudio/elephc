//! Purpose:
//! Grows and probes native hash buckets for borrowed-key assignment.
//!
//! Called from:
//! - The associative-array assignment emitter after selecting its COW mode.
//!
//! Key details:
//! - An existing match branches to the caller's reference or overwrite policy.
//! - A new bucket acquires a persistent string key and links in insertion order.

use crate::{hash_layout, Emitter};

/// Probes and fills a borrowed-key hash write frame, persisting a key only on insertion.
pub fn emit_aarch64(emitter: &mut impl Emitter) {
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
    emitter.instruction("bl __rt_hash_key_hash");                               // compute the typed key hash, result in x0

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

    // -- compute entry address: entries + index * 64 --
    emitter.instruction("ldr x9, [sp, #48]");                                   // load current probe index
    emitter.instruction("mov x11, #64");                                        // entry size = 64 bytes with per-entry tags and insertion-order links
    hash_layout::emit_entry_address(emitter, "x12", "x5", "x9");

    // -- check occupied field --
    emitter.instruction("ldr x13, [x12]");                                      // x13 = occupied flag of this entry
    emitter.instruction("cmp x13, #1");                                         // check if slot is occupied
    emitter.instruction("b.ne __rt_hash_set_insert");                           // if empty (0) or tombstone (2), insert here

    // -- slot is occupied: compare keys to check for update --
    emitter.instruction("ldr x1, [sp, #8]");                                    // x1 = our key_ptr
    emitter.instruction("ldr x2, [sp, #16]");                                   // x2 = our key_len
    emitter.instruction("ldr x3, [x12, #8]");                                   // x3 = existing key_ptr in entry
    emitter.instruction("ldr x4, [x12, #16]");                                  // x4 = existing key_len in entry
    emitter.instruction("bl __rt_hash_key_eq");                                 // compare normalized typed keys, x0=1 if equal

    // -- restore probe state after str_eq --
    emitter.instruction("ldr x5, [sp, #0]");                                    // reload hash_table_ptr
    emitter.instruction("ldr x6, [x5, #8]");                                    // reload capacity
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload probe index

    // -- recompute entry address after call clobbered registers --
    emitter.instruction("mov x11, #64");                                        // entry size = 64 bytes with per-entry tags and insertion-order links
    hash_layout::emit_entry_address(emitter, "x12", "x5", "x9");

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
    emitter.instruction("cmn x2, #1");                                          // check whether the inserted key is an integer key
    emitter.instruction("b.eq __rt_hash_set_key_ready");                        // integer keys are stored inline and do not need persistence
    emitter.instruction("bl __rt_str_persist");                                 // persist inserted string key into heap storage
    emitter.instruction("str x1, [sp, #8]");                                    // save persistent key pointer for slot write
    emitter.instruction("str x2, [sp, #16]");                                   // save persistent key length for slot write
    emitter.label("__rt_hash_set_key_ready");
    emitter.instruction("ldr x5, [sp, #0]");                                    // reload hash table pointer after helper call
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload probe index after helper call
    emitter.instruction("mov x11, #64");                                        // entry size = 64 bytes with per-entry tags and insertion-order links
    hash_layout::emit_entry_address(emitter, "x12", "x5", "x9");
    emitter.instruction("mov x13, #1");                                         // occupied = 1
    emitter.instruction("str x13, [x12]");                                      // set entry as occupied
    emitter.instruction("ldr x13, [sp, #8]");                                   // load persistent key_ptr
    emitter.instruction("str x13, [x12, #8]");                                  // store key_ptr in entry
    emitter.instruction("ldr x13, [sp, #16]");                                  // load key_len
    emitter.instruction("str x13, [x12, #16]");                                 // store key_len in entry
    super::hash_bucket::emit_publish_payload(emitter);
    super::hash_bucket::emit_link_new_bucket(emitter, "__rt_hash_set", "__rt_hash_set_insert_header");
}

/// Probes and fills a borrowed-key hash write frame, persisting a key only on insertion.
pub fn emit_x86_64(emitter: &mut impl Emitter) {
    // -- check load factor: grow if count * 4 >= capacity * 3 (75%) --
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // load the current live-entry count
    emitter.instruction("mov r11, QWORD PTR [rax + 8]");                        // load the current table capacity
    emitter.instruction("mov r12, r10");                                        // copy count before scaling it
    emitter.instruction("shl r12, 2");                                          // scaled_count = count * 4
    emitter.instruction("lea r13, [r11 + r11 * 2]");                            // scaled_capacity = capacity * 3
    emitter.instruction("cmp r12, r13");                                        // has the table reached the growth threshold?
    emitter.instruction("jl __rt_hash_set_no_grow_x");                          // skip growth while under the load threshold
    emitter.instruction("mov rdi, rax");                                        // pass the unique table to the grow helper
    emitter.instruction("call __rt_hash_grow");                                 // double the hash table capacity before probing
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the grown table pointer
    emitter.label("__rt_hash_set_no_grow_x");

    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the incoming key low word after the copy-on-write helper clobbered caller-saved argument registers
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // reload the incoming key high word after the copy-on-write helper clobbered caller-saved argument registers
    emitter.instruction("call __rt_hash_key_hash");                             // compute the 64-bit hash for the normalized inserted key
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the hash-table pointer after the hash helper returns
    emitter.instruction("mov r11, QWORD PTR [r10 + 8]");                        // load the table capacity for the modulo operation and linear-probe loop
    emitter.instruction("xor edx, edx");                                        // clear the high dividend half before dividing the 64-bit hash by the capacity
    emitter.instruction("div r11");                                             // compute hash % capacity using the SysV integer divide remainder register
    emitter.instruction("mov QWORD PTR [rbp - 56], rdx");                       // save the initial probe index so the loop can survive helper calls

    emitter.label("__rt_hash_set_probe");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the hash-table pointer at the top of every probe iteration
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the current probe index before deriving the slot address
    hash_layout::emit_entry_address(emitter, "r12", "r10", "r11");
    emitter.instruction("mov r13, QWORD PTR [r12]");                            // load the occupied marker for the probed hash-entry slot
    emitter.instruction("cmp r13, 1");                                          // check whether the current probe landed on an occupied entry
    emitter.instruction("jne __rt_hash_set_insert");                            // empty or tombstone entries can be claimed immediately for insertion
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // pass the incoming key pointer to the x86_64 string-equality helper
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // pass the incoming key length to the x86_64 string-equality helper
    emitter.instruction("mov rdx, QWORD PTR [r12 + 8]");                        // pass the stored entry key pointer to the equality helper
    emitter.instruction("mov rcx, QWORD PTR [r12 + 16]");                       // pass the stored entry key length to the equality helper
    emitter.instruction("call __rt_hash_key_eq");                               // compare the existing normalized key with the inserted key before deciding between update and probe
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
    emitter.instruction("mov QWORD PTR [rbp - 64], r12");                       // preserve the selected entry pointer across the string-persist helper call for computed keys
    emitter.instruction("cmp QWORD PTR [rbp - 24], -1");                        // check whether the inserted key is an integer key
    emitter.instruction("je __rt_hash_set_key_ready");                          // integer keys are stored inline and do not need persistence
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // move the inserted key pointer into the x86_64 string helper input register
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // move the inserted key length into the paired x86_64 string helper register
    emitter.instruction("call __rt_str_persist");                               // duplicate the inserted key so hash-table storage does not alias transient concat-buffer bytes
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the owned key pointer for the selected hash entry
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the owned key length for the selected hash entry
    emitter.label("__rt_hash_set_key_ready");
    emitter.instruction("mov r12, QWORD PTR [rbp - 64]");                       // restore the selected entry pointer after persisting the inserted key
    emitter.instruction("mov r13, QWORD PTR [rbp - 16]");                       // reload the normalized key low word for storage in the selected hash entry
    emitter.instruction("mov QWORD PTR [r12 + 8], r13");                        // store the owned string pointer or integer payload into the selected hash entry
    emitter.instruction("mov r13, QWORD PTR [rbp - 24]");                       // reload the normalized key high word for storage in the selected hash entry
    emitter.instruction("mov QWORD PTR [r12 + 16], r13");                       // store the string length or integer sentinel into the selected hash entry
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the hash-table pointer after the string-persist helper clobbered caller-saved registers
    super::hash_bucket::emit_publish_payload(emitter);
    super::hash_bucket::emit_link_new_bucket(emitter, "__rt_hash_set", "__rt_hash_set_bump_count");
}
