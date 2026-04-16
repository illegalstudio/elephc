use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_merge: merge two integer arrays into a new array.
/// Input: x0 = first array pointer, x1 = second array pointer
/// Output: x0 = pointer to new merged array
pub fn emit_array_merge(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_merge_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_merge ---");
    emitter.label_global("__rt_array_merge");

    // -- set up stack frame, save source arrays --
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save arr1 pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save arr2 pointer
    emitter.instruction("ldr x9, [x0]");                                        // x9 = arr1 length
    emitter.instruction("str x9, [sp, #16]");                                   // save arr1 length
    emitter.instruction("ldr x10, [x1]");                                       // x10 = arr2 length
    emitter.instruction("str x10, [sp, #24]");                                  // save arr2 length

    // -- create new array with combined capacity --
    emitter.instruction("add x0, x9, x10");                                     // x0 = total capacity = len1 + len2
    emitter.instruction("mov x1, #8");                                          // x1 = elem_size = 8 (integers)
    emitter.instruction("bl __rt_array_new");                                   // allocate new array, x0 = new array ptr
    emitter.instruction("str x0, [sp, #32]");                                   // save new array pointer

    // -- copy arr1 elements to new array --
    emitter.instruction("ldr x1, [sp, #0]");                                    // x1 = arr1 pointer
    emitter.instruction("ldr x9, [sp, #16]");                                   // x9 = arr1 length
    emitter.instruction("add x2, x1, #24");                                     // x2 = arr1 data base
    emitter.instruction("add x3, x0, #24");                                     // x3 = new array data base
    emitter.instruction("mov x4, #0");                                          // x4 = i = 0

    emitter.label("__rt_array_merge_copy1");
    emitter.instruction("cmp x4, x9");                                          // compare i with arr1 length
    emitter.instruction("b.ge __rt_array_merge_copy2_setup");                   // if done with arr1, move to arr2
    emitter.instruction("ldr x5, [x2, x4, lsl #3]");                            // x5 = arr1[i]
    emitter.instruction("str x5, [x3, x4, lsl #3]");                            // new_arr[i] = arr1[i]
    emitter.instruction("add x4, x4, #1");                                      // i += 1
    emitter.instruction("b __rt_array_merge_copy1");                            // continue loop

    // -- copy arr2 elements after arr1 elements --
    emitter.label("__rt_array_merge_copy2_setup");
    emitter.instruction("ldr x1, [sp, #8]");                                    // x1 = arr2 pointer
    emitter.instruction("ldr x10, [sp, #24]");                                  // x10 = arr2 length
    emitter.instruction("add x2, x1, #24");                                     // x2 = arr2 data base
    emitter.instruction("ldr x0, [sp, #32]");                                   // x0 = new array pointer
    emitter.instruction("ldr x9, [sp, #16]");                                   // x9 = arr1 length (offset for writing)
    emitter.instruction("add x3, x0, #24");                                     // x3 = new array data base
    emitter.instruction("mov x4, #0");                                          // x4 = j = 0

    emitter.label("__rt_array_merge_copy2");
    emitter.instruction("cmp x4, x10");                                         // compare j with arr2 length
    emitter.instruction("b.ge __rt_array_merge_done");                          // if done with arr2, finish up
    emitter.instruction("ldr x5, [x2, x4, lsl #3]");                            // x5 = arr2[j]
    emitter.instruction("add x6, x9, x4");                                      // x6 = arr1_len + j (destination index)
    emitter.instruction("str x5, [x3, x6, lsl #3]");                            // new_arr[arr1_len + j] = arr2[j]
    emitter.instruction("add x4, x4, #1");                                      // j += 1
    emitter.instruction("b __rt_array_merge_copy2");                            // continue loop

    // -- set length on new array and return --
    emitter.label("__rt_array_merge_done");
    emitter.instruction("ldr x0, [sp, #32]");                                   // x0 = new array pointer
    emitter.instruction("ldr x9, [sp, #16]");                                   // x9 = arr1 length
    emitter.instruction("ldr x10, [sp, #24]");                                  // x10 = arr2 length
    emitter.instruction("add x9, x9, x10");                                     // x9 = total length
    emitter.instruction("str x9, [x0]");                                        // set new array length

    // -- tear down stack frame and return --
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = merged array
}

fn emit_array_merge_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_merge ---");
    emitter.label_global("__rt_array_merge");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving scalar merge spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the two source indexed-array pointers and the merged result pointer
    emitter.instruction("sub rsp, 48");                                         // reserve aligned spill slots for the source indexed-array pointers, lengths, and merged result pointer across constructor calls
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the first source indexed-array pointer across the merged-array constructor call
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the second source indexed-array pointer across the merged-array constructor call
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the first source indexed-array logical length before deriving the merged capacity
    emitter.instruction("mov r11, QWORD PTR [rsi]");                            // load the second source indexed-array logical length before deriving the merged capacity
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // preserve the first source indexed-array logical length across the merged-array constructor call
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // preserve the second source indexed-array logical length across the merged-array constructor call
    emitter.instruction("mov rdi, r10");                                        // seed the merged-array capacity from the first source indexed-array logical length
    emitter.instruction("add rdi, r11");                                        // extend the merged-array capacity by the second source indexed-array logical length
    emitter.instruction("mov rsi, 8");                                          // request 8-byte scalar payload slots for the merged indexed array
    emitter.instruction("call __rt_array_new");                                 // allocate the merged indexed array through the shared x86_64 constructor
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the merged indexed-array pointer across the scalar copy loops
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload the first source indexed-array pointer after the constructor clobbered caller-saved registers
    emitter.instruction("lea r8, [r8 + 24]");                                   // compute the first scalar payload slot address in the first source indexed array
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload the merged indexed-array pointer before seeding the first scalar copy loop
    emitter.instruction("lea r9, [r9 + 24]");                                   // compute the first scalar payload slot address in the merged indexed array
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the first source indexed-array logical length after the constructor clobbered caller-saved registers
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // reload the second source indexed-array logical length after the constructor clobbered caller-saved registers
    emitter.instruction("xor ecx, ecx");                                        // initialize the first source copy cursor to the front of the merged indexed array

    emitter.label("__rt_array_merge_copy1_x86");
    emitter.instruction("cmp rcx, r10");                                        // compare the first source copy cursor against the first indexed-array logical length
    emitter.instruction("jge __rt_array_merge_copy2_setup_x86");                // continue with the second source once every first-array scalar payload has been copied
    emitter.instruction("mov rax, QWORD PTR [r8 + rcx * 8]");                   // load the current scalar payload from the first source indexed array
    emitter.instruction("mov QWORD PTR [r9 + rcx * 8], rax");                   // store that scalar payload into the matching slot of the merged indexed array
    emitter.instruction("add rcx, 1");                                          // advance the first source copy cursor after copying one scalar payload
    emitter.instruction("jmp __rt_array_merge_copy1_x86");                      // continue copying the first source indexed-array payloads into the merged array

    emitter.label("__rt_array_merge_copy2_setup_x86");
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // reload the second source indexed-array pointer before copying its scalar payloads
    emitter.instruction("lea r8, [r8 + 24]");                                   // compute the first scalar payload slot address in the second source indexed array
    emitter.instruction("xor ecx, ecx");                                        // initialize the second source copy cursor to the front of the second indexed array

    emitter.label("__rt_array_merge_copy2_x86");
    emitter.instruction("cmp rcx, r11");                                        // compare the second source copy cursor against the second indexed-array logical length
    emitter.instruction("jge __rt_array_merge_done_x86");                       // finish once every second-array scalar payload has been appended to the merged array
    emitter.instruction("mov rax, QWORD PTR [r8 + rcx * 8]");                   // load the current scalar payload from the second source indexed array
    emitter.instruction("mov rdx, r10");                                        // seed the merged destination index from the first source indexed-array logical length
    emitter.instruction("add rdx, rcx");                                        // offset the merged destination index by the current second-source copy cursor
    emitter.instruction("mov QWORD PTR [r9 + rdx * 8], rax");                   // store the second-source scalar payload after the copied first-source prefix in the merged array
    emitter.instruction("add rcx, 1");                                          // advance the second source copy cursor after copying one scalar payload
    emitter.instruction("jmp __rt_array_merge_copy2_x86");                      // continue appending second-source scalar payloads into the merged array

    emitter.label("__rt_array_merge_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the merged indexed-array pointer before publishing its final logical length
    emitter.instruction("add r10, r11");                                        // compute the merged indexed-array logical length from the two source lengths
    emitter.instruction("mov QWORD PTR [rax], r10");                            // store the merged indexed-array logical length back into the merged array header
    emitter.instruction("add rsp, 48");                                         // release the scalar merge spill slots before returning to the caller
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the scalar merge helper completes
    emitter.instruction("ret");                                                 // return the merged indexed-array pointer in rax
}
