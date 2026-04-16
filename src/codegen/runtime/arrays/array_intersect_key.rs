use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_intersect_key: return entries from hash1 whose keys ARE in hash2.
/// Input:  x0=hash1, x1=hash2
/// Output: x0=new hash table with entries from hash1 found in hash2
pub fn emit_array_intersect_key(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_intersect_key_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_intersect_key ---");
    emitter.label_global("__rt_array_intersect_key");

    // -- set up stack frame, save arguments --
    // Stack layout:
    //   [sp, #0]  = hash1 pointer
    //   [sp, #8]  = hash2 pointer
    //   [sp, #16] = result hash table pointer
    //   [sp, #24] = iterator cursor
    //   [sp, #32] = saved x29
    //   [sp, #40] = saved x30
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save hash1 pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save hash2 pointer

    // -- create result hash table with same capacity as hash1 --
    emitter.instruction("ldr x0, [x0, #8]");                                    // x0 = hash1 capacity
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload hash1 pointer
    emitter.instruction("ldr x1, [x9, #16]");                                   // x1 = hash1 value_type
    emitter.instruction("bl __rt_hash_new");                                    // create result hash table, x0 = result ptr
    emitter.instruction("str x0, [sp, #16]");                                   // save result hash table pointer

    // -- iterate over hash1 entries --
    emitter.instruction("str xzr, [sp, #24]");                                  // iterator cursor = 0 (start from hash header head)

    emitter.label("__rt_array_isect_key_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // x0 = hash1 pointer
    emitter.instruction("ldr x1, [sp, #24]");                                   // x1 = current iterator cursor
    emitter.instruction("bl __rt_hash_iter_next");                              // get next entry, x0=next_cursor, x1=key_ptr, x2=key_len, x3=val_lo, x4=val_hi, x5=val_tag

    // -- check if iteration is done --
    emitter.instruction("cmn x0, #1");                                          // check if x0 == -1 (end of iteration)
    emitter.instruction("b.eq __rt_array_isect_key_done");                      // if done, return result

    // -- save iterator state and entry data --
    emitter.instruction("str x0, [sp, #24]");                                   // save next iterator cursor

    // -- save entry values on temp stack space --
    emitter.instruction("sub sp, sp, #48");                                     // allocate temp space for entry data and value_tag
    emitter.instruction("str x1, [sp, #0]");                                    // save key_ptr
    emitter.instruction("str x2, [sp, #8]");                                    // save key_len
    emitter.instruction("str x3, [sp, #16]");                                   // save value_lo
    emitter.instruction("str x4, [sp, #24]");                                   // save value_hi
    emitter.instruction("str x5, [sp, #32]");                                   // save value_tag

    // -- check if this key exists in hash2 --
    emitter.instruction("ldr x0, [sp, #56]");                                   // load hash2 pointer (sp+8 shifted by 48)
                                              // x1=key_ptr, x2=key_len already set
    emitter.instruction("bl __rt_hash_get");                                    // check if key exists in hash2, x0=found

    // -- if key IS found in hash2, add to result --
    emitter.instruction("cbz x0, __rt_array_isect_key_skip");                   // if NOT found in hash2, skip this entry

    // -- copied hash values stay borrowed until we retain them for the result --
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload this entry's runtime value_tag
    emitter.instruction("cmp x9, #1");                                          // is the borrowed value a string?
    emitter.instruction("b.eq __rt_array_isect_key_retain");                    // strings need retain via the uniform dispatcher
    emitter.instruction("cmp x9, #4");                                          // is the borrowed value heap-backed?
    emitter.instruction("b.lt __rt_array_isect_key_copy");                      // scalar values need no retain
    emitter.instruction("cmp x9, #7");                                          // do heap-backed tags stay within range?
    emitter.instruction("b.gt __rt_array_isect_key_copy");                      // unknown tags are ignored here
    emitter.label("__rt_array_isect_key_retain");
    emitter.instruction("ldr x0, [sp, #16]");                                   // load borrowed heap pointer from saved value_lo
    emitter.instruction("bl __rt_incref");                                      // retain copied heap value for the result hash

    // -- key found in hash2: add to result hash table --
    emitter.label("__rt_array_isect_key_copy");
    emitter.instruction("ldr x0, [sp, #64]");                                   // load result hash table (sp+16 shifted by 48)
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload key_ptr
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload key_len
    emitter.instruction("ldr x3, [sp, #16]");                                   // reload value_lo
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload value_hi
    emitter.instruction("ldr x5, [sp, #32]");                                   // reload value_tag
    emitter.instruction("bl __rt_hash_set");                                    // insert into result hash table
    emitter.instruction("str x0, [sp, #64]");                                   // update result hash table pointer after possible growth

    emitter.label("__rt_array_isect_key_skip");
    emitter.instruction("add sp, sp, #48");                                     // deallocate temp space
    emitter.instruction("b __rt_array_isect_key_loop");                         // continue iterating

    // -- return result hash table --
    emitter.label("__rt_array_isect_key_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // x0 = result hash table pointer
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = result hash table
}

fn emit_array_intersect_key_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_intersect_key ---");
    emitter.label_global("__rt_array_intersect_key");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving intersect-key state slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved hash pointers and iterator cursor
    emitter.instruction("sub rsp, 80");                                         // reserve local storage for hash1, hash2, result, iterator cursor, and a non-overlapping temporary entry scratch area
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the source associative-array pointer across the intersect-key helper calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the mask associative-array pointer across the intersect-key helper calls
    emitter.instruction("mov rdi, QWORD PTR [rdi + 8]");                        // load the source associative-array capacity to seed the filtered result hash table
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the source associative-array pointer for the packed value-type load
    emitter.instruction("mov rsi, QWORD PTR [r10 + 16]");                       // load the source associative-array value_type for the filtered result hash table
    emitter.instruction("call __rt_hash_new");                                  // allocate the filtered result associative-array with matching capacity and value_type
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the filtered result associative-array pointer across iteration
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // initialize the insertion-order iterator cursor to the hash-header head sentinel

    emitter.label("__rt_array_isect_key_loop");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the source associative-array pointer for the next insertion-order iteration step
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // reload the current insertion-order iterator cursor
    emitter.instruction("call __rt_hash_iter_next");                            // advance one associative-array insertion-order entry and return its key plus payload
    emitter.instruction("cmp rax, -1");                                         // has associative-array iteration reached the done sentinel?
    emitter.instruction("je __rt_array_isect_key_done");                        // finish once every source associative-array entry has been visited
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the updated insertion-order iterator cursor for the next loop step
    emitter.instruction("mov QWORD PTR [rbp - 40], rdi");                       // save the current associative-array entry key pointer for mask probing and possible copy
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                       // save the current associative-array entry key length for mask probing and possible copy
    emitter.instruction("mov QWORD PTR [rbp - 56], rcx");                       // save the current associative-array entry low payload word for possible copy into the result
    emitter.instruction("mov QWORD PTR [rbp - 64], r8");                        // save the current associative-array entry high payload word in the temporary scratch area
    emitter.instruction("mov QWORD PTR [rbp - 72], r9");                        // save the current associative-array entry runtime tag in the temporary scratch area
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // load the mask associative-array pointer for key existence probing
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // pass the current associative-array entry key pointer to the hash-get helper
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // pass the current associative-array entry key length to the hash-get helper
    emitter.instruction("call __rt_hash_get");                                  // probe whether the current source key exists in the mask associative-array
    emitter.instruction("test rax, rax");                                       // did the mask associative-array contain the current source key?
    emitter.instruction("jz __rt_array_isect_key_loop");                        // skip copying keys that are absent from the mask associative-array
    emitter.instruction("mov r10, QWORD PTR [rbp - 72]");                       // reload the saved runtime value tag for the source associative-array entry
    emitter.instruction("cmp r10, 1");                                          // is the copied associative-array value a string?
    emitter.instruction("je __rt_array_isect_key_retain");                      // strings need a retain because the filtered result becomes a new owner
    emitter.instruction("cmp r10, 4");                                          // is the copied associative-array value heap-backed?
    emitter.instruction("jl __rt_array_isect_key_copy");                        // scalar payloads can be copied directly into the filtered result
    emitter.instruction("cmp r10, 7");                                          // do heap-backed associative-array value tags stay within the supported retainable range?
    emitter.instruction("jg __rt_array_isect_key_copy");                        // unsupported tags fall back to raw copy without an extra retain
    emitter.label("__rt_array_isect_key_retain");
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // move the copied associative-array heap pointer into the incref helper input register
    emitter.instruction("call __rt_incref");                                    // retain the copied associative-array heap payload for the filtered result hash table

    emitter.label("__rt_array_isect_key_copy");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // load the filtered result associative-array pointer for insertion
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // reload the copied associative-array key pointer for filtered insertion
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // reload the copied associative-array key length for filtered insertion
    emitter.instruction("mov rcx, QWORD PTR [rbp - 56]");                       // reload the copied associative-array low payload word for filtered insertion
    emitter.instruction("mov r8, QWORD PTR [rbp - 64]");                        // reload the copied associative-array high payload word for filtered insertion
    emitter.instruction("mov r9, QWORD PTR [rbp - 72]");                        // reload the copied associative-array runtime tag for filtered insertion
    emitter.instruction("call __rt_hash_set");                                  // insert the copied associative-array entry into the filtered result hash table
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // save the possibly-grown filtered result associative-array pointer
    emitter.instruction("jmp __rt_array_isect_key_loop");                       // continue scanning the remaining source associative-array entries

    emitter.label("__rt_array_isect_key_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the filtered associative-array pointer in the standard integer result register
    emitter.instruction("add rsp, 80");                                         // release the intersect-key helper local storage before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning from the intersect-key helper
    emitter.instruction("ret");                                                 // return the filtered associative-array pointer to generated code
}
