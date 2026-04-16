use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_unique: create a new array with duplicate integer values removed.
/// Input: x0 = array pointer
/// Output: x0 = pointer to new deduplicated array
/// Uses O(n^2) comparison — simple but correct for small arrays.
pub fn emit_array_unique(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_unique_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_unique ---");
    emitter.label_global("__rt_array_unique");

    // -- set up stack frame, save source array --
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save source array pointer
    emitter.instruction("ldr x9, [x0]");                                        // x9 = source array length
    emitter.instruction("str x9, [sp, #8]");                                    // save source length

    // -- create new array with same capacity (worst case: all unique) --
    emitter.instruction("mov x0, x9");                                          // x0 = capacity = source length
    emitter.instruction("mov x1, #8");                                          // x1 = elem_size = 8 (integers)
    emitter.instruction("bl __rt_array_new");                                   // allocate new array
    emitter.instruction("str x0, [sp, #16]");                                   // save new array pointer

    // -- iterate source array, add each element if not already in new array --
    emitter.instruction("ldr x1, [sp, #0]");                                    // x1 = source array pointer
    emitter.instruction("ldr x9, [sp, #8]");                                    // x9 = source length
    emitter.instruction("add x2, x1, #24");                                     // x2 = source data base
    emitter.instruction("add x3, x0, #24");                                     // x3 = dest data base
    emitter.instruction("mov x4, #0");                                          // x4 = src_i = 0 (source index)
    emitter.instruction("mov x5, #0");                                          // x5 = dst_len = 0 (dest length so far)

    emitter.label("__rt_array_unique_outer");
    emitter.instruction("cmp x4, x9");                                          // compare src_i with source length
    emitter.instruction("b.ge __rt_array_unique_done");                         // if src_i >= length, we're done
    emitter.instruction("ldr x6, [x2, x4, lsl #3]");                            // x6 = source[src_i] (candidate element)

    // -- check if candidate already exists in dest array --
    emitter.instruction("mov x7, #0");                                          // x7 = check_i = 0

    emitter.label("__rt_array_unique_inner");
    emitter.instruction("cmp x7, x5");                                          // compare check_i with dest length
    emitter.instruction("b.ge __rt_array_unique_add");                          // if checked all dest, element is unique
    emitter.instruction("ldr x8, [x3, x7, lsl #3]");                            // x8 = dest[check_i]
    emitter.instruction("cmp x8, x6");                                          // compare with candidate
    emitter.instruction("b.eq __rt_array_unique_skip");                         // if equal, it's a duplicate — skip
    emitter.instruction("add x7, x7, #1");                                      // check_i += 1
    emitter.instruction("b __rt_array_unique_inner");                           // continue inner loop

    // -- element is unique, add to dest array --
    emitter.label("__rt_array_unique_add");
    emitter.instruction("str x6, [x3, x5, lsl #3]");                            // dest[dst_len] = candidate
    emitter.instruction("add x5, x5, #1");                                      // dst_len += 1

    // -- advance to next source element --
    emitter.label("__rt_array_unique_skip");
    emitter.instruction("add x4, x4, #1");                                      // src_i += 1
    emitter.instruction("b __rt_array_unique_outer");                           // continue outer loop

    // -- set final length and return --
    emitter.label("__rt_array_unique_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // x0 = new array pointer
    emitter.instruction("str x5, [x0]");                                        // set array length = number of unique elements

    // -- tear down stack frame and return --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = deduplicated array
}

fn emit_array_unique_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_unique ---");
    emitter.label_global("__rt_array_unique");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving scalar unique spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the source indexed-array pointer, source length, and deduplicated result pointer
    emitter.instruction("sub rsp, 32");                                         // reserve aligned spill slots for the scalar unique bookkeeping while keeping constructor calls 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the source indexed-array pointer across the deduplicated-array constructor call
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the source indexed-array logical length before constructing the deduplicated result array
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // preserve the source indexed-array logical length across the constructor call and uniqueness scan loops
    emitter.instruction("mov rdi, r10");                                        // pass the source indexed-array logical length as the worst-case deduplicated-array capacity to the shared constructor
    emitter.instruction("mov rsi, 8");                                          // request 8-byte scalar payload slots for the deduplicated indexed array
    emitter.instruction("call __rt_array_new");                                 // allocate the deduplicated indexed array through the shared x86_64 constructor
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the deduplicated indexed-array pointer across the nested uniqueness scan loops
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload the source indexed-array pointer after the constructor clobbered caller-saved registers
    emitter.instruction("lea r8, [r8 + 24]");                                   // compute the first scalar payload slot address in the source indexed array
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // reload the deduplicated indexed-array pointer before seeding the uniqueness scan loops
    emitter.instruction("lea r9, [r9 + 24]");                                   // compute the first scalar payload slot address in the deduplicated indexed array
    emitter.instruction("xor ecx, ecx");                                        // initialize the source uniqueness cursor at the front of the source indexed array
    emitter.instruction("xor r10d, r10d");                                      // initialize the deduplicated indexed-array logical length to zero before scanning any source payloads

    emitter.label("__rt_array_unique_outer_x86");
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 16]");                       // compare the source uniqueness cursor against the source indexed-array logical length
    emitter.instruction("jge __rt_array_unique_done_x86");                      // finish once every source scalar payload has been checked for prior duplicates
    emitter.instruction("mov rax, QWORD PTR [r8 + rcx * 8]");                   // load the current source scalar payload that might need to be appended to the deduplicated indexed array
    emitter.instruction("xor edx, edx");                                        // initialize the deduplicated-array scan cursor at the front of the current unique-prefix result

    emitter.label("__rt_array_unique_inner_x86");
    emitter.instruction("cmp rdx, r10");                                        // compare the deduplicated-array scan cursor against the number of scalar payloads already accepted as unique
    emitter.instruction("jge __rt_array_unique_add_x86");                       // append the current source scalar payload when no prior unique slot matches it
    emitter.instruction("cmp QWORD PTR [r9 + rdx * 8], rax");                   // compare the current source scalar payload against one previously accepted unique scalar value
    emitter.instruction("je __rt_array_unique_skip_x86");                       // skip the append path when the current source scalar payload is a duplicate of an earlier accepted value
    emitter.instruction("add rdx, 1");                                          // advance the deduplicated-array scan cursor after ruling out one prior unique scalar value
    emitter.instruction("jmp __rt_array_unique_inner_x86");                     // continue scanning earlier accepted unique scalar payloads for duplicates

    emitter.label("__rt_array_unique_add_x86");
    emitter.instruction("mov QWORD PTR [r9 + r10 * 8], rax");                   // append the current source scalar payload at the end of the deduplicated indexed array
    emitter.instruction("add r10, 1");                                          // advance the deduplicated indexed-array logical length after accepting one new unique scalar payload

    emitter.label("__rt_array_unique_skip_x86");
    emitter.instruction("add rcx, 1");                                          // advance the source uniqueness cursor after deciding whether to append the current source payload
    emitter.instruction("jmp __rt_array_unique_outer_x86");                     // continue scanning source scalar payloads until the source indexed array is exhausted

    emitter.label("__rt_array_unique_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // reload the deduplicated indexed-array pointer before publishing its final logical length
    emitter.instruction("mov QWORD PTR [rax], r10");                            // store the number of accepted unique scalar payloads as the deduplicated array length
    emitter.instruction("add rsp, 32");                                         // release the scalar unique spill slots before returning to the caller
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer after the scalar unique helper completes
    emitter.instruction("ret");                                                 // return the deduplicated indexed-array pointer in rax
}
