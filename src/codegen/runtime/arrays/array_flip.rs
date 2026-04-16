use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_flip: swap keys and values for an indexed int array.
/// Creates a hash table where str(value) becomes the key and the index becomes the value.
/// Input:  x0=array_ptr (indexed int array)
/// Output: x0=new hash table
pub fn emit_array_flip(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_flip_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_flip ---");
    emitter.label_global("__rt_array_flip");

    // -- set up stack frame, save arguments --
    // Stack layout:
    //   [sp, #0]  = source array pointer
    //   [sp, #8]  = hash table pointer (result)
    //   [sp, #16] = loop index i
    //   [sp, #24] = saved x29
    //   [sp, #32] = saved x30
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save source array pointer

    // -- create hash table with capacity = array length * 2 (load factor) --
    emitter.instruction("ldr x0, [x0]");                                        // x0 = array length
    emitter.instruction("lsl x0, x0, #1");                                      // x0 = length * 2 (capacity with headroom)
    emitter.instruction("mov x9, #16");                                         // x9 = minimum capacity
    emitter.instruction("cmp x0, x9");                                          // compare with minimum
    emitter.instruction("csel x0, x9, x0, lt");                                 // if length*2 < 16, use 16
    emitter.instruction("mov x1, #0");                                          // value_type = 0 (int)
    emitter.instruction("bl __rt_hash_new");                                    // create hash table, x0 = hash ptr
    emitter.instruction("str x0, [sp, #8]");                                    // save hash table pointer

    // -- loop over array elements --
    emitter.instruction("str xzr, [sp, #16]");                                  // i = 0

    emitter.label("__rt_array_flip_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload source array pointer
    emitter.instruction("ldr x3, [x0]");                                        // x3 = array length
    emitter.instruction("ldr x4, [sp, #16]");                                   // x4 = i
    emitter.instruction("cmp x4, x3");                                          // compare i with length
    emitter.instruction("b.ge __rt_array_flip_done");                           // if i >= length, done

    // -- load array[i] and convert to string key --
    emitter.instruction("add x5, x0, #24");                                     // x5 = data base
    emitter.instruction("ldr x0, [x5, x4, lsl #3]");                            // x0 = array[i] (the integer value)
    emitter.instruction("bl __rt_itoa");                                        // convert int to string, x1=ptr, x2=len

    // -- call hash_set: key=str(value), value=index --
    emitter.instruction("ldr x0, [sp, #8]");                                    // x0 = hash table pointer
    // x1 = key_ptr (from itoa)
    // x2 = key_len (from itoa)
    emitter.instruction("ldr x3, [sp, #16]");                                   // x3 = value_lo = index i
    emitter.instruction("mov x4, #0");                                          // x4 = value_hi = 0
    emitter.instruction("mov x5, #0");                                          // x5 = value_tag = integer
    emitter.instruction("bl __rt_hash_set");                                    // insert key-value pair
    emitter.instruction("str x0, [sp, #8]");                                    // update hash table pointer after possible growth

    // -- advance loop --
    emitter.instruction("ldr x4, [sp, #16]");                                   // reload i
    emitter.instruction("add x4, x4, #1");                                      // i += 1
    emitter.instruction("str x4, [sp, #16]");                                   // save updated i
    emitter.instruction("b __rt_array_flip_loop");                              // continue loop

    // -- return hash table --
    emitter.label("__rt_array_flip_done");
    emitter.instruction("ldr x0, [sp, #8]");                                    // x0 = hash table pointer
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = hash table
}

fn emit_array_flip_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_flip ---");
    emitter.label_global("__rt_array_flip");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving array-flip spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the source array, result hash, and loop index bookkeeping
    emitter.instruction("sub rsp, 32");                                         // reserve aligned spill slots for the scalar array-flip bookkeeping while keeping nested calls 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the source indexed-array pointer across nested hash-constructor and insertion helper calls
    emitter.instruction("mov rdi, QWORD PTR [rdi]");                            // load the source indexed-array logical length before deriving the result hash capacity
    emitter.instruction("shl rdi, 1");                                          // double the source indexed-array length to give the destination hash some insertion headroom
    emitter.instruction("cmp rdi, 16");                                         // clamp the destination hash capacity to the minimum bucket count expected by the runtime
    emitter.instruction("jge __rt_array_flip_capacity_x86");                    // keep the doubled source length when it already meets the minimum bucket count
    emitter.instruction("mov rdi, 16");                                         // fall back to the minimum destination hash capacity for very small indexed arrays
    emitter.label("__rt_array_flip_capacity_x86");
    emitter.instruction("mov rsi, 0");                                          // request integer payload tags in the destination hash because array_flip stores source indices as values
    emitter.instruction("call __rt_hash_new");                                  // allocate the destination hash table through the shared x86_64 hash constructor
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the destination hash pointer across repeated integer-to-string and insertion helper calls
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // initialize the array-flip loop index to the first payload slot of the source indexed array
    emitter.label("__rt_array_flip_loop_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload the array-flip loop index before reading the next source payload
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the source indexed-array pointer before reading its logical length and selected payload
    emitter.instruction("cmp rcx, QWORD PTR [r10]");                            // compare the loop index against the source indexed-array logical length
    emitter.instruction("jge __rt_array_flip_done_x86");                        // finish once every source payload has been converted into a destination hash key
    emitter.instruction("lea r11, [r10 + 24]");                                 // compute the payload base address for the source indexed array
    emitter.instruction("mov rax, QWORD PTR [r11 + rcx * 8]");                  // load the current integer payload into the x86_64 integer-to-string helper input register
    emitter.instruction("call __rt_itoa");                                      // convert the current integer payload into a string key for the destination hash table
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the destination hash pointer before inserting the flipped key/value pair
    emitter.instruction("mov rsi, rax");                                        // place the converted key pointer in the second x86_64 hash insertion argument register
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the current source index after __rt_itoa clobbered caller-saved registers
    emitter.instruction("mov rcx, r10");                                        // move the current source index into the low-word hash insertion register because array_flip stores indices as values
    emitter.instruction("xor r8d, r8d");                                        // clear the high-word hash insertion register because array_flip stores integer indices as values
    emitter.instruction("mov r9, 0");                                           // request integer payload tags in the destination hash because array_flip stores source indices as values
    emitter.instruction("call __rt_hash_set");                                  // insert the flipped key/value pair into the destination hash table
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // persist the possibly-grown destination hash pointer after hash insertion
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the array-flip loop index after helper calls clobbered caller-saved registers
    emitter.instruction("add r10, 1");                                          // advance the loop index after flipping one source payload
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // persist the updated array-flip loop index across the next iteration
    emitter.instruction("jmp __rt_array_flip_loop_x86");                        // continue flipping source payloads into destination hash keys
    emitter.label("__rt_array_flip_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return the destination hash pointer in the standard x86_64 integer result register
    emitter.instruction("add rsp, 32");                                         // release the array-flip spill slots before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                 // return the destination hash pointer in rax
}
