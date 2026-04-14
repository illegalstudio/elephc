use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_combine: create associative array from keys array + values array.
/// Input:  x0=keys_array (string array), x1=values_array, x2=value_type_tag
/// Output: x0=new hash table
/// Both arrays must have the same length.
pub fn emit_array_combine(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_combine_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_combine ---");
    emitter.label_global("__rt_array_combine");

    // -- set up stack frame, save arguments --
    // Stack layout:
    //   [sp, #0]  = keys array pointer
    //   [sp, #8]  = values array pointer
    //   [sp, #16] = hash table pointer (result)
    //   [sp, #24] = loop index i
    //   [sp, #32] = result value_tag
    //   [sp, #48] = saved x29
    //   [sp, #56] = saved x30
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save keys array pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save values array pointer
    emitter.instruction("str x2, [sp, #32]");                                   // save result value_type tag

    // -- create hash table with capacity = length * 2 --
    emitter.instruction("ldr x0, [x0]");                                        // x0 = keys array length
    emitter.instruction("lsl x0, x0, #1");                                      // x0 = length * 2 (capacity with headroom)
    emitter.instruction("mov x9, #16");                                         // x9 = minimum capacity
    emitter.instruction("cmp x0, x9");                                          // compare with minimum
    emitter.instruction("csel x0, x9, x0, lt");                                 // if length*2 < 16, use 16
    emitter.instruction("ldr x1, [sp, #32]");                                   // x1 = requested result value_type tag
    emitter.instruction("bl __rt_hash_new");                                    // create hash table, x0 = hash ptr
    emitter.instruction("str x0, [sp, #16]");                                   // save hash table pointer

    // -- loop over array elements --
    emitter.instruction("str xzr, [sp, #24]");                                  // i = 0

    emitter.label("__rt_array_combine_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload keys array pointer
    emitter.instruction("ldr x3, [x0]");                                        // x3 = keys array length
    emitter.instruction("ldr x4, [sp, #24]");                                   // x4 = i
    emitter.instruction("cmp x4, x3");                                          // compare i with length
    emitter.instruction("b.ge __rt_array_combine_done");                        // if i >= length, done

    // -- load key string from keys[i] (16 bytes per string element) --
    emitter.instruction("lsl x5, x4, #4");                                      // x5 = i * 16 (byte offset for string element)
    emitter.instruction("add x5, x0, x5");                                      // x5 = keys_array + byte offset
    emitter.instruction("add x5, x5, #24");                                     // x5 = skip header to data region
    emitter.instruction("ldr x1, [x5]");                                        // x1 = key_ptr = keys[i].ptr
    emitter.instruction("ldr x2, [x5, #8]");                                    // x2 = key_len = keys[i].len

    // -- load value from values[i] (8 bytes per int element) --
    emitter.instruction("ldr x5, [sp, #8]");                                    // reload values array pointer
    emitter.instruction("add x5, x5, #24");                                     // x5 = values data base
    emitter.instruction("ldr x3, [x5, x4, lsl #3]");                            // x3 = values[i]
    emitter.instruction("mov x4, #0");                                          // x4 = value_hi = 0

    // -- call hash_set --
    emitter.instruction("ldr x0, [sp, #16]");                                   // x0 = hash table pointer
    emitter.instruction("ldr x5, [sp, #32]");                                   // x5 = value_tag for values[i]
    emitter.instruction("bl __rt_hash_set");                                    // insert key-value pair
    emitter.instruction("str x0, [sp, #16]");                                   // update hash table pointer after possible growth

    // -- advance loop --
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload i
    emitter.instruction("add x4, x4, #1");                                      // i += 1
    emitter.instruction("str x4, [sp, #24]");                                   // save updated i
    emitter.instruction("b __rt_array_combine_loop");                           // continue loop

    // -- return hash table --
    emitter.label("__rt_array_combine_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // x0 = hash table pointer
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = hash table
}

fn emit_array_combine_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_combine ---");
    emitter.label_global("__rt_array_combine");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving array-combine spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for keys, values, result hash, loop index, and value-tag bookkeeping
    emitter.instruction("sub rsp, 48");                                         // reserve aligned spill slots for the scalar array-combine bookkeeping while keeping nested calls 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the string-key indexed array across nested hash-constructor and insertion helper calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the scalar-value indexed array across nested hash-constructor and insertion helper calls
    emitter.instruction("mov QWORD PTR [rbp - 40], rdx");                       // preserve the requested hash value_type tag across nested hash-constructor and insertion helper calls
    emitter.instruction("mov rdi, QWORD PTR [rdi]");                            // load the string-key indexed-array logical length before deriving the result hash capacity
    emitter.instruction("shl rdi, 1");                                          // double the string-key count to give the destination hash some insertion headroom
    emitter.instruction("cmp rdi, 16");                                         // clamp the destination hash capacity to the minimum bucket count expected by the runtime
    emitter.instruction("jge __rt_array_combine_capacity_x86");                 // keep the doubled key-count capacity when it already meets the minimum bucket count
    emitter.instruction("mov rdi, 16");                                         // fall back to the minimum destination hash capacity for very small key arrays
    emitter.label("__rt_array_combine_capacity_x86");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // pass the requested hash value_type tag to the shared x86_64 hash constructor
    emitter.instruction("call __rt_hash_new");                                  // allocate the destination hash table through the shared x86_64 hash constructor
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the destination hash pointer across repeated insertion helper calls
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // initialize the array-combine loop index to the first key/value pair
    emitter.label("__rt_array_combine_loop_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the array-combine loop index before reading the next key/value pair
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the string-key indexed array before reading its logical length and selected key slot
    emitter.instruction("cmp rcx, QWORD PTR [r10]");                            // compare the loop index against the string-key indexed-array logical length
    emitter.instruction("jge __rt_array_combine_done_x86");                     // finish once every key/value pair has been inserted into the destination hash table
    emitter.instruction("mov r11, rcx");                                        // copy the loop index before scaling it to the 16-byte string slot size
    emitter.instruction("shl r11, 4");                                          // scale the loop index by the 16-byte string slot size used by the key array
    emitter.instruction("add r10, r11");                                        // advance from the key-array base pointer to the selected string key slot
    emitter.instruction("add r10, 24");                                         // skip the indexed-array header to reach the selected string key slot payload
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the destination hash pointer before inserting the selected key/value pair
    emitter.instruction("mov rsi, QWORD PTR [r10]");                            // load the selected key pointer from the string-key indexed array
    emitter.instruction("mov rdx, QWORD PTR [r10 + 8]");                        // load the selected key length from the string-key indexed array
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the scalar-value indexed array before reading the selected scalar payload
    emitter.instruction("lea r10, [r10 + 24]");                                 // compute the payload base address for the scalar-value indexed array
    emitter.instruction("mov rcx, QWORD PTR [r10 + rcx * 8]");                  // load the selected scalar payload into the low-word hash insertion register
    emitter.instruction("xor r8d, r8d");                                        // clear the high-word hash insertion register because scalar array-combine payloads use only the low word
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the requested hash value_type tag into the hash insertion tag register
    emitter.instruction("call __rt_hash_set");                                  // insert the selected key plus scalar payload into the destination hash table
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // persist the possibly-grown destination hash pointer after hash insertion
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the array-combine loop index after helper calls clobbered caller-saved registers
    emitter.instruction("add r10, 1");                                          // advance the loop index after inserting one key/value pair
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // persist the updated array-combine loop index across the next insertion helper call
    emitter.instruction("jmp __rt_array_combine_loop_x86");                     // continue combining string keys with scalar values into the destination hash table
    emitter.label("__rt_array_combine_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the destination hash pointer in the standard x86_64 integer result register
    emitter.instruction("add rsp, 48");                                         // release the array-combine spill slots before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                 // return the destination hash pointer in rax
}
