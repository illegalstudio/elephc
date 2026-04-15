use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// hash_insert_owned: insert a key-value pair whose key/value ownership already
/// belongs to the destination table. Used by hash_grow when moving entries.
/// Input:  x0=hash_table_ptr, x1=key_ptr, x2=key_len, x3=value_lo, x4=value_hi, x5=value_tag
/// Output: x0=hash_table_ptr
pub fn emit_hash_insert_owned(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hash_insert_owned_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: hash_insert_owned ---");
    emitter.label_global("__rt_hash_insert_owned");

    // -- set up stack frame, save all inputs --
    // Stack layout:
    //   [sp, #0]  = hash_table_ptr
    //   [sp, #8]  = key_ptr
    //   [sp, #16] = key_len
    //   [sp, #24] = value_lo
    //   [sp, #32] = value_hi
    //   [sp, #40] = value_tag
    //   [sp, #48] = probe index
    //   [sp, #64] = saved x29
    //   [sp, #72] = saved x30
    emitter.instruction("sub sp, sp, #80");                                     // allocate 80 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save hash_table_ptr
    emitter.instruction("str x1, [sp, #8]");                                    // save key_ptr
    emitter.instruction("str x2, [sp, #16]");                                   // save key_len
    emitter.instruction("str x3, [sp, #24]");                                   // save value_lo
    emitter.instruction("str x4, [sp, #32]");                                   // save value_hi
    emitter.instruction("str x5, [sp, #40]");                                   // save value_tag

    // -- hash the existing owned key --
    emitter.instruction("bl __rt_hash_fnv1a");                                  // compute hash of the moved key

    // -- compute slot index: hash % capacity --
    emitter.instruction("ldr x5, [sp, #0]");                                    // reload destination table pointer
    emitter.instruction("ldr x6, [x5, #8]");                                    // load table capacity
    emitter.instruction("udiv x7, x0, x6");                                     // divide hash by capacity
    emitter.instruction("msub x8, x7, x6, x0");                                 // compute hash % capacity
    emitter.instruction("str x8, [sp, #48]");                                   // save initial probe index
    emitter.instruction("mov x10, #0");                                         // probe count = 0

    // -- linear probe until we find an empty slot --
    emitter.label("__rt_hash_insert_owned_probe");
    emitter.instruction("cmp x10, x6");                                         // have we probed every slot?
    emitter.instruction("b.ge __rt_hash_insert_owned_done");                    // stop if the table is unexpectedly full

    emitter.instruction("ldr x9, [sp, #48]");                                   // reload current probe index
    emitter.instruction("mov x11, #64");                                        // entry size = 64 bytes with per-entry tags and insertion-order links
    emitter.instruction("mul x12, x9, x11");                                    // compute byte offset for this slot
    emitter.instruction("add x12, x5, x12");                                    // advance from table base to slot
    emitter.instruction("add x12, x12, #40");                                   // skip hash header to entry storage
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
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload probe index after call clobbers regs
    emitter.instruction("mov x11, #64");                                        // entry size = 64 bytes with per-entry tags and insertion-order links
    emitter.instruction("mul x12, x9, x11");                                    // recompute byte offset for this slot
    emitter.instruction("add x12, x5, x12");                                    // advance from table base to slot
    emitter.instruction("add x12, x12, #40");                                   // skip hash header to entry storage
    emitter.instruction("cbnz x0, __rt_hash_insert_owned_overwrite");           // overwrite if this key already exists

    // -- advance to the next probe slot --
    emitter.instruction("add x9, x9, #1");                                      // increment probe index
    emitter.instruction("udiv x7, x9, x6");                                     // divide updated index by capacity
    emitter.instruction("msub x9, x7, x6, x9");                                 // wrap index with modulo capacity
    emitter.instruction("str x9, [sp, #48]");                                   // save wrapped probe index
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
    emitter.instruction("ldr x13, [sp, #40]");                                  // reload moved value_tag
    emitter.instruction("str x13, [x12, #40]");                                 // store value_tag in slot
    emitter.instruction("ldr x14, [x5, #32]");                                  // load the previous tail slot for insertion-order linking
    emitter.instruction("str x14, [x12, #48]");                                 // store prev = old tail on the new entry
    emitter.instruction("mov x15, #-1");                                        // sentinel index for end of the insertion-order chain
    emitter.instruction("str x15, [x12, #56]");                                 // store next = none on the new tail entry
    emitter.instruction("ldr x15, [x5, #24]");                                  // load the current head slot
    emitter.instruction("cmp x15, #-1");                                        // is this the first inserted slot in the destination hash?
    emitter.instruction("b.ne __rt_hash_insert_owned_link_tail");               // existing hashes append after the previous tail
    emitter.instruction("str x9, [x5, #24]");                                   // initialize head = inserted slot
    emitter.instruction("str x9, [x5, #32]");                                   // initialize tail = inserted slot
    emitter.instruction("b __rt_hash_insert_owned_count");                      // skip the tail-link update for the first entry
    emitter.label("__rt_hash_insert_owned_link_tail");
    emitter.instruction("mov x16, #64");                                        // x16 = hash entry size for tail-slot addressing
    emitter.instruction("mul x17, x14, x16");                                   // x17 = previous tail slot byte offset
    emitter.instruction("add x17, x5, x17");                                    // advance from table base to the previous tail slot
    emitter.instruction("add x17, x17, #40");                                   // skip the hash header to the previous tail entry
    emitter.instruction("str x9, [x17, #56]");                                  // link old tail.next = inserted slot
    emitter.instruction("str x9, [x5, #32]");                                   // update tail = inserted slot
    emitter.label("__rt_hash_insert_owned_count");
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
    emitter.instruction("ldr x13, [sp, #40]");                                  // reload moved value_tag
    emitter.instruction("str x13, [x12, #40]");                                 // overwrite value_tag in existing slot

    emitter.label("__rt_hash_insert_owned_done");
    emitter.instruction("ldr x0, [sp, #0]");                                    // return destination table pointer
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_hash_insert_owned_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_insert_owned ---");
    emitter.label_global("__rt_hash_insert_owned");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving owned-insert spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved table/key/value tuple
    emitter.instruction("sub rsp, 96");                                         // reserve local storage plus callee-saved register spills while keeping nested calls 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 72], r12");                       // preserve r12 because the owned-insert probe logic reuses it as a long-lived scratch register
    emitter.instruction("mov QWORD PTR [rbp - 80], r13");                       // preserve r13 because the owned-insert probe logic reuses it as a long-lived scratch register
    emitter.instruction("mov QWORD PTR [rbp - 88], r14");                       // preserve r14 because insertion-order linking reuses it as a callee-saved scratch register
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the destination hash-table pointer across helper calls and probe iterations
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the owned key pointer across helper calls and probe iterations
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the owned key length across helper calls and probe iterations
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save the low payload word across the probe and optional overwrite paths
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // save the high payload word across the probe and optional overwrite paths
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // save the runtime value tag across the probe and optional overwrite paths
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the owned key pointer into the x86_64 hash helper input register
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // reload the owned key length into the paired x86_64 hash helper input register
    emitter.instruction("call __rt_hash_fnv1a");                                // compute the 64-bit FNV-1a hash for the owned key payload
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the destination hash-table pointer after the hash helper returns
    emitter.instruction("mov r11, QWORD PTR [r10 + 8]");                        // load the destination table capacity for the modulo operation and linear-probe loop
    emitter.instruction("xor edx, edx");                                        // clear the high dividend half before dividing the 64-bit hash by the capacity
    emitter.instruction("div r11");                                             // compute hash % capacity using the SysV integer divide remainder register
    emitter.instruction("mov QWORD PTR [rbp - 56], rdx");                       // save the initial probe index so the loop can survive helper calls

    emitter.label("__rt_hash_insert_owned_probe");
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the destination hash-table pointer at the top of every probe iteration
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the current probe index before deriving the slot address
    emitter.instruction("mov r12, r11");                                        // copy the current probe index before scaling it into a byte offset
    emitter.instruction("shl r12, 6");                                          // convert the probe index into a 64-byte entry offset
    emitter.instruction("add r12, r10");                                        // advance from the hash-table base pointer to the selected entry block
    emitter.instruction("add r12, 40");                                         // skip the fixed 40-byte hash header to land on the selected entry
    emitter.instruction("mov r13, QWORD PTR [r12]");                            // load the occupied marker for the probed hash-entry slot
    emitter.instruction("cmp r13, 1");                                          // check whether the current probe landed on an occupied entry
    emitter.instruction("jne __rt_hash_insert_owned_write");                    // empty or tombstone entries can be claimed immediately for insertion
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // pass the incoming owned key pointer to the x86_64 string-equality helper
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // pass the incoming owned key length to the x86_64 string-equality helper
    emitter.instruction("mov rdx, QWORD PTR [r12 + 8]");                        // pass the stored entry key pointer to the equality helper
    emitter.instruction("mov rcx, QWORD PTR [r12 + 16]");                       // pass the stored entry key length to the equality helper
    emitter.instruction("call __rt_str_eq");                                    // compare the existing key with the incoming owned key before deciding between overwrite and probe
    emitter.instruction("test rax, rax");                                       // check whether the probed slot already stores the same logical key
    emitter.instruction("jne __rt_hash_insert_owned_overwrite");                // overwrite the existing payload instead of probing further when the keys match
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the destination hash-table pointer after the equality helper clobbered caller-saved registers
    emitter.instruction("mov r11, QWORD PTR [r10 + 8]");                        // reload the destination table capacity before advancing the linear-probe cursor
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");                       // reload the current probe index before incrementing it
    emitter.instruction("add rdx, 1");                                          // advance to the next linear-probe slot after a key mismatch
    emitter.instruction("cmp rdx, r11");                                        // detect wraparound once the probe index reaches the table capacity
    emitter.instruction("jb __rt_hash_insert_owned_store_probe");               // keep the incremented probe index when the cursor remains in bounds
    emitter.instruction("xor edx, edx");                                        // wrap the probe cursor back to slot zero once the end of the table is reached

    emitter.label("__rt_hash_insert_owned_store_probe");
    emitter.instruction("mov QWORD PTR [rbp - 56], rdx");                       // persist the updated probe index before the next loop iteration
    emitter.instruction("jmp __rt_hash_insert_owned_probe");                    // continue probing until an empty slot or existing matching key is found

    emitter.label("__rt_hash_insert_owned_write");
    emitter.instruction("mov QWORD PTR [r12], 1");                              // mark the selected entry slot as occupied before filling its payload fields
    emitter.instruction("mov r13, QWORD PTR [rbp - 16]");                       // reload the owned key pointer that already belongs to the destination hash table
    emitter.instruction("mov QWORD PTR [r12 + 8], r13");                        // store the owned key pointer into the selected hash entry without repersisting it
    emitter.instruction("mov r13, QWORD PTR [rbp - 24]");                       // reload the owned key length that already belongs to the destination hash table
    emitter.instruction("mov QWORD PTR [r12 + 16], r13");                       // store the owned key length into the selected hash entry without repersisting it
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
    emitter.instruction("jne __rt_hash_insert_owned_link_tail");                // existing tables need an extra step to wire the previous tail forward to the inserted slot
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the inserted slot index to seed the insertion-order head pointer
    emitter.instruction("mov QWORD PTR [r10 + 24], r11");                       // header[24]: initialize the insertion-order head for the first occupied entry
    emitter.instruction("mov QWORD PTR [r10 + 32], r11");                       // header[32]: initialize the insertion-order tail for the first occupied entry
    emitter.instruction("jmp __rt_hash_insert_owned_bump_count");               // skip the previous-tail forward-link update on the very first insertion

    emitter.label("__rt_hash_insert_owned_link_tail");
    emitter.instruction("mov r14, r13");                                        // copy the previous tail slot index before scaling it into a byte offset
    emitter.instruction("shl r14, 6");                                          // convert the previous tail slot index into a 64-byte entry offset
    emitter.instruction("add r14, r10");                                        // advance from the hash-table base pointer to the previous tail entry block
    emitter.instruction("add r14, 40");                                         // skip the fixed hash header to land on the previous tail entry
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the inserted slot index for the previous-tail forward-link store
    emitter.instruction("mov QWORD PTR [r14 + 56], r11");                       // update the previous tail entry to point at the inserted slot as its logical successor
    emitter.instruction("mov QWORD PTR [r10 + 32], r11");                       // publish the inserted slot as the new insertion-order tail in the hash header

    emitter.label("__rt_hash_insert_owned_bump_count");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the current live-entry count from the hash header before incrementing it
    emitter.instruction("add r11, 1");                                          // increment the live-entry count after claiming a previously empty hash slot
    emitter.instruction("mov QWORD PTR [r10], r11");                            // store the updated live-entry count back into the hash header
    emitter.instruction("mov rax, r10");                                        // return the destination hash-table pointer after a successful owned insertion
    emitter.instruction("mov r14, QWORD PTR [rbp - 88]");                       // restore the caller's r14 before leaving the owned-insert helper
    emitter.instruction("mov r13, QWORD PTR [rbp - 80]");                       // restore the caller's r13 before leaving the owned-insert helper
    emitter.instruction("mov r12, QWORD PTR [rbp - 72]");                       // restore the caller's r12 before leaving the owned-insert helper
    emitter.instruction("add rsp, 96");                                         // release the local spill area that held the saved table/key/value tuple
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to the insertion caller
    emitter.instruction("ret");                                                 // return to the caller with the destination hash-table pointer in rax

    emitter.label("__rt_hash_insert_owned_overwrite");
    emitter.instruction("mov r13, QWORD PTR [rbp - 32]");                       // reload the replacement low payload word for the existing key slot
    emitter.instruction("mov QWORD PTR [r12 + 24], r13");                       // overwrite the stored low payload word in the existing hash entry
    emitter.instruction("mov r13, QWORD PTR [rbp - 40]");                       // reload the replacement high payload word for the existing key slot
    emitter.instruction("mov QWORD PTR [r12 + 32], r13");                       // overwrite the stored high payload word in the existing hash entry
    emitter.instruction("mov r13, QWORD PTR [rbp - 48]");                       // reload the replacement runtime value tag for the existing key slot
    emitter.instruction("mov QWORD PTR [r12 + 40], r13");                       // overwrite the stored runtime value tag in the existing hash entry
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the unchanged destination hash-table pointer after an in-place value overwrite
    emitter.instruction("mov r14, QWORD PTR [rbp - 88]");                       // restore the caller's r14 before leaving the owned-overwrite path
    emitter.instruction("mov r13, QWORD PTR [rbp - 80]");                       // restore the caller's r13 before leaving the owned-overwrite path
    emitter.instruction("mov r12, QWORD PTR [rbp - 72]");                       // restore the caller's r12 before leaving the owned-overwrite path
    emitter.instruction("add rsp, 96");                                         // release the local spill area before leaving the owned-overwrite path
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to the insertion caller
    emitter.instruction("ret");                                                 // return to the caller with the destination hash-table pointer in rax
}
