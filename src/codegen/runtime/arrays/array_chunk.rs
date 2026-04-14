use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_chunk: split an int array into chunks of a given size.
/// Input:  x0=array_ptr, x1=chunk_size
/// Output: x0=outer array (array of array pointers, elem_size=8)
/// Each inner array is an int array containing up to chunk_size elements.
pub fn emit_array_chunk(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_chunk_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_chunk ---");
    emitter.label_global("__rt_array_chunk");

    // -- set up stack frame, save arguments --
    // Stack layout:
    //   [sp, #0]  = source array pointer
    //   [sp, #8]  = chunk size
    //   [sp, #16] = outer array pointer (result)
    //   [sp, #24] = source index i (position in source array)
    //   [sp, #32] = current inner array pointer
    //   [sp, #40] = saved x29
    //   [sp, #48] = saved x30
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save source array pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save chunk size

    // -- calculate number of chunks: ceil(length / chunk_size) --
    emitter.instruction("ldr x2, [x0]");                                        // x2 = source array length
    emitter.instruction("sub x3, x1, #1");                                      // x3 = chunk_size - 1
    emitter.instruction("add x2, x2, x3");                                      // x2 = length + chunk_size - 1
    emitter.instruction("udiv x2, x2, x1");                                     // x2 = ceil(length / chunk_size) = num chunks

    // -- create outer array to hold chunk pointers --
    emitter.instruction("mov x0, x2");                                          // x0 = capacity = num chunks
    emitter.instruction("mov x1, #8");                                          // x1 = elem_size (8 bytes per pointer)
    emitter.instruction("bl __rt_array_new");                                   // create outer array, x0 = outer ptr
    emitter.instruction("str x0, [sp, #16]");                                   // save outer array pointer

    // -- initialize source index --
    emitter.instruction("str xzr, [sp, #24]");                                  // i = 0

    // -- outer loop: create one chunk per iteration --
    emitter.label("__rt_array_chunk_outer");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload source array pointer
    emitter.instruction("ldr x3, [x0]");                                        // x3 = source array length
    emitter.instruction("ldr x4, [sp, #24]");                                   // x4 = i (current position in source)
    emitter.instruction("cmp x4, x3");                                          // check if we've processed all elements
    emitter.instruction("b.ge __rt_array_chunk_done");                          // if i >= length, done

    // -- create inner array for this chunk --
    emitter.instruction("ldr x0, [sp, #8]");                                    // x0 = chunk_size (capacity for inner array)
    emitter.instruction("mov x1, #8");                                          // x1 = elem_size (8 bytes per int)
    emitter.instruction("bl __rt_array_new");                                   // create inner array, x0 = inner ptr
    emitter.instruction("str x0, [sp, #32]");                                   // save inner array pointer

    // -- inner loop: copy up to chunk_size elements to inner array --
    emitter.instruction("mov x5, #0");                                          // x5 = j = 0 (count within this chunk)

    emitter.label("__rt_array_chunk_inner");
    emitter.instruction("ldr x6, [sp, #8]");                                    // x6 = chunk_size
    emitter.instruction("cmp x5, x6");                                          // check if j >= chunk_size
    emitter.instruction("b.ge __rt_array_chunk_push");                          // if chunk is full, push it

    emitter.instruction("ldr x0, [sp, #0]");                                    // reload source array pointer
    emitter.instruction("ldr x3, [x0]");                                        // x3 = source array length
    emitter.instruction("ldr x4, [sp, #24]");                                   // x4 = i
    emitter.instruction("cmp x4, x3");                                          // check if i >= source length
    emitter.instruction("b.ge __rt_array_chunk_push");                          // if source exhausted, push partial chunk

    // -- copy source[i] to inner array --
    emitter.instruction("add x7, x0, #24");                                     // x7 = source data base
    emitter.instruction("ldr x1, [x7, x4, lsl #3]");                            // x1 = source[i]
    emitter.instruction("ldr x0, [sp, #32]");                                   // x0 = inner array pointer
    emitter.instruction("bl __rt_array_push_int");                              // push element to inner array

    // -- advance indices --
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload i
    emitter.instruction("add x4, x4, #1");                                      // i += 1
    emitter.instruction("str x4, [sp, #24]");                                   // save updated i
    emitter.instruction("add x5, x5, #1");                                      // j += 1
    emitter.instruction("b __rt_array_chunk_inner");                            // continue filling this chunk

    // -- push inner array pointer to outer array --
    emitter.label("__rt_array_chunk_push");
    emitter.instruction("ldr x0, [sp, #16]");                                   // x0 = outer array pointer
    emitter.instruction("ldr x1, [sp, #32]");                                   // x1 = inner array pointer (value to push)
    emitter.instruction("bl __rt_array_push_int");                              // push inner array ptr to outer array
    emitter.instruction("b __rt_array_chunk_outer");                            // create next chunk

    // -- return outer array --
    emitter.label("__rt_array_chunk_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // x0 = outer array pointer
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = outer array
}

fn emit_array_chunk_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_chunk ---");
    emitter.label_global("__rt_array_chunk");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving array-chunk spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the source array, chunk size, outer array, source index, and current inner array
    emitter.instruction("sub rsp, 40");                                         // reserve aligned spill slots for the scalar array-chunk bookkeeping while keeping nested calls 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the source indexed-array pointer across nested constructor and append helper calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the requested chunk size across nested constructor and append helper calls
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // load the source indexed-array logical length before computing the number of chunks
    emitter.instruction("mov rcx, rsi");                                        // copy the requested chunk size before biasing the numerator for ceiling division
    emitter.instruction("sub rcx, 1");                                          // compute chunk_size - 1 for the ceiling-division numerator bias
    emitter.instruction("add rax, rcx");                                        // bias the source indexed-array logical length for ceiling division
    emitter.instruction("xor edx, edx");                                        // clear the high dividend half before dividing the biased length by the chunk size
    emitter.instruction("div rsi");                                             // compute ceil(length / chunk_size) in the standard x86_64 integer quotient register
    emitter.instruction("mov rdi, rax");                                        // pass the number of chunks as the outer indexed-array capacity to the shared constructor
    emitter.instruction("mov rsi, 8");                                          // use 8-byte payload slots because the outer array stores inner indexed-array pointers
    emitter.instruction("call __rt_array_new");                                 // allocate the outer indexed array through the shared x86_64 constructor
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the outer indexed-array pointer across inner-array construction and append helper calls
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // initialize the source index to the first payload slot of the source indexed array
    emitter.label("__rt_array_chunk_outer_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the source index before checking whether every source payload slot has been consumed
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the source indexed-array pointer before reading the logical length and candidate payloads
    emitter.instruction("cmp rcx, QWORD PTR [r10]");                            // compare the source index against the source indexed-array logical length
    emitter.instruction("jge __rt_array_chunk_done_x86");                       // finish once every source payload slot has been assigned to some chunk
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // pass the requested chunk size as the inner indexed-array capacity to the shared constructor
    emitter.instruction("mov rsi, 8");                                          // use 8-byte payload slots because the current implementation chunks scalar indexed arrays
    emitter.instruction("call __rt_array_new");                                 // allocate the current inner indexed array through the shared x86_64 constructor
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve the current inner indexed-array pointer while filling it from the source array
    emitter.instruction("xor r9, r9");                                          // initialize the inner chunk index to the first payload slot of the current inner indexed array
    emitter.label("__rt_array_chunk_inner_x86");
    emitter.instruction("cmp r9, QWORD PTR [rbp - 16]");                        // compare the inner chunk index against the requested chunk size
    emitter.instruction("jge __rt_array_chunk_push_x86");                       // push the current inner indexed array once the requested chunk size has been reached
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the source indexed-array pointer before checking whether the source payload stream is exhausted
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the source index before checking whether the source payload stream is exhausted
    emitter.instruction("cmp rcx, QWORD PTR [r10]");                            // compare the source index against the source indexed-array logical length
    emitter.instruction("jge __rt_array_chunk_push_x86");                       // push the partially-filled inner indexed array once the source payload stream is exhausted
    emitter.instruction("lea r11, [r10 + 24]");                                 // compute the payload base address for the source indexed array
    emitter.instruction("mov rsi, QWORD PTR [r11 + rcx * 8]");                  // load the current scalar payload from the source indexed array into the append helper value register
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // reload the current inner indexed-array pointer into the append helper receiver register
    emitter.instruction("call __rt_array_push_int");                            // append the current scalar payload into the current inner indexed array
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // persist the possibly-grown current inner indexed-array pointer after the append helper returns
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the source index after helper calls clobbered caller-saved registers
    emitter.instruction("add rcx, 1");                                          // advance the source index after copying one payload into the current inner indexed array
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // persist the updated source index across the next inner-loop iteration
    emitter.instruction("add r9, 1");                                           // advance the inner chunk index after filling one payload slot in the current inner indexed array
    emitter.instruction("jmp __rt_array_chunk_inner_x86");                      // continue filling the current inner indexed array until it is full or the source payload stream ends
    emitter.label("__rt_array_chunk_push_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the outer indexed-array pointer before appending the finished inner indexed array
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // place the finished inner indexed-array pointer in the append helper value register
    emitter.instruction("call __rt_array_push_int");                            // append the finished inner indexed-array pointer into the outer indexed array
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // persist the possibly-grown outer indexed-array pointer after appending the finished inner indexed array
    emitter.instruction("jmp __rt_array_chunk_outer_x86");                      // continue chunking the remaining source payloads into new inner indexed arrays
    emitter.label("__rt_array_chunk_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the outer indexed-array pointer in the standard x86_64 integer result register
    emitter.instruction("add rsp, 40");                                         // release the array-chunk spill slots before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                 // return the outer indexed-array pointer in rax
}
