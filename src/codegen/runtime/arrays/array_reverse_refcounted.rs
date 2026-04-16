use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_reverse_refcounted: create a reversed copy of a refcounted array.
/// Input: x0 = array pointer
/// Output: x0 = pointer to new reversed array
pub fn emit_array_reverse_refcounted(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_reverse_refcounted_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_reverse_refcounted ---");
    emitter.label_global("__rt_array_reverse_refcounted");

    // -- set up stack frame, save source array info --
    emitter.instruction("sub sp, sp, #48");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save source array pointer
    emitter.instruction("ldr x9, [x0]");                                        // load source array length
    emitter.instruction("str x9, [sp, #8]");                                    // save source length

    // -- create destination array with matching capacity --
    emitter.instruction("mov x0, x9");                                          // use source length as destination capacity
    emitter.instruction("mov x1, #8");                                          // use 8-byte slots for heap pointers
    emitter.instruction("bl __rt_array_new");                                   // allocate destination array
    emitter.instruction("str x0, [sp, #16]");                                   // save destination array pointer

    // -- iterate backwards and append retained payloads --
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload source length after allocator call
    emitter.instruction("sub x4, x9, #1");                                      // initialize source index to the last element
    emitter.label("__rt_array_reverse_ref_loop");
    emitter.instruction("cmp x4, #0");                                          // test whether source index dropped below zero
    emitter.instruction("b.lt __rt_array_reverse_ref_done");                    // finish when every source element has been copied
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload source array pointer
    emitter.instruction("add x2, x1, #24");                                     // compute source data base
    emitter.instruction("ldr x1, [x2, x4, lsl #3]");                            // load borrowed source payload
    emitter.instruction("str x4, [sp, #24]");                                   // preserve source index across helper calls
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload destination array pointer
    emitter.instruction("bl __rt_array_push_refcounted");                       // append retained payload into destination array
    emitter.instruction("str x0, [sp, #16]");                                   // persist destination pointer after possible growth
    emitter.instruction("ldr x4, [sp, #24]");                                   // restore source index after helper calls
    emitter.instruction("sub x4, x4, #1");                                      // decrement source index
    emitter.instruction("b __rt_array_reverse_ref_loop");                       // continue reversing

    emitter.label("__rt_array_reverse_ref_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload destination array pointer
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return reversed array
}

fn emit_array_reverse_refcounted_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_reverse_refcounted ---");
    emitter.label_global("__rt_array_reverse_refcounted");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving refcounted reverse spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the source indexed-array pointer, length, and reversed destination array
    emitter.instruction("sub rsp, 32");                                         // reserve aligned spill slots for the refcounted reverse bookkeeping while keeping helper calls 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the source indexed-array pointer across the destination constructor call
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the source indexed-array logical length before sizing the reversed destination array
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // preserve the source indexed-array logical length across the destination constructor call and reverse loop
    emitter.instruction("mov rdi, r10");                                        // pass the source indexed-array logical length as the reversed destination capacity to the shared constructor
    emitter.instruction("mov rsi, 8");                                          // request 8-byte payload slots for the reversed refcounted indexed array
    emitter.instruction("call __rt_array_new");                                 // allocate the reversed destination indexed array through the shared x86_64 constructor
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the reversed destination indexed-array pointer across the refcounted append helper calls
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // initialize the reverse-copy index to the first destination payload slot

    emitter.label("__rt_array_reverse_ref_loop_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the reverse-copy index before selecting the next source payload from the tail
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 16]");                       // compare the reverse-copy index against the source indexed-array logical length
    emitter.instruction("jge __rt_array_reverse_ref_done_x86");                 // finish once every source refcounted payload has been copied into the reversed destination array
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the source indexed-array logical length before deriving the current reverse source index
    emitter.instruction("sub r10, rcx");                                        // back up from the source length by the number of already-copied payloads
    emitter.instruction("sub r10, 1");                                          // land on the current reverse-order source payload index
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload the source indexed-array pointer before reading the reverse-order payload
    emitter.instruction("lea r11, [r11 + 24]");                                 // compute the payload base address for the source indexed array
    emitter.instruction("mov rsi, QWORD PTR [r11 + r10 * 8]");                  // load the current borrowed refcounted payload from the source indexed array
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the reversed destination indexed-array pointer before appending the retained payload
    emitter.instruction("call __rt_array_push_refcounted");                     // append the retained reverse-order payload into the reversed destination indexed array
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // persist the possibly-grown reversed destination indexed-array pointer after the append helper returns
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the reverse-copy index after helper calls clobbered caller-saved registers
    emitter.instruction("add rcx, 1");                                          // advance the reverse-copy index after appending one reverse-order payload
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // persist the updated reverse-copy index across the next append helper call
    emitter.instruction("jmp __rt_array_reverse_ref_loop_x86");                 // continue appending reverse-order refcounted payloads until the source array is exhausted

    emitter.label("__rt_array_reverse_ref_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the reversed destination indexed-array pointer in the standard x86_64 integer result register
    emitter.instruction("add rsp, 32");                                         // release the refcounted reverse spill slots before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to the caller
    emitter.instruction("ret");                                                 // return the reversed refcounted indexed-array pointer in rax
}
