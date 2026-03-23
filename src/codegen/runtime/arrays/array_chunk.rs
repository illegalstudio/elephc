use crate::codegen::emit::Emitter;

/// array_chunk: split an int array into chunks of a given size.
/// Input:  x0=array_ptr, x1=chunk_size
/// Output: x0=outer array (array of array pointers, elem_size=8)
/// Each inner array is an int array containing up to chunk_size elements.
pub fn emit_array_chunk(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_chunk ---");
    emitter.label("__rt_array_chunk");

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
