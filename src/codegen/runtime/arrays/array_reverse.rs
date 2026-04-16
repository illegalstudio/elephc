use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_reverse: create a reversed copy of an integer array.
/// Input: x0 = array pointer
/// Output: x0 = pointer to new reversed array
pub fn emit_array_reverse(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_reverse_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_reverse ---");
    emitter.label_global("__rt_array_reverse");

    // -- set up stack frame, save source array info --
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save source array pointer
    emitter.instruction("ldr x9, [x0]");                                        // x9 = source array length
    emitter.instruction("str x9, [sp, #8]");                                    // save length to stack

    // -- create new array with same capacity --
    emitter.instruction("mov x0, x9");                                          // x0 = capacity = source length
    emitter.instruction("mov x1, #8");                                          // x1 = elem_size = 8 (integers)
    emitter.instruction("bl __rt_array_new");                                   // allocate new array, x0 = new array ptr
    emitter.instruction("str x0, [sp, #16]");                                   // save new array pointer

    // -- set up copy loop: copy elements in reverse --
    emitter.instruction("ldr x1, [sp, #0]");                                    // x1 = source array pointer
    emitter.instruction("ldr x9, [sp, #8]");                                    // x9 = length
    emitter.instruction("add x2, x1, #24");                                     // x2 = source data base
    emitter.instruction("add x3, x0, #24");                                     // x3 = dest data base
    emitter.instruction("sub x4, x9, #1");                                      // x4 = src_index = length - 1 (start from end)
    emitter.instruction("mov x5, #0");                                          // x5 = dst_index = 0

    // -- copy loop: read from end, write to start --
    emitter.label("__rt_array_reverse_loop");
    emitter.instruction("cmp x4, #0");                                          // check if src_index < 0
    emitter.instruction("b.lt __rt_array_reverse_done");                        // if so, copying is complete
    emitter.instruction("ldr x6, [x2, x4, lsl #3]");                            // x6 = source[src_index]
    emitter.instruction("str x6, [x3, x5, lsl #3]");                            // dest[dst_index] = x6
    emitter.instruction("sub x4, x4, #1");                                      // src_index -= 1
    emitter.instruction("add x5, x5, #1");                                      // dst_index += 1
    emitter.instruction("b __rt_array_reverse_loop");                           // continue loop

    // -- set length on new array and return --
    emitter.label("__rt_array_reverse_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // x0 = new array pointer
    emitter.instruction("ldr x9, [sp, #8]");                                    // x9 = length
    emitter.instruction("str x9, [x0]");                                        // set new array length = source length

    // -- tear down stack frame and return --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = new reversed array
}

fn emit_array_reverse_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_reverse ---");
    emitter.label_global("__rt_array_reverse");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving scalar reverse spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the source indexed-array pointer, source length, and reversed result pointer
    emitter.instruction("sub rsp, 32");                                         // reserve aligned spill slots for the scalar reverse bookkeeping while keeping constructor calls 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the source indexed-array pointer across the reversed-array constructor call
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the source indexed-array logical length before constructing the reversed result array
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // preserve the source indexed-array logical length across the constructor call and reverse-copy loop
    emitter.instruction("mov rdi, r10");                                        // pass the source indexed-array logical length as the reversed-array capacity to the shared constructor
    emitter.instruction("mov rsi, 8");                                          // request 8-byte scalar payload slots for the reversed indexed array
    emitter.instruction("call __rt_array_new");                                 // allocate the reversed indexed array through the shared x86_64 constructor
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the reversed indexed-array pointer across the scalar reverse-copy loop
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload the source indexed-array pointer after the constructor clobbered caller-saved registers
    emitter.instruction("lea r8, [r8 + 24]");                                   // compute the first scalar payload slot address in the source indexed array
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload the reversed indexed-array pointer before seeding the reverse-copy loop
    emitter.instruction("lea r9, [r9 + 24]");                                   // compute the first scalar payload slot address in the reversed indexed array
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the source indexed-array logical length after the constructor clobbered caller-saved registers
    emitter.instruction("xor ecx, ecx");                                        // initialize the reversed destination cursor at the front of the reversed indexed array

    emitter.label("__rt_array_reverse_loop_x86");
    emitter.instruction("cmp rcx, r10");                                        // compare the reversed destination cursor against the source indexed-array logical length
    emitter.instruction("jge __rt_array_reverse_done_x86");                     // finish once every source scalar payload has been copied into the reversed indexed array
    emitter.instruction("mov r11, r10");                                        // seed the source reverse index from the source indexed-array logical length
    emitter.instruction("sub r11, rcx");                                        // back up the source reverse index by the number of already-copied scalar payloads
    emitter.instruction("sub r11, 1");                                          // land on the current source scalar payload that must appear next in reverse order
    emitter.instruction("mov rax, QWORD PTR [r8 + r11 * 8]");                   // load the current reverse-ordered scalar payload from the source indexed array
    emitter.instruction("mov QWORD PTR [r9 + rcx * 8], rax");                   // store that scalar payload into the next front slot of the reversed indexed array
    emitter.instruction("add rcx, 1");                                          // advance the reversed destination cursor after copying one reverse-ordered scalar payload
    emitter.instruction("jmp __rt_array_reverse_loop_x86");                     // continue copying reverse-ordered scalar payloads until the source array is exhausted

    emitter.label("__rt_array_reverse_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the reversed indexed-array pointer before publishing its logical length
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the source indexed-array logical length before storing it into the reversed array header
    emitter.instruction("mov QWORD PTR [rax], r10");                            // store the full source indexed-array logical length as the reversed array length
    emitter.instruction("add rsp, 32");                                         // release the scalar reverse spill slots before returning to the caller
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the scalar reverse helper completes
    emitter.instruction("ret");                                                 // return the reversed indexed-array pointer in rax
}
