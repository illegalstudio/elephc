use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_grow: double the capacity of an array after copy-on-write splitting.
/// Ensures the source array is unique, allocates a new array with 2x capacity,
/// copies header + elements, frees the previous unique storage, and returns the
/// new pointer.
/// Input:  x0 = old array pointer
/// Output: x0 = new array pointer (with doubled capacity)
pub fn emit_array_grow(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_grow_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_grow ---");
    emitter.label_global("__rt_array_grow");

    // -- set up stack frame --
    // Stack layout:
    //   [sp, #0]  = old array pointer
    //   [sp, #8]  = old length
    //   [sp, #16] = old elem_size
    //   [sp, #24] = new capacity
    //   [sp, #32] = saved x29
    //   [sp, #40] = saved x30
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up new frame pointer
    emitter.instruction("bl __rt_array_ensure_unique");                         // split shared arrays before the growth path reallocates storage
    emitter.instruction("str x0, [sp, #0]");                                    // save the unique source array pointer

    // -- read old array header --
    emitter.instruction("ldr x9, [x0]");                                        // x9 = old length
    emitter.instruction("str x9, [sp, #8]");                                    // save old length
    emitter.instruction("ldr x10, [x0, #8]");                                   // x10 = old capacity
    emitter.instruction("ldr x11, [x0, #16]");                                  // x11 = elem_size
    emitter.instruction("str x11, [sp, #16]");                                  // save elem_size

    // -- compute new capacity (2x old, minimum 8) --
    emitter.instruction("lsl x12, x10, #1");                                    // x12 = old_capacity * 2
    emitter.instruction("cmp x12, #8");                                         // at least 8 elements
    emitter.instruction("b.ge __rt_array_grow_alloc");                          // skip if already >= 8
    emitter.instruction("mov x12, #8");                                         // minimum capacity = 8
    emitter.label("__rt_array_grow_alloc");
    emitter.instruction("str x12, [sp, #24]");                                  // save new capacity

    // -- allocate new array: 24 + new_capacity * elem_size --
    emitter.instruction("mul x0, x12, x11");                                    // x0 = new_capacity * elem_size
    emitter.instruction("add x0, x0, #24");                                     // x0 = total bytes (header + data)
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate new array → x0
    emitter.instruction("ldr x14, [sp, #0]");                                   // reload the previous array pointer after heap_alloc
    emitter.instruction("ldr x14, [x14, #-8]");                                 // copy the packed kind word from the previous array storage
    emitter.instruction("and x14, x14, #0xffff");                               // preserve only persistent kind/value-type/COW bits on the grown array
    emitter.instruction("str x14, [x0, #-8]");                                  // preserve heap kind, array value_type, and copy-on-write metadata

    // -- write new array header --
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload old length
    emitter.instruction("str x9, [x0]");                                        // new_array.length = old length
    emitter.instruction("ldr x12, [sp, #24]");                                  // reload new capacity
    emitter.instruction("str x12, [x0, #8]");                                   // new_array.capacity = new capacity
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload elem_size
    emitter.instruction("str x11, [x0, #16]");                                  // new_array.elem_size = old elem_size

    // -- copy elements from old array to new array --
    emitter.instruction("ldr x1, [sp, #0]");                                    // x1 = old array pointer
    emitter.instruction("add x1, x1, #24");                                     // x1 = old data start (skip header)
    emitter.instruction("add x2, x0, #24");                                     // x2 = new data start (skip header)
    emitter.instruction("mul x3, x9, x11");                                     // x3 = bytes to copy (length * elem_size)
    emitter.instruction("str x0, [sp, #24]");                                   // save new array ptr (reusing slot)

    // -- byte-copy loop --
    emitter.label("__rt_array_grow_copy");
    emitter.instruction("cbz x3, __rt_array_grow_done");                        // all bytes copied
    emitter.instruction("ldrb w4, [x1], #1");                                   // load byte from old, advance
    emitter.instruction("strb w4, [x2], #1");                                   // store byte to new, advance
    emitter.instruction("sub x3, x3, #1");                                      // decrement remaining
    emitter.instruction("b __rt_array_grow_copy");                              // continue copying

    // -- free the previous unique storage and return the grown array pointer --
    emitter.label("__rt_array_grow_done");
    emitter.instruction("ldr x0, [sp, #0]");                                    // x0 = previous unique array pointer
    emitter.instruction("bl __rt_heap_free");                                   // release the old array storage now that the grown copy is live
    emitter.instruction("ldr x0, [sp, #24]");                                   // x0 = new array pointer

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = new array
}

fn emit_array_grow_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_grow ---");
    emitter.label_global("__rt_array_grow");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving indexed-array growth spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved source pointer, length, element size, and grown array pointer
    emitter.instruction("sub rsp, 40");                                         // reserve aligned spill slots for the unique source array pointer, length, element size, and grown array pointer
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the incoming indexed-array pointer across uniqueness and allocation helper calls
    emitter.instruction("call __rt_array_ensure_unique");                       // split shared indexed arrays before reallocating storage for growth
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the unique indexed-array pointer after copy-on-write splitting
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // load the indexed-array logical length before allocating the grown storage
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // preserve the previous logical length for the payload copy and final header restoration
    emitter.instruction("mov r11, QWORD PTR [rax + 8]");                        // load the indexed-array capacity before computing the grown capacity
    emitter.instruction("mov r12, QWORD PTR [rax + 16]");                       // load the indexed-array element size so the grown storage keeps the same layout
    emitter.instruction("mov QWORD PTR [rbp - 24], r12");                       // preserve the indexed-array element size across the allocator helper call
    emitter.instruction("lea r13, [r11 + r11]");                                // compute the doubled indexed-array capacity as the baseline growth target
    emitter.instruction("cmp r13, 8");                                          // should the doubled indexed-array capacity be raised to the minimum growth floor?
    emitter.instruction("jae __rt_array_grow_alloc");                           // keep the doubled capacity when it already reaches the minimum indexed-array growth floor
    emitter.instruction("mov r13, 8");                                          // enforce a minimum indexed-array capacity of eight elements after growth
    emitter.label("__rt_array_grow_alloc");
    emitter.instruction("mov rdi, r13");                                        // pass the grown indexed-array capacity to the shared array allocator helper
    emitter.instruction("mov rsi, r12");                                        // pass the preserved element size so the grown indexed array keeps its slot width
    emitter.instruction("call __rt_array_new");                                 // allocate the grown indexed-array backing storage through the shared allocator helper
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve the grown indexed-array pointer across the payload copy and old-storage free helper call
    emitter.instruction("mov r14, QWORD PTR [rbp - 8]");                        // reload the previous unique indexed-array pointer after the allocator returns
    emitter.instruction("mov r15, QWORD PTR [r14 - 8]");                        // load the packed heap-kind metadata from the previous indexed-array header
    emitter.instruction("mov r11, QWORD PTR [rax - 8]");                        // snapshot the freshly allocated grown header so the x86_64 heap marker survives the metadata rewrite
    emitter.instruction("and r11, -65536");                                     // keep the high x86_64 heap-marker bits while clearing the low container-kind payload lane
    emitter.instruction("and r15, 0xffff");                                     // preserve only the stable indexed-array kind, value_type, and copy-on-write metadata bits
    emitter.instruction("or r11, r15");                                         // combine the fresh grown header marker bits with the stable source container-kind payload bits
    emitter.instruction("mov QWORD PTR [rax - 8], r11");                        // stamp the grown indexed-array with the preserved container metadata without losing the x86_64 heap marker
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the previous logical length after helper calls clobbered caller-saved registers
    emitter.instruction("mov QWORD PTR [rax], r10");                            // restore the previous logical length on the grown indexed-array header
    emitter.instruction("mov QWORD PTR [rax + 8], r13");                        // store the grown indexed-array capacity in the new header
    emitter.instruction("mov r12, QWORD PTR [rbp - 24]");                       // reload the preserved element size after helper calls clobbered caller-saved registers
    emitter.instruction("mov QWORD PTR [rax + 16], r12");                       // store the preserved element size in the grown indexed-array header
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // reload the previous unique indexed-array pointer before copying its live payload region
    emitter.instruction("lea rsi, [rsi + 24]");                                 // advance to the payload base of the previous indexed-array storage
    emitter.instruction("lea rdi, [rax + 24]");                                 // advance to the payload base of the grown indexed-array storage
    emitter.instruction("mov rcx, r10");                                        // seed the payload byte-count computation from the previous logical length
    emitter.instruction("imul rcx, r12");                                       // compute the number of live payload bytes that must be copied into the grown indexed array
    emitter.label("__rt_array_grow_copy");
    emitter.instruction("test rcx, rcx");                                       // have we copied every live payload byte into the grown indexed-array storage?
    emitter.instruction("je __rt_array_grow_done");                             // stop the payload copy loop once the live payload region is exhausted
    emitter.instruction("mov r8b, BYTE PTR [rsi]");                             // load the next live payload byte from the previous indexed-array storage
    emitter.instruction("mov BYTE PTR [rdi], r8b");                             // store the copied payload byte into the grown indexed-array storage
    emitter.instruction("add rsi, 1");                                          // advance the source payload cursor after copying one byte
    emitter.instruction("add rdi, 1");                                          // advance the destination payload cursor after copying one byte
    emitter.instruction("sub rcx, 1");                                          // decrement the number of live payload bytes that still need to be copied
    emitter.instruction("jmp __rt_array_grow_copy");                            // continue copying the live payload region into the grown indexed-array storage
    emitter.label("__rt_array_grow_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the previous unique indexed-array pointer before releasing its backing storage
    emitter.instruction("call __rt_heap_free");                                 // release the previous unique indexed-array storage now that the grown copy is live
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // return the grown indexed-array pointer after releasing the previous storage
    emitter.instruction("add rsp, 40");                                         // release the indexed-array growth spill slots before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the grown indexed array
    emitter.instruction("ret");                                                 // return to the caller with rax holding the grown indexed-array pointer
}
