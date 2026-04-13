use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// hash_free_deep: free a hash table and all owned keys / heap-backed values.
/// Input:  x0 = hash table pointer
/// Output: none
pub fn emit_hash_free_deep(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hash_free_deep_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: hash_free_deep ---");
    emitter.label_global("__rt_hash_free_deep");

    // -- null and heap-range check --
    emitter.instruction("cbz x0, __rt_hash_free_deep_done");                    // skip if null
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_heap_buf");
    emitter.instruction("cmp x0, x9");                                          // is table below heap start?
    emitter.instruction("b.lo __rt_hash_free_deep_done");                       // skip non-heap pointers
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_heap_off");
    emitter.instruction("ldr x10, [x10]");                                      // load current heap offset
    emitter.instruction("add x10, x9, x10");                                    // compute current heap end
    emitter.instruction("cmp x0, x10");                                         // is table at or beyond heap end?
    emitter.instruction("b.hs __rt_hash_free_deep_done");                       // skip invalid pointers

    // -- set up stack frame --
    // Stack layout:
    //   [sp, #0]  = hash table pointer
    //   [sp, #8]  = capacity
    //   [sp, #16] = scratch
    //   [sp, #24] = loop index
    //   [sp, #32] = saved x29
    //   [sp, #40] = saved x30
    emitter.instruction("sub sp, sp, #48");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save hash table pointer
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_gc_release_suppressed");
    emitter.instruction("mov x10, #1");                                         // ordinary deep-free walks suppress nested collector runs
    emitter.instruction("str x10, [x9]");                                       // store release-suppressed = 1 for child cleanup
    emitter.instruction("ldr x9, [x0, #8]");                                    // load table capacity
    emitter.instruction("str x9, [sp, #8]");                                    // save capacity for the loop
    emitter.instruction("str xzr, [sp, #24]");                                  // loop index = 0

    // -- iterate all slots and free occupied entries --
    emitter.label("__rt_hash_free_deep_loop");
    emitter.instruction("ldr x11, [sp, #24]");                                  // reload loop index
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload capacity
    emitter.instruction("cmp x11, x10");                                        // are we done scanning all slots?
    emitter.instruction("b.ge __rt_hash_free_deep_struct");                     // finish once index reaches capacity

    emitter.instruction("ldr x9, [sp, #0]");                                    // reload hash table pointer
    emitter.instruction("mov x12, #64");                                        // entry size = 64 bytes with per-entry tags and insertion-order links
    emitter.instruction("mul x13, x11, x12");                                   // compute byte offset for this slot
    emitter.instruction("add x13, x9, x13");                                    // advance from table base to slot
    emitter.instruction("add x13, x13, #40");                                   // skip hash header to entry storage
    emitter.instruction("ldr x14, [x13]");                                      // load occupied flag
    emitter.instruction("cmp x14, #1");                                         // is this slot occupied?
    emitter.instruction("b.ne __rt_hash_free_deep_next");                       // skip empty or tombstone slots

    // -- decref the key string for this entry (keys may be shared after COW clone) --
    emitter.instruction("ldr x0, [x13, #8]");                                   // load key pointer
    emitter.instruction("str x11, [sp, #24]");                                  // preserve loop index across helper call
    emitter.instruction("ldr w15, [x0, #-12]");                                 // load key refcount from heap header
    emitter.instruction("subs w15, w15, #1");                                   // decrement key refcount
    emitter.instruction("str w15, [x0, #-12]");                                 // store decremented refcount
    emitter.instruction("b.ne __rt_hash_free_deep_key_shared");                 // refcount > 0: key is shared, skip free
    emitter.instruction("bl __rt_heap_free");                                   // refcount = 0: free the key storage
    emitter.label("__rt_hash_free_deep_key_shared");
    emitter.instruction("ldr x11, [sp, #24]");                                  // restore loop index after key handling

    // -- free the entry value based on the runtime value tag --
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload hash table pointer after helper call
    emitter.instruction("mov x12, #64");                                        // entry size = 64 bytes with per-entry tags and insertion-order links
    emitter.instruction("mul x13, x11, x12");                                   // recompute byte offset for this slot
    emitter.instruction("add x13, x9, x13");                                    // advance from table base to slot
    emitter.instruction("add x13, x13, #40");                                   // skip hash header to entry storage
    emitter.instruction("ldr x14, [x13, #40]");                                 // reload this entry's runtime value_tag
    emitter.instruction("cmp x14, #1");                                         // is the entry value heap-backed at all?
    emitter.instruction("b.eq __rt_hash_free_deep_value_any");                  // strings release through the uniform dispatch helper
    emitter.instruction("cmp x14, #4");                                         // is this a nested indexed array?
    emitter.instruction("b.eq __rt_hash_free_deep_value_any");                  // arrays release through the uniform dispatch helper
    emitter.instruction("cmp x14, #5");                                         // is this a nested associative array?
    emitter.instruction("b.eq __rt_hash_free_deep_value_any");                  // hashes release through the uniform dispatch helper
    emitter.instruction("cmp x14, #6");                                         // is this a nested object / callable?
    emitter.instruction("b.eq __rt_hash_free_deep_value_any");                  // objects release through the uniform dispatch helper
    emitter.instruction("cmp x14, #7");                                         // is this a boxed mixed value?
    emitter.instruction("b.eq __rt_hash_free_deep_value_any");                  // mixed cells release through the uniform dispatch helper
    emitter.instruction("b __rt_hash_free_deep_next");                          // plain scalars need no cleanup

    emitter.label("__rt_hash_free_deep_value_any");
    emitter.instruction("ldr x0, [x13, #24]");                                  // load the heap-backed value pointer from the entry payload
    emitter.instruction("str x11, [sp, #24]");                                  // preserve loop index across helper call
    emitter.instruction("bl __rt_decref_any");                                  // release the heap-backed value through the uniform dispatcher
    emitter.instruction("ldr x11, [sp, #24]");                                  // restore loop index after helper call

    emitter.label("__rt_hash_free_deep_next");
    emitter.instruction("add x11, x11, #1");                                    // advance to the next slot
    emitter.instruction("str x11, [sp, #24]");                                  // save updated loop index
    emitter.instruction("b __rt_hash_free_deep_loop");                          // continue scanning entries

    // -- free the hash table struct itself --
    emitter.label("__rt_hash_free_deep_struct");
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_gc_release_suppressed");
    emitter.instruction("str xzr, [x9]");                                       // clear release suppression before freeing the container storage
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload hash table pointer
    emitter.instruction("bl __rt_heap_free");                                   // free the hash table storage
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame

    emitter.label("__rt_hash_free_deep_done");
    emitter.instruction("ret");                                                 // return to caller
}

fn emit_hash_free_deep_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_free_deep ---");
    emitter.label_global("__rt_hash_free_deep");

    emitter.instruction("test rax, rax");                                       // skip null hash pointers immediately because they do not own heap storage
    emitter.instruction("jz __rt_hash_free_deep_done");                         // null hashes need no release work
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // load the stamped x86_64 heap kind word from the uniform header
    emitter.instruction("shr r10, 32");                                         // isolate the high-word heap marker used by the x86_64 heap wrapper
    emitter.instruction(&format!("cmp r10d, 0x{:x}", X86_64_HEAP_MAGIC_HI32));  // ignore foreign pointers that do not carry the elephc x86_64 heap marker
    emitter.instruction("jne __rt_hash_free_deep_done");                        // only elephc-owned hash tables participate in x86_64 deep-free bookkeeping
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving hash-free spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved hash pointer, capacity, and loop index
    emitter.instruction("sub rsp, 32");                                         // reserve local storage for the hash pointer, capacity, loop index, and entry scratch state
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the hash-table pointer across nested helper calls while freeing entries
    emitter.instruction("mov r10, QWORD PTR [rax + 8]");                        // load the table capacity before entering the entry-scan loop
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // save the table capacity so the entry-scan loop can survive nested helper calls
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // initialize the entry-scan loop index to zero

    emitter.label("__rt_hash_free_deep_loop");
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the current entry-scan index at the top of every loop iteration
    emitter.instruction("cmp r10, QWORD PTR [rbp - 16]");                       // stop once every slot up to the stored capacity has been inspected
    emitter.instruction("jae __rt_hash_free_deep_struct");                      // exit the loop when the entry-scan index reaches the table capacity
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload the hash-table pointer after any nested helper call
    emitter.instruction("mov rcx, r10");                                        // copy the current entry-scan index before scaling it into a byte offset
    emitter.instruction("shl rcx, 6");                                          // convert the entry index into a 64-byte entry offset
    emitter.instruction("add rcx, r11");                                        // advance from the hash-table base pointer to the selected entry block
    emitter.instruction("add rcx, 40");                                         // skip the fixed 40-byte hash header to land on the selected entry
    emitter.instruction("mov r8, QWORD PTR [rcx]");                             // load the occupied marker for the current hash-entry slot
    emitter.instruction("cmp r8, 1");                                           // only fully occupied hash-entry slots own key/value payloads that need cleanup
    emitter.instruction("jne __rt_hash_free_deep_next");                        // skip empty or tombstone slots during the deep-free scan
    emitter.instruction("mov rax, QWORD PTR [rcx + 8]");                        // load the persisted key pointer for the current hash-entry slot
    emitter.instruction("test rax, rax");                                       // skip missing keys defensively even though occupied entries should normally have one
    emitter.instruction("jz __rt_hash_free_deep_value");                        // move on to the entry value cleanup when the key pointer is unexpectedly null
    emitter.instruction("mov r9, QWORD PTR [rax - 8]");                         // load the key heap kind word so foreign pointers are ignored safely
    emitter.instruction("shr r9, 32");                                          // isolate the high-word heap marker used by the x86_64 heap wrapper
    emitter.instruction(&format!("cmp r9d, 0x{:x}", X86_64_HEAP_MAGIC_HI32));   // only elephc-owned persisted keys participate in x86_64 key decref bookkeeping
    emitter.instruction("jne __rt_hash_free_deep_value");                       // skip foreign key pointers rather than trying to mutate a missing heap header
    emitter.instruction("mov r9d, DWORD PTR [rax - 12]");                       // load the persisted key refcount from the uniform heap header
    emitter.instruction("sub r9d, 1");                                          // decrement the key refcount because this hash table is releasing its ownership
    emitter.instruction("mov DWORD PTR [rax - 12], r9d");                       // store the decremented key refcount back into the uniform heap header
    emitter.instruction("jnz __rt_hash_free_deep_value");                       // keep shared keys alive when another owner still holds a reference
    emitter.instruction("call __rt_heap_free");                                 // free the persisted key storage once the last owner releases it

    emitter.label("__rt_hash_free_deep_value");
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the current entry-scan index after the optional key-release helper call
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload the hash-table pointer after the optional key-release helper call
    emitter.instruction("mov rcx, r10");                                        // copy the current entry-scan index before recomputing the entry address
    emitter.instruction("shl rcx, 6");                                          // convert the entry index into a 64-byte entry offset again after helper calls
    emitter.instruction("add rcx, r11");                                        // advance from the hash-table base pointer back to the selected entry block
    emitter.instruction("add rcx, 40");                                         // skip the fixed 40-byte hash header to land on the selected entry again
    emitter.instruction("mov r8, QWORD PTR [rcx + 40]");                        // load the runtime value tag to decide whether the entry payload owns heap storage
    emitter.instruction("cmp r8, 1");                                           // detect string entry payloads that own a persisted string allocation
    emitter.instruction("je __rt_hash_free_deep_value_string");                 // release persisted string values through the safe heap-free helper
    emitter.instruction("cmp r8, 5");                                           // detect nested associative-array payloads in the current bootstrap subset
    emitter.instruction("je __rt_hash_free_deep_value_hash");                   // release nested associative-array payloads through hash decref
    emitter.instruction("cmp r8, 7");                                           // detect boxed mixed payloads in the current bootstrap subset
    emitter.instruction("je __rt_hash_free_deep_value_mixed");                  // release boxed mixed payloads through mixed decref
    emitter.instruction("jmp __rt_hash_free_deep_next");                        // plain scalar payloads do not require any additional cleanup

    emitter.label("__rt_hash_free_deep_value_string");
    emitter.instruction("mov rax, QWORD PTR [rcx + 24]");                       // load the persisted string pointer stored in the current hash-entry payload
    emitter.instruction("call __rt_heap_free_safe");                            // release the persisted string payload owned by the current hash entry
    emitter.instruction("jmp __rt_hash_free_deep_next");                        // continue scanning entries after releasing the current string payload

    emitter.label("__rt_hash_free_deep_value_hash");
    emitter.instruction("mov rax, QWORD PTR [rcx + 24]");                       // load the nested associative-array pointer stored in the current hash-entry payload
    emitter.instruction("call __rt_decref_hash");                               // release the nested associative-array payload through the x86_64 hash decref helper
    emitter.instruction("jmp __rt_hash_free_deep_next");                        // continue scanning entries after releasing the nested associative-array payload

    emitter.label("__rt_hash_free_deep_value_mixed");
    emitter.instruction("mov rax, QWORD PTR [rcx + 24]");                       // load the boxed mixed pointer stored in the current hash-entry payload
    emitter.instruction("call __rt_decref_mixed");                              // release the boxed mixed payload through the x86_64 mixed decref helper

    emitter.label("__rt_hash_free_deep_next");
    emitter.instruction("add QWORD PTR [rbp - 24], 1");                         // advance the entry-scan index to the next hash slot
    emitter.instruction("jmp __rt_hash_free_deep_loop");                        // continue scanning occupied entries until the whole table has been released

    emitter.label("__rt_hash_free_deep_struct");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the hash-table pointer after finishing the deep-free scan
    emitter.instruction("call __rt_heap_free");                                 // release the hash-table storage itself through the x86_64 heap wrapper
    emitter.instruction("add rsp, 32");                                         // release the spill slots reserved for the hash-free scan state
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to generated code
    emitter.label("__rt_hash_free_deep_done");
    emitter.instruction("ret");                                                 // return to the caller after releasing the hash table and its owned entries
}
