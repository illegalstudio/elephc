use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_fill_keys_refcounted: create an associative array from string keys and a borrowed refcounted payload.
/// Input:  x0=keys_array (string array), x1=borrowed heap pointer
/// Output: x0=new hash table
pub fn emit_array_fill_keys_refcounted(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_fill_keys_refcounted_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_fill_keys_refcounted ---");
    emitter.label_global("__rt_array_fill_keys_refcounted");

    // -- set up stack frame, save arguments --
    emitter.instruction("sub sp, sp, #80");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save keys array pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save borrowed fill payload
    emitter.instruction("str x2, [sp, #32]");                                   // save result value_type tag

    // -- create hash table with capacity = max(length * 2, 16) --
    emitter.instruction("ldr x0, [x0]");                                        // load keys array length
    emitter.instruction("lsl x0, x0, #1");                                      // compute capacity with headroom
    emitter.instruction("mov x9, #16");                                         // load minimum capacity
    emitter.instruction("cmp x0, x9");                                          // compare requested capacity with minimum
    emitter.instruction("csel x0, x9, x0, lt");                                 // clamp capacity to at least 16
    emitter.instruction("ldr x1, [sp, #32]");                                   // x1 = requested result value_type tag
    emitter.instruction("bl __rt_hash_new");                                    // allocate result hash table
    emitter.instruction("str x0, [sp, #16]");                                   // save result hash pointer
    emitter.instruction("str xzr, [sp, #24]");                                  // initialize loop index

    emitter.label("__rt_array_fill_keys_ref_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload keys array pointer
    emitter.instruction("ldr x3, [x0]");                                        // load keys array length
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload loop index
    emitter.instruction("cmp x4, x3");                                          // compare loop index with key count
    emitter.instruction("b.ge __rt_array_fill_keys_ref_done");                  // finish once every key has been inserted
    emitter.instruction("lsl x5, x4, #4");                                      // compute byte offset for string key element
    emitter.instruction("add x5, x0, x5");                                      // move to keyed string element
    emitter.instruction("add x5, x5, #24");                                     // skip array header
    emitter.instruction("ldr x1, [x5]");                                        // load key pointer
    emitter.instruction("ldr x2, [x5, #8]");                                    // load key length
    emitter.instruction("str x1, [sp, #40]");                                   // preserve key pointer across incref
    emitter.instruction("str x2, [sp, #48]");                                   // preserve key length across incref
    emitter.instruction("ldr x3, [sp, #8]");                                    // reload borrowed fill payload
    emitter.instruction("str x3, [sp, #56]");                                   // preserve borrowed fill payload across incref
    emitter.instruction("mov x0, x3");                                          // move borrowed fill payload into incref argument register
    emitter.instruction("bl __rt_incref");                                      // retain borrowed fill payload before hash_set takes ownership
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload result hash pointer
    emitter.instruction("ldr x1, [sp, #40]");                                   // restore key pointer
    emitter.instruction("ldr x2, [sp, #48]");                                   // restore key length
    emitter.instruction("ldr x3, [sp, #56]");                                   // restore retained fill payload
    emitter.instruction("mov x4, #0");                                          // value_hi unused for 8-byte refcounted payloads
    emitter.instruction("ldr x5, [sp, #32]");                                   // x5 = value_tag for the retained fill payload
    emitter.instruction("bl __rt_hash_set");                                    // insert retained fill payload into the result hash
    emitter.instruction("str x0, [sp, #16]");                                   // persist hash pointer after possible growth
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload loop index
    emitter.instruction("add x4, x4, #1");                                      // increment loop index
    emitter.instruction("str x4, [sp, #24]");                                   // persist updated loop index
    emitter.instruction("b __rt_array_fill_keys_ref_loop");                     // continue filling keys

    emitter.label("__rt_array_fill_keys_ref_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload result hash pointer
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return result hash
}

fn emit_array_fill_keys_refcounted_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_fill_keys_refcounted ---");
    emitter.label_global("__rt_array_fill_keys_refcounted");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving associative-array refcounted-fill spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for keys, payload, hash pointer, loop index, and value tag bookkeeping
    emitter.instruction("sub rsp, 64");                                         // reserve aligned spill slots for keys, payload, hash pointer, loop index, value tag, and the current string key pair
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the indexed array of keys across hash allocation, incref, and repeated insert helper calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the borrowed heap payload across hash allocation, incref, and repeated insert helper calls
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // preserve the requested associative-array value_type tag across hash allocation, incref, and repeated insert helper calls
    emitter.instruction("mov rdi, QWORD PTR [rdi]");                            // load the indexed array key count and place it in the first x86_64 hash-constructor argument register
    emitter.instruction("shl rdi, 1");                                          // double the indexed array key count to provide the associative-array constructor some insertion headroom
    emitter.instruction("cmp rdi, 16");                                         // clamp the requested associative-array capacity to the minimum bucket count expected by the hash runtime
    emitter.instruction("jge __rt_array_fill_keys_ref_capacity_x86");           // keep the doubled key-count capacity when it already meets the minimum bucket count
    emitter.instruction("mov rdi, 16");                                         // fall back to the minimum associative-array bucket count for very small key arrays
    emitter.label("__rt_array_fill_keys_ref_capacity_x86");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // pass the requested associative-array value_type tag to the x86_64 hash constructor
    emitter.instruction("call __rt_hash_new");                                  // allocate the destination associative array through the shared x86_64 hash constructor
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the destination associative-array pointer across repeated incref and insert helper calls
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // initialize the indexed-array key loop index to the first key slot
    emitter.label("__rt_array_fill_keys_ref_loop_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the indexed-array key loop index before loading the next string key slot
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the indexed array of keys before reading the key-count loop bound
    emitter.instruction("cmp rcx, QWORD PTR [r10]");                            // compare the current key loop index against the indexed-array key count
    emitter.instruction("jge __rt_array_fill_keys_ref_done_x86");               // finish once every indexed-array key slot has been inserted into the associative array
    emitter.instruction("mov r11, rcx");                                        // copy the indexed-array key loop index before scaling it to the 16-byte string slot size
    emitter.instruction("shl r11, 4");                                          // scale the indexed-array key loop index by the 16-byte string slot size
    emitter.instruction("add r10, r11");                                        // advance from the indexed-array base pointer to the selected string key slot
    emitter.instruction("add r10, 24");                                         // skip the indexed-array header to reach the selected string key slot payload
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the current key pointer from the selected indexed-array string slot
    emitter.instruction("mov QWORD PTR [rbp - 48], r11");                       // preserve the current key pointer across the incref helper call
    emitter.instruction("mov r11, QWORD PTR [r10 + 8]");                        // load the current key length from the selected indexed-array string slot
    emitter.instruction("mov QWORD PTR [rbp - 56], r11");                       // preserve the current key length across the incref helper call
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // move the borrowed heap payload into the x86_64 incref input register
    emitter.instruction("call __rt_incref");                                    // retain the borrowed heap payload before the associative-array insert helper becomes an owner
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the destination associative-array pointer before inserting the current key/value pair
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // restore the current key pointer after the incref helper clobbered caller-saved registers
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");                       // restore the current key length after the incref helper clobbered caller-saved registers
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the retained heap payload into the x86_64 hash insertion low-word register
    emitter.instruction("xor r8d, r8d");                                        // clear the x86_64 hash insertion high-word register because retained heap payloads use only the low payload word
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the requested associative-array value_type tag into the x86_64 hash insertion tag register
    emitter.instruction("call __rt_hash_set");                                  // insert the current key plus retained heap payload into the destination associative array
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // persist the possibly-grown destination associative-array pointer after hash insertion
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the indexed-array key loop index after hash insertion clobbered caller-saved registers
    emitter.instruction("add r10, 1");                                          // advance the indexed-array key loop index after inserting one key/value pair
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // persist the updated indexed-array key loop index across the next insertion helper call
    emitter.instruction("jmp __rt_array_fill_keys_ref_loop_x86");               // continue inserting indexed-array string keys into the destination associative array
    emitter.label("__rt_array_fill_keys_ref_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the filled associative-array pointer in the standard x86_64 integer result register
    emitter.instruction("add rsp, 64");                                         // release the associative-array refcounted-fill spill slots before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                 // return the filled associative-array pointer in rax
}
