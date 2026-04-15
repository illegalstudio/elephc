use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_unique_refcounted: create a new array with duplicate pointer-identical refcounted values removed.
/// Input: x0 = array pointer
/// Output: x0 = pointer to new deduplicated array
pub fn emit_array_unique_refcounted(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_unique_refcounted_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_unique_refcounted ---");
    emitter.label_global("__rt_array_unique_refcounted");

    // -- set up stack frame, save source array --
    emitter.instruction("sub sp, sp, #48");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save source array pointer
    emitter.instruction("ldr x9, [x0]");                                        // load source array length
    emitter.instruction("str x9, [sp, #8]");                                    // save source array length

    // -- create destination array with worst-case capacity --
    emitter.instruction("mov x0, x9");                                          // use source length as destination capacity
    emitter.instruction("mov x1, #8");                                          // use 8-byte slots for heap pointers
    emitter.instruction("bl __rt_array_new");                                   // allocate destination array
    emitter.instruction("str x0, [sp, #16]");                                   // save destination array pointer
    emitter.instruction("mov x4, #0");                                          // initialize source index

    emitter.label("__rt_array_unique_ref_outer");
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload source array length
    emitter.instruction("cmp x4, x9");                                          // compare source index with source length
    emitter.instruction("b.ge __rt_array_unique_ref_done");                     // finish once every source element has been considered
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload source array pointer
    emitter.instruction("add x2, x0, #24");                                     // compute source data base
    emitter.instruction("ldr x6, [x2, x4, lsl #3]");                            // load candidate payload
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload destination array pointer
    emitter.instruction("ldr x5, [x0]");                                        // load current destination length
    emitter.instruction("add x3, x0, #24");                                     // compute destination data base
    emitter.instruction("mov x7, #0");                                          // initialize destination scan index

    emitter.label("__rt_array_unique_ref_inner");
    emitter.instruction("cmp x7, x5");                                          // compare scan index with destination length
    emitter.instruction("b.ge __rt_array_unique_ref_add");                      // candidate is unique when scan reaches the end
    emitter.instruction("ldr x8, [x3, x7, lsl #3]");                            // load existing destination payload
    emitter.instruction("cmp x8, x6");                                          // compare pointer identity
    emitter.instruction("b.eq __rt_array_unique_ref_skip");                     // skip candidate when it already exists in the destination
    emitter.instruction("add x7, x7, #1");                                      // increment destination scan index
    emitter.instruction("b __rt_array_unique_ref_inner");                       // continue scanning destination array

    emitter.label("__rt_array_unique_ref_add");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload destination array pointer
    emitter.instruction("mov x1, x6");                                          // move borrowed candidate payload into push argument register
    emitter.instruction("bl __rt_array_push_refcounted");                       // append retained candidate into destination array
    emitter.instruction("str x0, [sp, #16]");                                   // persist destination pointer after possible growth

    emitter.label("__rt_array_unique_ref_skip");
    emitter.instruction("add x4, x4, #1");                                      // increment source index
    emitter.instruction("b __rt_array_unique_ref_outer");                       // continue deduplicating

    emitter.label("__rt_array_unique_ref_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload destination array pointer
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return deduplicated array
}

fn emit_array_unique_refcounted_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_unique_refcounted ---");
    emitter.label_global("__rt_array_unique_refcounted");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving refcounted unique spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the source indexed-array pointer, destination array, and outer loop index
    emitter.instruction("sub rsp, 32");                                         // reserve aligned spill slots for the refcounted unique bookkeeping while keeping helper calls 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the source indexed-array pointer across constructor and append helper calls
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the source indexed-array logical length before sizing the deduplicated destination array
    emitter.instruction("mov rdi, r10");                                        // pass the source indexed-array logical length as the deduplicated destination capacity to the shared constructor
    emitter.instruction("mov rsi, 8");                                          // request 8-byte payload slots for the deduplicated refcounted indexed array
    emitter.instruction("call __rt_array_new");                                 // allocate the deduplicated destination indexed array through the shared x86_64 constructor
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the deduplicated destination indexed-array pointer across the refcounted append helper calls
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // initialize the outer unique loop index to the first source payload slot

    emitter.label("__rt_array_unique_ref_outer_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload the outer unique loop index before reading the next source payload
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the source indexed-array pointer before reading its logical length and selected payload
    emitter.instruction("cmp rcx, QWORD PTR [r10]");                            // compare the outer unique loop index against the source indexed-array logical length
    emitter.instruction("jge __rt_array_unique_ref_done_x86");                  // finish once every source refcounted payload has been considered for deduplication
    emitter.instruction("lea r10, [r10 + 24]");                                 // compute the payload base address for the source indexed array
    emitter.instruction("mov r8, QWORD PTR [r10 + rcx * 8]");                   // load the current candidate refcounted payload from the source indexed array
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the destination indexed-array pointer before scanning for an existing identical payload
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the destination indexed-array logical length before scanning it for a duplicate payload
    emitter.instruction("lea r10, [r10 + 24]");                                 // compute the payload base address for the destination indexed array
    emitter.instruction("xor r9, r9");                                          // initialize the destination scan index to the first payload slot of the deduplicated destination indexed array

    emitter.label("__rt_array_unique_ref_inner_x86");
    emitter.instruction("cmp r9, r11");                                         // compare the destination scan index against the current deduplicated destination length
    emitter.instruction("jge __rt_array_unique_ref_add_x86");                   // append the candidate payload once the existing destination payloads contain no pointer-identical match
    emitter.instruction("cmp r8, QWORD PTR [r10 + r9 * 8]");                    // compare the candidate payload pointer against the current deduplicated destination payload pointer
    emitter.instruction("je __rt_array_unique_ref_skip_x86");                   // skip the candidate payload when it already exists in the deduplicated destination array
    emitter.instruction("add r9, 1");                                           // advance the destination scan index after checking one existing deduplicated payload
    emitter.instruction("jmp __rt_array_unique_ref_inner_x86");                 // continue scanning the deduplicated destination array for a duplicate payload

    emitter.label("__rt_array_unique_ref_add_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // reload the deduplicated destination indexed-array pointer before appending the retained candidate payload
    emitter.instruction("mov rsi, r8");                                         // place the candidate refcounted payload in the second x86_64 append helper argument register
    emitter.instruction("call __rt_array_push_refcounted");                     // append the retained candidate payload into the deduplicated destination indexed array
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // persist the possibly-grown deduplicated destination indexed-array pointer after the append helper returns

    emitter.label("__rt_array_unique_ref_skip_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload the outer unique loop index after helper calls clobbered caller-saved registers
    emitter.instruction("add rcx, 1");                                          // advance the outer unique loop index to the next source refcounted payload
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // persist the updated outer unique loop index across the next iteration
    emitter.instruction("jmp __rt_array_unique_ref_outer_x86");                 // continue deduplicating source refcounted payloads until the source array is exhausted

    emitter.label("__rt_array_unique_ref_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // return the deduplicated destination indexed-array pointer in the standard x86_64 integer result register
    emitter.instruction("add rsp, 32");                                         // release the refcounted unique spill slots before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to the caller
    emitter.instruction("ret");                                                 // return the deduplicated refcounted indexed-array pointer in rax
}
