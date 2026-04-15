use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_combine_refcounted: create an associative array from string keys and refcounted values.
/// Input:  x0=keys_array (string array), x1=values_array (refcounted payload array)
/// Output: x0=new hash table
pub fn emit_array_combine_refcounted(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_combine_refcounted_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_combine_refcounted ---");
    emitter.label_global("__rt_array_combine_refcounted");

    // -- set up stack frame, save arguments --
    emitter.instruction("sub sp, sp, #80");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save keys array pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save values array pointer
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

    emitter.label("__rt_array_combine_ref_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload keys array pointer
    emitter.instruction("ldr x3, [x0]");                                        // load keys array length
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload loop index
    emitter.instruction("cmp x4, x3");                                          // compare loop index with key count
    emitter.instruction("b.ge __rt_array_combine_ref_done");                    // finish once every key/value pair has been inserted
    emitter.instruction("lsl x5, x4, #4");                                      // compute byte offset for string key element
    emitter.instruction("add x5, x0, x5");                                      // move to keyed string element
    emitter.instruction("add x5, x5, #24");                                     // skip array header
    emitter.instruction("ldr x1, [x5]");                                        // load key pointer
    emitter.instruction("ldr x2, [x5, #8]");                                    // load key length
    emitter.instruction("str x1, [sp, #40]");                                   // preserve key pointer across incref
    emitter.instruction("str x2, [sp, #48]");                                   // preserve key length across incref
    emitter.instruction("ldr x5, [sp, #8]");                                    // reload values array pointer
    emitter.instruction("add x5, x5, #24");                                     // compute values data base
    emitter.instruction("ldr x3, [x5, x4, lsl #3]");                            // load borrowed refcounted value
    emitter.instruction("str x3, [sp, #56]");                                   // preserve borrowed value across incref
    emitter.instruction("mov x0, x3");                                          // move borrowed value into incref argument register
    emitter.instruction("bl __rt_incref");                                      // retain borrowed value before hash_set takes ownership
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload result hash pointer
    emitter.instruction("ldr x1, [sp, #40]");                                   // restore key pointer
    emitter.instruction("ldr x2, [sp, #48]");                                   // restore key length
    emitter.instruction("ldr x3, [sp, #56]");                                   // restore retained value pointer
    emitter.instruction("mov x4, #0");                                          // value_hi unused for 8-byte refcounted payloads
    emitter.instruction("ldr x5, [sp, #32]");                                   // x5 = value_tag for the retained value payload
    emitter.instruction("bl __rt_hash_set");                                    // insert retained value into the result hash
    emitter.instruction("str x0, [sp, #16]");                                   // persist hash pointer after possible growth
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload loop index
    emitter.instruction("add x4, x4, #1");                                      // increment loop index
    emitter.instruction("str x4, [sp, #24]");                                   // persist updated loop index
    emitter.instruction("b __rt_array_combine_ref_loop");                       // continue combining

    emitter.label("__rt_array_combine_ref_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload result hash pointer
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return result hash
}

fn emit_array_combine_refcounted_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_combine_refcounted ---");
    emitter.label_global("__rt_array_combine_refcounted");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving refcounted array-combine spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the keys array, values array, destination hash, loop index, and retained value scratch
    emitter.instruction("sub rsp, 64");                                         // reserve aligned spill slots for the refcounted array-combine bookkeeping while keeping helper calls 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the string-key indexed array across nested hash-constructor and insertion helper calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the refcounted-values indexed array across nested hash-constructor and insertion helper calls
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // preserve the requested hash value_type tag across nested hash-constructor and insertion helper calls
    emitter.instruction("mov rdi, QWORD PTR [rdi]");                            // load the string-key indexed-array logical length before deriving the destination hash capacity
    emitter.instruction("shl rdi, 1");                                          // double the string-key count to give the destination hash some insertion headroom
    emitter.instruction("cmp rdi, 16");                                         // clamp the destination hash capacity to the minimum bucket count expected by the runtime
    emitter.instruction("jge __rt_array_combine_ref_capacity_x86");             // keep the doubled key-count capacity when it already meets the minimum bucket count
    emitter.instruction("mov rdi, 16");                                         // fall back to the minimum destination hash capacity for very small key arrays
    emitter.label("__rt_array_combine_ref_capacity_x86");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // pass the requested hash value_type tag to the shared x86_64 hash constructor
    emitter.instruction("call __rt_hash_new");                                  // allocate the destination hash table through the shared x86_64 hash constructor
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve the destination hash pointer across repeated insertion helper calls
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // initialize the array-combine loop index to the first key/value pair

    emitter.label("__rt_array_combine_ref_loop_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // reload the array-combine loop index before reading the next key/value pair
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the string-key indexed array before reading its logical length and selected key slot
    emitter.instruction("cmp rcx, QWORD PTR [r10]");                            // compare the loop index against the string-key indexed-array logical length
    emitter.instruction("jge __rt_array_combine_ref_done_x86");                 // finish once every key/value pair has been inserted into the destination hash table
    emitter.instruction("mov r11, rcx");                                        // copy the loop index before scaling it to the 16-byte string slot size
    emitter.instruction("shl r11, 4");                                          // scale the loop index by the 16-byte string slot size used by the key array
    emitter.instruction("add r10, r11");                                        // advance from the key-array base pointer to the selected string key slot
    emitter.instruction("add r10, 24");                                         // skip the indexed-array header to reach the selected string key payload
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the selected key pointer from the string-key indexed array
    emitter.instruction("mov QWORD PTR [rbp - 48], r11");                       // preserve the selected key pointer across incref and hash insertion helper calls
    emitter.instruction("mov r11, QWORD PTR [r10 + 8]");                        // load the selected key length from the string-key indexed array
    emitter.instruction("mov QWORD PTR [rbp - 56], r11");                       // preserve the selected key length across incref and hash insertion helper calls
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the refcounted-values indexed array before reading the selected value payload
    emitter.instruction("lea r10, [r10 + 24]");                                 // compute the payload base address for the refcounted-values indexed array
    emitter.instruction("mov r11, QWORD PTR [r10 + rcx * 8]");                  // load the selected borrowed refcounted value payload from the values indexed array
    emitter.instruction("mov QWORD PTR [rbp - 64], r11");                       // preserve the selected borrowed refcounted value payload across the incref helper call
    emitter.instruction("mov rdi, r11");                                        // pass the selected borrowed refcounted value payload to the shared x86_64 incref helper
    emitter.instruction("call __rt_incref");                                    // retain the borrowed refcounted value payload before the destination hash takes ownership of it
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // reload the destination hash pointer before inserting the retained key/value pair
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // reload the selected key pointer into the first key-argument register for hash insertion
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");                       // reload the selected key length into the second key-argument register for hash insertion
    emitter.instruction("mov rcx, QWORD PTR [rbp - 64]");                       // reload the retained refcounted value payload into the low-word hash insertion register
    emitter.instruction("xor r8d, r8d");                                        // clear the high-word hash insertion register because refcounted array-combine payloads use only the low word
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload the requested hash value_type tag into the hash insertion tag register
    emitter.instruction("call __rt_hash_set");                                  // insert the selected key plus retained refcounted payload into the destination hash table
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // persist the possibly-grown destination hash pointer after hash insertion
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");                       // reload the array-combine loop index after helper calls clobbered caller-saved registers
    emitter.instruction("add r10, 1");                                          // advance the loop index after inserting one key/value pair
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // persist the updated array-combine loop index across the next insertion helper call
    emitter.instruction("jmp __rt_array_combine_ref_loop_x86");                 // continue combining string keys with refcounted values into the destination hash table

    emitter.label("__rt_array_combine_ref_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // return the destination hash pointer in the standard x86_64 integer result register
    emitter.instruction("add rsp, 64");                                         // release the refcounted array-combine spill slots before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to the caller
    emitter.instruction("ret");                                                 // return the refcounted array-combine destination hash pointer in rax
}
