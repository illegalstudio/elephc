use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_merge_refcounted: merge two arrays of refcounted 8-byte payloads into a new array.
/// Input: x0 = first array pointer, x1 = second array pointer
/// Output: x0 = pointer to new merged array
pub fn emit_array_merge_refcounted(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_merge_refcounted_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_merge_refcounted ---");
    emitter.label_global("__rt_array_merge_refcounted");

    // -- set up stack frame, save source arrays --
    emitter.instruction("sub sp, sp, #80");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save first array pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save second array pointer
    emitter.instruction("ldr x9, [x0]");                                        // load first array length
    emitter.instruction("str x9, [sp, #16]");                                   // save first array length
    emitter.instruction("ldr x10, [x1]");                                       // load second array length
    emitter.instruction("str x10, [sp, #24]");                                  // save second array length

    // -- create destination array with combined capacity --
    emitter.instruction("add x0, x9, x10");                                     // compute merged capacity
    emitter.instruction("mov x1, #8");                                          // use 8-byte element slots for heap pointers
    emitter.instruction("bl __rt_array_new");                                   // allocate destination array
    emitter.instruction("str x0, [sp, #32]");                                   // save destination array pointer

    // -- copy first array into destination, retaining each payload --
    emitter.instruction("mov x4, #0");                                          // initialize first loop index
    emitter.label("__rt_array_merge_ref_copy1");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload first array length
    emitter.instruction("cmp x4, x9");                                          // compare index with first array length
    emitter.instruction("b.ge __rt_array_merge_ref_copy2_setup");               // move to second array when first is done
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload first array pointer
    emitter.instruction("add x2, x1, #24");                                     // compute first array data base
    emitter.instruction("ldr x5, [x2, x4, lsl #3]");                            // load first array element pointer
    emitter.instruction("str x5, [sp, #40]");                                   // preserve element pointer across incref call
    emitter.instruction("mov x0, x5");                                          // move element pointer into incref argument register
    emitter.instruction("bl __rt_incref");                                      // retain borrowed payload for destination ownership
    emitter.instruction("ldr x5, [sp, #40]");                                   // restore retained payload pointer
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload destination array pointer
    emitter.instruction("add x3, x0, #24");                                     // compute destination data base
    emitter.instruction("str x5, [x3, x4, lsl #3]");                            // store retained payload into destination
    emitter.instruction("add x4, x4, #1");                                      // increment first loop index
    emitter.instruction("b __rt_array_merge_ref_copy1");                        // continue copying first array

    // -- copy second array after the first segment --
    emitter.label("__rt_array_merge_ref_copy2_setup");
    emitter.instruction("mov x4, #0");                                          // initialize second loop index
    emitter.label("__rt_array_merge_ref_copy2");
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload second array length
    emitter.instruction("cmp x4, x10");                                         // compare index with second array length
    emitter.instruction("b.ge __rt_array_merge_ref_done");                      // finish once second array is copied
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload second array pointer
    emitter.instruction("add x2, x1, #24");                                     // compute second array data base
    emitter.instruction("ldr x5, [x2, x4, lsl #3]");                            // load second array element pointer
    emitter.instruction("str x5, [sp, #40]");                                   // preserve element pointer across incref call
    emitter.instruction("mov x0, x5");                                          // move element pointer into incref argument register
    emitter.instruction("bl __rt_incref");                                      // retain borrowed payload for destination ownership
    emitter.instruction("ldr x5, [sp, #40]");                                   // restore retained payload pointer
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload destination array pointer
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload first array length as destination offset
    emitter.instruction("add x6, x9, x4");                                      // compute destination index after first segment
    emitter.instruction("add x3, x0, #24");                                     // compute destination data base
    emitter.instruction("str x5, [x3, x6, lsl #3]");                            // store retained payload into destination
    emitter.instruction("add x4, x4, #1");                                      // increment second loop index
    emitter.instruction("b __rt_array_merge_ref_copy2");                        // continue copying second array

    // -- finalize destination length and return --
    emitter.label("__rt_array_merge_ref_done");
    emitter.instruction("ldr x0, [sp, #32]");                                   // reload destination array pointer
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload first array length
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload second array length
    emitter.instruction("add x9, x9, x10");                                     // compute merged length
    emitter.instruction("str x9, [x0]");                                        // store merged length into destination header
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return merged array in x0
}

fn emit_array_merge_refcounted_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_merge_refcounted ---");
    emitter.label_global("__rt_array_merge_refcounted");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving refcounted merge spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for both source arrays, their lengths, and the destination array
    emitter.instruction("sub rsp, 48");                                         // reserve aligned spill slots for the refcounted merge bookkeeping while keeping helper calls 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the first source indexed-array pointer across constructor and append helper calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the second source indexed-array pointer across constructor and append helper calls
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the first source indexed-array logical length before sizing the merged destination array
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // preserve the first source indexed-array logical length across the destination constructor call
    emitter.instruction("mov r11, QWORD PTR [rsi]");                            // load the second source indexed-array logical length before sizing the merged destination array
    emitter.instruction("mov QWORD PTR [rbp - 32], r11");                       // preserve the second source indexed-array logical length across the destination constructor call
    emitter.instruction("mov rdi, r10");                                        // seed the merged destination capacity from the first source indexed-array logical length
    emitter.instruction("add rdi, r11");                                        // add the second source indexed-array logical length to compute the merged destination capacity
    emitter.instruction("mov rsi, 8");                                          // request 8-byte payload slots for the merged destination indexed array
    emitter.instruction("call __rt_array_new");                                 // allocate the merged destination indexed array through the shared x86_64 constructor
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve the merged destination indexed-array pointer across the refcounted append helper calls
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // initialize the merge loop index to the first payload slot of the first source indexed array

    emitter.label("__rt_array_merge_ref_copy_first_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the first-source merge loop index before reading the next refcounted payload
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 24]");                       // compare the first-source merge loop index against the first source indexed-array logical length
    emitter.instruction("jge __rt_array_merge_ref_copy_second_setup_x86");      // switch to the second source indexed array once the first source has been exhausted
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the first source indexed-array pointer before reading the current payload
    emitter.instruction("lea r10, [r10 + 24]");                                 // compute the payload base address for the first source indexed array
    emitter.instruction("mov rsi, QWORD PTR [r10 + rcx * 8]");                  // load the current borrowed refcounted payload from the first source indexed array
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // reload the merged destination indexed-array pointer before appending the retained payload
    emitter.instruction("call __rt_array_push_refcounted");                     // append the retained first-source payload into the merged destination indexed array
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // persist the possibly-grown merged destination indexed-array pointer after the append helper returns
    emitter.instruction("add rcx, 1");                                          // advance the first-source merge loop index after appending one refcounted payload
    emitter.instruction("mov QWORD PTR [rbp - 48], rcx");                       // persist the updated first-source merge loop index across the next append helper call
    emitter.instruction("jmp __rt_array_merge_ref_copy_first_x86");             // continue appending refcounted payloads from the first source indexed array

    emitter.label("__rt_array_merge_ref_copy_second_setup_x86");
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // reset the merge loop index before appending payloads from the second source indexed array

    emitter.label("__rt_array_merge_ref_copy_second_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // reload the second-source merge loop index before reading the next refcounted payload
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 32]");                       // compare the second-source merge loop index against the second source indexed-array logical length
    emitter.instruction("jge __rt_array_merge_ref_done_x86");                   // finish once every refcounted payload from the second source indexed array has been appended
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the second source indexed-array pointer before reading the current payload
    emitter.instruction("lea r10, [r10 + 24]");                                 // compute the payload base address for the second source indexed array
    emitter.instruction("mov rsi, QWORD PTR [r10 + rcx * 8]");                  // load the current borrowed refcounted payload from the second source indexed array
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // reload the merged destination indexed-array pointer before appending the retained payload
    emitter.instruction("call __rt_array_push_refcounted");                     // append the retained second-source payload into the merged destination indexed array
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // persist the possibly-grown merged destination indexed-array pointer after the append helper returns
    emitter.instruction("add rcx, 1");                                          // advance the second-source merge loop index after appending one refcounted payload
    emitter.instruction("mov QWORD PTR [rbp - 48], rcx");                       // persist the updated second-source merge loop index across the next append helper call
    emitter.instruction("jmp __rt_array_merge_ref_copy_second_x86");            // continue appending refcounted payloads from the second source indexed array

    emitter.label("__rt_array_merge_ref_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // return the merged destination indexed-array pointer in the standard x86_64 integer result register
    emitter.instruction("add rsp, 48");                                         // release the refcounted merge spill slots before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to the caller
    emitter.instruction("ret");                                                 // return the merged refcounted indexed-array pointer in rax
}
