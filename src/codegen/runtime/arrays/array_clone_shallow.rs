use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_clone_shallow: duplicate an indexed array for copy-on-write semantics.
/// Scalar payloads are byte-copied, string payloads are re-persisted, and
/// refcounted child pointers are retained for the cloned owner.
/// Input:  x0 = source array pointer
/// Output: x0 = cloned array pointer
pub fn emit_array_clone_shallow(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_clone_shallow_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_clone_shallow ---");
    emitter.label_global("__rt_array_clone_shallow");

    // -- set up stack frame and preserve callee-saved registers --
    emitter.instruction("sub sp, sp, #96");                                     // allocate 96 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // set up new frame pointer
    emitter.instruction("stp x19, x20, [sp, #64]");                             // save callee-saved x19/x20
    emitter.instruction("stp x21, x22, [sp, #48]");                             // save callee-saved x21/x22
    emitter.instruction("stp x23, x24, [sp, #32]");                             // save callee-saved x23/x24
    emitter.instruction("str x0, [sp, #0]");                                    // save the source array pointer

    // -- snapshot source metadata needed for allocation and post-copy fixups --
    emitter.instruction("ldr x19, [x0]");                                       // x19 = source length
    emitter.instruction("ldr x20, [x0, #8]");                                   // x20 = source capacity
    emitter.instruction("ldr x21, [x0, #16]");                                  // x21 = source elem_size
    emitter.instruction("ldr x22, [x0, #-8]");                                  // x22 = packed kind word from the source header

    // -- allocate a destination array with the same capacity/layout --
    emitter.instruction("mov x0, x20");                                         // x0 = cloned array capacity
    emitter.instruction("mov x1, x21");                                         // x1 = cloned array elem_size
    emitter.instruction("bl __rt_array_new");                                   // allocate a fresh destination array
    emitter.instruction("str x0, [sp, #8]");                                    // save the cloned array pointer
    emitter.instruction("mov x20, x0");                                         // keep the cloned array pointer in a callee-saved register
    emitter.instruction("and x22, x22, #0xffff");                               // preserve only persistent kind/value-type/COW bits
    emitter.instruction("str x22, [x20, #-8]");                                 // copy the persistent packed metadata into the clone
    emitter.instruction("str x19, [x20]");                                      // restore the logical length on the cloned array

    // -- byte-copy the payload region so scalars/floats arrive intact --
    emitter.instruction("ldr x1, [sp, #0]");                                    // x1 = source array pointer
    emitter.instruction("add x1, x1, #24");                                     // x1 = source payload base
    emitter.instruction("add x2, x20, #24");                                    // x2 = clone payload base
    emitter.instruction("mul x3, x19, x21");                                    // x3 = payload bytes to copy
    emitter.label("__rt_array_clone_shallow_copy");
    emitter.instruction("cbz x3, __rt_array_clone_shallow_fixup");              // skip the copy loop when the array is empty
    emitter.instruction("ldrb w4, [x1], #1");                                   // load one byte from the source payload
    emitter.instruction("strb w4, [x2], #1");                                   // store one byte into the cloned payload
    emitter.instruction("sub x3, x3, #1");                                      // decrement the remaining byte count
    emitter.instruction("b __rt_array_clone_shallow_copy");                     // continue copying until the payload is exhausted

    // -- repair cloned ownership according to the runtime value_type tag --
    emitter.label("__rt_array_clone_shallow_fixup");
    emitter.instruction("lsr x9, x22, #8");                                     // move the packed array value_type tag into the low bits
    emitter.instruction("and x9, x9, #0x7f");                                   // isolate the value_type without the persistent COW flag
    emitter.instruction("cmp x9, #1");                                          // is this a string array?
    emitter.instruction("b.eq __rt_array_clone_shallow_strings");               // string slots need fresh persisted payloads
    emitter.instruction("cmp x9, #4");                                          // is this an array of indexed arrays?
    emitter.instruction("b.eq __rt_array_clone_shallow_refs");                  // nested refcounted payloads need retains
    emitter.instruction("cmp x9, #5");                                          // is this an array of associative arrays?
    emitter.instruction("b.eq __rt_array_clone_shallow_refs");                  // nested refcounted payloads need retains
    emitter.instruction("cmp x9, #6");                                          // is this an array of objects?
    emitter.instruction("b.eq __rt_array_clone_shallow_refs");                  // nested refcounted payloads need retains
    emitter.instruction("cmp x9, #7");                                          // is this an array of boxed mixed values?
    emitter.instruction("b.eq __rt_array_clone_shallow_refs");                  // boxed mixed payloads also need retains
    emitter.instruction("b __rt_array_clone_shallow_done");                     // scalar payloads are already correct after the byte copy

    // -- string arrays must own their own persisted payloads after the split --
    emitter.label("__rt_array_clone_shallow_strings");
    emitter.instruction("mov x23, #0");                                         // x23 = slot index for string re-persistence
    emitter.label("__rt_array_clone_shallow_strings_loop");
    emitter.instruction("cmp x23, x19");                                        // have we handled every live string slot?
    emitter.instruction("b.ge __rt_array_clone_shallow_done");                  // yes — string fixups are complete
    emitter.instruction("lsl x10, x23, #4");                                    // x10 = slot byte offset for 16-byte string entries
    emitter.instruction("add x10, x20, x10");                                   // advance from clone base to the current slot
    emitter.instruction("add x10, x10, #24");                                   // skip the array header to string storage
    emitter.instruction("ldr x1, [x10]");                                       // x1 = cloned string pointer from the copied slot
    emitter.instruction("ldr x2, [x10, #8]");                                   // x2 = cloned string length from the copied slot
    emitter.instruction("bl __rt_str_persist");                                 // duplicate the immutable string payload for the cloned owner
    emitter.instruction("lsl x10, x23, #4");                                    // recompute the slot byte offset after the helper call
    emitter.instruction("add x10, x20, x10");                                   // advance from clone base to the current slot again
    emitter.instruction("add x10, x10, #24");                                   // skip the array header to string storage again
    emitter.instruction("str x1, [x10]");                                       // install the newly persisted string pointer into the cloned slot
    emitter.instruction("str x2, [x10, #8]");                                   // install the newly persisted string length into the cloned slot
    emitter.instruction("add x23, x23, #1");                                    // advance to the next live string slot
    emitter.instruction("b __rt_array_clone_shallow_strings_loop");             // continue duplicating cloned string payloads

    // -- refcounted arrays share child pointers, so the clone must retain them --
    emitter.label("__rt_array_clone_shallow_refs");
    emitter.instruction("mov x23, #0");                                         // x23 = slot index for child retains
    emitter.instruction("add x24, x20, #24");                                   // x24 = cloned payload base for 8-byte child pointers
    emitter.label("__rt_array_clone_shallow_refs_loop");
    emitter.instruction("cmp x23, x19");                                        // have we visited every live child pointer slot?
    emitter.instruction("b.ge __rt_array_clone_shallow_done");                  // yes — refcounted fixups are complete
    emitter.instruction("ldr x0, [x24, x23, lsl #3]");                          // load the cloned child pointer from the copied payload
    emitter.instruction("bl __rt_incref");                                      // retain the shared child pointer for the cloned array owner
    emitter.instruction("add x23, x23, #1");                                    // advance to the next live child slot
    emitter.instruction("b __rt_array_clone_shallow_refs_loop");                // continue retaining shared child pointers

    // -- restore callee-saved registers and return the cloned array --
    emitter.label("__rt_array_clone_shallow_done");
    emitter.instruction("mov x0, x20");                                         // return the cloned array pointer
    emitter.instruction("ldp x23, x24, [sp, #32]");                             // restore callee-saved x23/x24
    emitter.instruction("ldp x21, x22, [sp, #48]");                             // restore callee-saved x21/x22
    emitter.instruction("ldp x19, x20, [sp, #64]");                             // restore callee-saved x19/x20
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // deallocate the stack frame
    emitter.instruction("ret");                                                 // return with x0 = cloned array pointer
}

fn emit_array_clone_shallow_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_clone_shallow ---");
    emitter.label_global("__rt_array_clone_shallow");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving clone spill slots and callee-saved registers
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved source pointer and cloned array pointer
    emitter.instruction("push r12");                                            // preserve r12 because the clone helper uses it for the source length across nested helper calls
    emitter.instruction("push r13");                                            // preserve r13 because the clone helper uses it for the source capacity across nested helper calls
    emitter.instruction("push r14");                                            // preserve r14 because the clone helper uses it for the source element size across nested helper calls
    emitter.instruction("push r15");                                            // preserve r15 because the clone helper uses it for the packed heap-kind metadata across nested helper calls
    emitter.instruction("sub rsp, 16");                                         // reserve aligned spill slots for the saved source array pointer and cloned array pointer
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the source indexed-array pointer across the clone helper control flow
    emitter.instruction("mov r12, QWORD PTR [rdi]");                            // load the source indexed-array logical length before allocating the clone
    emitter.instruction("mov r13, QWORD PTR [rdi + 8]");                        // load the source indexed-array capacity before allocating the clone
    emitter.instruction("mov r14, QWORD PTR [rdi + 16]");                       // load the source indexed-array element size so the clone keeps the same slot width
    emitter.instruction("mov r15, QWORD PTR [rdi - 8]");                        // load the packed heap-kind metadata from the source indexed-array header
    emitter.instruction("mov rdi, r13");                                        // pass the source indexed-array capacity to the shared array allocator helper
    emitter.instruction("mov rsi, r14");                                        // pass the source indexed-array element size to the shared array allocator helper
    emitter.instruction("call __rt_array_new");                                 // allocate a fresh indexed-array backing store for the clone
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the cloned indexed-array pointer across payload copy and ownership fixups
    emitter.instruction("mov r11, QWORD PTR [rax - 8]");                        // snapshot the freshly allocated clone header so the x86_64 heap marker survives the metadata rewrite
    emitter.instruction("and r11, -65536");                                     // keep the high x86_64 heap-marker bits while clearing the low container-kind payload lane
    emitter.instruction("and r15, 0xffff");                                     // preserve only the stable indexed-array kind, value_type, and copy-on-write metadata bits
    emitter.instruction("or r11, r15");                                         // combine the fresh clone header marker bits with the stable source container-kind payload bits
    emitter.instruction("mov QWORD PTR [rax - 8], r11");                        // stamp the cloned indexed-array with the preserved container metadata without losing the x86_64 heap marker
    emitter.instruction("mov QWORD PTR [rax], r12");                            // restore the source logical length on the cloned indexed-array header
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // reload the source indexed-array pointer before copying its live payload region
    emitter.instruction("lea rsi, [rsi + 24]");                                 // advance to the source indexed-array payload base
    emitter.instruction("lea rdi, [rax + 24]");                                 // advance to the cloned indexed-array payload base
    emitter.instruction("mov rcx, r12");                                        // seed the payload byte-count computation from the source logical length
    emitter.instruction("imul rcx, r14");                                       // compute the number of live payload bytes that must be copied into the clone
    emitter.label("__rt_array_clone_shallow_copy");
    emitter.instruction("test rcx, rcx");                                       // have we copied every live payload byte into the cloned indexed-array storage?
    emitter.instruction("je __rt_array_clone_shallow_fixup");                   // stop the payload copy loop once the live payload region is exhausted
    emitter.instruction("mov r8b, BYTE PTR [rsi]");                             // load the next live payload byte from the source indexed-array storage
    emitter.instruction("mov BYTE PTR [rdi], r8b");                             // store the copied payload byte into the cloned indexed-array storage
    emitter.instruction("add rsi, 1");                                          // advance the source payload cursor after copying one byte
    emitter.instruction("add rdi, 1");                                          // advance the cloned payload cursor after copying one byte
    emitter.instruction("sub rcx, 1");                                          // decrement the number of live payload bytes that still need to be copied
    emitter.instruction("jmp __rt_array_clone_shallow_copy");                   // continue copying the live payload region into the clone
    emitter.label("__rt_array_clone_shallow_fixup");
    emitter.instruction("mov r9, r15");                                         // copy the packed heap-kind metadata before isolating the runtime indexed-array value_type tag
    emitter.instruction("shr r9, 8");                                           // move the runtime indexed-array value_type tag into the low bits for dispatch
    emitter.instruction("and r9, 0x7f");                                        // isolate the runtime indexed-array value_type tag without the persistent container flag
    emitter.instruction("cmp r9, 1");                                           // does the clone store string slots that need fresh owned payloads after the shallow copy?
    emitter.instruction("je __rt_array_clone_shallow_strings");                 // duplicate every string slot so the cloned indexed array owns its own persisted payloads
    emitter.instruction("cmp r9, 4");                                           // does the clone store indexed-array child pointers that need a retain for the cloned owner?
    emitter.instruction("je __rt_array_clone_shallow_refs");                    // retain every live child pointer for indexed-array payloads
    emitter.instruction("cmp r9, 5");                                           // does the clone store associative-array child pointers that need a retain for the cloned owner?
    emitter.instruction("je __rt_array_clone_shallow_refs");                    // retain every live child pointer for associative-array payloads
    emitter.instruction("cmp r9, 6");                                           // does the clone store object child pointers that need a retain for the cloned owner?
    emitter.instruction("je __rt_array_clone_shallow_refs");                    // retain every live child pointer for object payloads
    emitter.instruction("cmp r9, 7");                                           // does the clone store boxed mixed child pointers that need a retain for the cloned owner?
    emitter.instruction("je __rt_array_clone_shallow_refs");                    // retain every live child pointer for boxed mixed payloads
    emitter.instruction("jmp __rt_array_clone_shallow_done");                   // scalar and float payloads are already correct after the shallow byte copy
    emitter.label("__rt_array_clone_shallow_strings");
    emitter.instruction("xor r10d, r10d");                                      // start the cloned string-slot fixup loop from the first live indexed-array slot
    emitter.label("__rt_array_clone_shallow_strings_loop");
    emitter.instruction("cmp r10, r12");                                        // have we duplicated every live string payload owned by the cloned indexed array?
    emitter.instruction("jae __rt_array_clone_shallow_done");                   // finish once every live string slot now owns a persisted payload copy
    emitter.instruction("mov r11, r10");                                        // copy the current string-slot index before scaling it into a 16-byte payload offset
    emitter.instruction("shl r11, 4");                                          // convert the logical string-slot index into the byte offset of the 16-byte payload slot
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // reload the cloned indexed-array pointer before addressing the current string slot
    emitter.instruction("lea r11, [r8 + r11 + 24]");                            // compute the address of the current cloned string slot inside the indexed-array payload region
    emitter.instruction("mov rax, QWORD PTR [r11]");                            // load the shallow-copied string pointer that still aliases the source indexed array
    emitter.instruction("mov rdx, QWORD PTR [r11 + 8]");                        // load the shallow-copied string length for the current cloned string slot
    emitter.instruction("call __rt_str_persist");                               // duplicate the string payload so the cloned indexed-array slot owns an independent persisted copy
    emitter.instruction("mov QWORD PTR [r11], rax");                            // install the newly persisted string pointer back into the cloned indexed-array slot
    emitter.instruction("mov QWORD PTR [r11 + 8], rdx");                        // install the preserved string length back into the cloned indexed-array slot
    emitter.instruction("add r10, 1");                                          // advance to the next live cloned string slot that still needs a persisted payload copy
    emitter.instruction("jmp __rt_array_clone_shallow_strings_loop");           // continue duplicating the live string payloads owned by the cloned indexed array
    emitter.label("__rt_array_clone_shallow_refs");
    emitter.instruction("xor r10d, r10d");                                      // start the cloned child-pointer retain loop from the first live indexed-array slot
    emitter.label("__rt_array_clone_shallow_refs_loop");
    emitter.instruction("cmp r10, r12");                                        // have we retained every live child pointer owned by the cloned indexed array?
    emitter.instruction("jae __rt_array_clone_shallow_done");                   // finish once every live child pointer has been retained for the cloned owner
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // reload the cloned indexed-array pointer before addressing the current child slot
    emitter.instruction("mov rax, QWORD PTR [r8 + r10 * 8 + 24]");              // load the shallow-copied child pointer from the current cloned indexed-array slot
    emitter.instruction("call __rt_incref");                                    // retain the shared child pointer so the cloned indexed-array owner has its own reference
    emitter.instruction("add r10, 1");                                          // advance to the next live child-pointer slot in the cloned indexed-array payload
    emitter.instruction("jmp __rt_array_clone_shallow_refs_loop");              // continue retaining live child pointers for the cloned indexed array
    emitter.label("__rt_array_clone_shallow_done");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return the cloned indexed-array pointer in the x86_64 integer result register
    emitter.instruction("add rsp, 16");                                         // release the spill slots used to preserve the source and cloned indexed-array pointers
    emitter.instruction("pop r15");                                             // restore r15 after using it for packed heap-kind metadata across nested helper calls
    emitter.instruction("pop r14");                                             // restore r14 after using it for the indexed-array element size across nested helper calls
    emitter.instruction("pop r13");                                             // restore r13 after using it for the indexed-array capacity across nested helper calls
    emitter.instruction("pop r12");                                             // restore r12 after using it for the indexed-array logical length across nested helper calls
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning the cloned indexed array
    emitter.instruction("ret");                                                 // return to the caller with rax holding the cloned indexed-array pointer
}
