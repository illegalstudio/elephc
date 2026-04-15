use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_chunk_refcounted: split an array of refcounted 8-byte payloads into chunks.
/// Input:  x0=array_ptr, x1=chunk_size
/// Output: x0=outer array (array of inner array pointers)
pub fn emit_array_chunk_refcounted(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_chunk_refcounted_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_chunk_refcounted ---");
    emitter.label_global("__rt_array_chunk_refcounted");

    // -- set up stack frame, save arguments --
    emitter.instruction("sub sp, sp, #80");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save source array pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save chunk size

    // -- calculate number of chunks: ceil(length / chunk_size) --
    emitter.instruction("ldr x2, [x0]");                                        // load source array length
    emitter.instruction("sub x3, x1, #1");                                      // compute chunk_size - 1
    emitter.instruction("add x2, x2, x3");                                      // bias numerator for ceiling division
    emitter.instruction("udiv x2, x2, x1");                                     // compute number of chunks

    // -- create outer array to hold chunk pointers --
    emitter.instruction("mov x0, x2");                                          // move outer array capacity into x0
    emitter.instruction("mov x1, #8");                                          // use 8-byte slots for inner array pointers
    emitter.instruction("bl __rt_array_new");                                   // allocate outer array
    emitter.instruction("str x0, [sp, #16]");                                   // save outer array pointer

    // -- initialize source and inner-loop indices --
    emitter.instruction("str xzr, [sp, #24]");                                  // initialize source index i = 0

    emitter.label("__rt_array_chunk_ref_outer");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload source array pointer
    emitter.instruction("ldr x3, [x0]");                                        // load source array length
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload source index i
    emitter.instruction("cmp x4, x3");                                          // compare source index with source length
    emitter.instruction("b.ge __rt_array_chunk_ref_done");                      // stop when every source element has been consumed

    // -- create inner array for this chunk --
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload chunk size as inner capacity
    emitter.instruction("mov x1, #8");                                          // use 8-byte slots for refcounted payload pointers
    emitter.instruction("bl __rt_array_new");                                   // allocate inner array
    emitter.instruction("str x0, [sp, #32]");                                   // save inner array pointer
    emitter.instruction("str xzr, [sp, #40]");                                  // initialize inner index j = 0

    emitter.label("__rt_array_chunk_ref_inner");
    emitter.instruction("ldr x5, [sp, #40]");                                   // reload inner index j
    emitter.instruction("ldr x6, [sp, #8]");                                    // reload chunk size
    emitter.instruction("cmp x5, x6");                                          // compare inner index with chunk size
    emitter.instruction("b.ge __rt_array_chunk_ref_push");                      // push current chunk once it is full
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload source array pointer
    emitter.instruction("ldr x3, [x0]");                                        // reload source array length
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload source index i
    emitter.instruction("cmp x4, x3");                                          // compare source index with source length
    emitter.instruction("b.ge __rt_array_chunk_ref_push");                      // push partial chunk once source is exhausted
    emitter.instruction("add x7, x0, #24");                                     // compute source data base
    emitter.instruction("ldr x1, [x7, x4, lsl #3]");                            // load source element pointer
    emitter.instruction("str x1, [sp, #48]");                                   // preserve source element pointer across incref
    emitter.instruction("mov x0, x1");                                          // move element pointer into incref argument register
    emitter.instruction("bl __rt_incref");                                      // retain source-owned payload before copying into inner array
    emitter.instruction("ldr x1, [sp, #48]");                                   // restore retained element pointer
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload inner array pointer
    emitter.instruction("bl __rt_array_push_int");                              // append retained payload pointer to inner array
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload source index after helper calls
    emitter.instruction("add x4, x4, #1");                                      // increment source index
    emitter.instruction("str x4, [sp, #24]");                                   // persist updated source index
    emitter.instruction("ldr x5, [sp, #40]");                                   // reload inner index after helper calls
    emitter.instruction("add x5, x5, #1");                                      // increment inner index
    emitter.instruction("str x5, [sp, #40]");                                   // persist updated inner index
    emitter.instruction("b __rt_array_chunk_ref_inner");                        // continue filling the current inner chunk

    emitter.label("__rt_array_chunk_ref_push");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload outer array pointer
    emitter.instruction("ldr x1, [sp, #32]");                                   // reload inner array pointer
    emitter.instruction("bl __rt_array_push_int");                              // transfer inner array ownership into outer array
    emitter.instruction("b __rt_array_chunk_ref_outer");                        // continue with the next chunk

    emitter.label("__rt_array_chunk_ref_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload outer array pointer as return value
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return outer array
}

fn emit_array_chunk_refcounted_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_chunk_refcounted ---");
    emitter.label_global("__rt_array_chunk_refcounted");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving refcounted array-chunk spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the source array, chunk size, outer array, source index, and current inner array
    emitter.instruction("sub rsp, 40");                                         // reserve aligned spill slots for the refcounted array-chunk bookkeeping while keeping nested calls 16-byte aligned
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

    emitter.label("__rt_array_chunk_ref_outer_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the source index before checking whether every source payload slot has been consumed
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the source indexed-array pointer before reading the logical length and candidate payloads
    emitter.instruction("cmp rcx, QWORD PTR [r10]");                            // compare the source index against the source indexed-array logical length
    emitter.instruction("jge __rt_array_chunk_ref_done_x86");                   // finish once every source payload slot has been assigned to some chunk
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // pass the requested chunk size as the inner indexed-array capacity to the shared constructor
    emitter.instruction("mov rsi, 8");                                          // use 8-byte payload slots because the current implementation chunks refcounted indexed arrays
    emitter.instruction("call __rt_array_new");                                 // allocate the current inner indexed array through the shared x86_64 constructor
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve the current inner indexed-array pointer while filling it from the source array
    emitter.instruction("xor r9, r9");                                          // initialize the inner chunk index to the first payload slot of the current inner indexed array

    emitter.label("__rt_array_chunk_ref_inner_x86");
    emitter.instruction("cmp r9, QWORD PTR [rbp - 16]");                        // compare the inner chunk index against the requested chunk size
    emitter.instruction("jge __rt_array_chunk_ref_push_x86");                   // push the current inner indexed array once the requested chunk size has been reached
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the source indexed-array pointer before checking whether the source payload stream is exhausted
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the source index before checking whether the source payload stream is exhausted
    emitter.instruction("cmp rcx, QWORD PTR [r10]");                            // compare the source index against the source indexed-array logical length
    emitter.instruction("jge __rt_array_chunk_ref_push_x86");                   // push the partially-filled inner indexed array once the source payload stream is exhausted
    emitter.instruction("lea r11, [r10 + 24]");                                 // compute the payload base address for the source indexed array
    emitter.instruction("mov rsi, QWORD PTR [r11 + rcx * 8]");                  // load the current borrowed refcounted payload from the source indexed array into the append helper value register
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // reload the current inner indexed-array pointer into the append helper receiver register
    emitter.instruction("call __rt_array_push_refcounted");                     // append the retained current refcounted payload into the current inner indexed array
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // persist the possibly-grown current inner indexed-array pointer after the append helper returns
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the source index after helper calls clobbered caller-saved registers
    emitter.instruction("add rcx, 1");                                          // advance the source index after copying one payload into the current inner indexed array
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // persist the updated source index across the next inner-loop iteration
    emitter.instruction("add r9, 1");                                           // advance the inner chunk index after filling one payload slot in the current inner indexed array
    emitter.instruction("jmp __rt_array_chunk_ref_inner_x86");                  // continue filling the current inner indexed array until it is full or the source payload stream ends

    emitter.label("__rt_array_chunk_ref_push_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the outer indexed-array pointer before appending the finished inner indexed array
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // place the finished inner indexed-array pointer in the append helper value register
    emitter.instruction("call __rt_array_push_int");                            // append the finished inner indexed-array pointer into the outer indexed array without retaining the freshly-created chunk again
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // persist the possibly-grown outer indexed-array pointer after appending the finished inner indexed array
    emitter.instruction("jmp __rt_array_chunk_ref_outer_x86");                  // continue chunking the remaining source payloads into new inner indexed arrays

    emitter.label("__rt_array_chunk_ref_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the outer indexed-array pointer in the standard x86_64 integer result register
    emitter.instruction("add rsp, 40");                                         // release the refcounted array-chunk spill slots before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                 // return the outer indexed-array pointer in rax
}
