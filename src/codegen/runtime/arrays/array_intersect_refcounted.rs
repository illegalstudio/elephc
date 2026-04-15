use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_intersect_refcounted: return elements present in both arrays for refcounted payload arrays.
/// Input:  x0=arr1, x1=arr2
/// Output: x0=new array containing retained elements found in both arrays
pub fn emit_array_intersect_refcounted(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_intersect_refcounted_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_intersect_refcounted ---");
    emitter.label_global("__rt_array_intersect_refcounted");

    // -- set up stack frame, save arguments --
    emitter.instruction("sub sp, sp, #48");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save first array pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save second array pointer

    // -- create result array with same capacity as arr1 --
    emitter.instruction("ldr x0, [x0, #8]");                                    // load first array capacity
    emitter.instruction("mov x1, #8");                                          // use 8-byte slots for heap pointers
    emitter.instruction("bl __rt_array_new");                                   // allocate result array
    emitter.instruction("str x0, [sp, #16]");                                   // save result array pointer
    emitter.instruction("str xzr, [sp, #24]");                                  // initialize outer loop index

    emitter.label("__rt_array_isect_ref_outer");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload first array pointer
    emitter.instruction("ldr x3, [x0]");                                        // load first array length
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload outer loop index
    emitter.instruction("cmp x4, x3");                                          // compare index with first array length
    emitter.instruction("b.ge __rt_array_isect_ref_done");                      // finish once every source element has been considered
    emitter.instruction("add x5, x0, #24");                                     // compute first array data base
    emitter.instruction("ldr x6, [x5, x4, lsl #3]");                            // load candidate payload from first array
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload second array pointer
    emitter.instruction("ldr x7, [x1]");                                        // load second array length
    emitter.instruction("add x8, x1, #24");                                     // compute second array data base
    emitter.instruction("mov x9, #0");                                          // initialize inner loop index

    emitter.label("__rt_array_isect_ref_inner");
    emitter.instruction("cmp x9, x7");                                          // compare inner index with second array length
    emitter.instruction("b.ge __rt_array_isect_ref_skip");                      // skip candidate when it does not appear in arr2
    emitter.instruction("ldr x10, [x8, x9, lsl #3]");                           // load second array payload
    emitter.instruction("cmp x6, x10");                                         // compare pointer identity
    emitter.instruction("b.eq __rt_array_isect_ref_match");                     // candidate appears in both arrays
    emitter.instruction("add x9, x9, #1");                                      // increment inner loop index
    emitter.instruction("b __rt_array_isect_ref_inner");                        // continue scanning arr2

    emitter.label("__rt_array_isect_ref_match");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload result array pointer
    emitter.instruction("mov x1, x6");                                          // move borrowed candidate payload into push argument register
    emitter.instruction("bl __rt_array_push_refcounted");                       // append retained candidate to the result array
    emitter.instruction("str x0, [sp, #16]");                                   // persist result pointer after possible growth

    emitter.label("__rt_array_isect_ref_skip");
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload outer loop index
    emitter.instruction("add x4, x4, #1");                                      // increment outer loop index
    emitter.instruction("str x4, [sp, #24]");                                   // persist updated outer loop index
    emitter.instruction("b __rt_array_isect_ref_outer");                        // continue scanning arr1

    emitter.label("__rt_array_isect_ref_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload result array pointer
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return result array
}

fn emit_array_intersect_refcounted_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_intersect_refcounted ---");
    emitter.label_global("__rt_array_intersect_refcounted");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving refcounted array-intersect spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for both inputs, the result array, and the outer loop index
    emitter.instruction("sub rsp, 32");                                         // reserve aligned spill slots for the refcounted array-intersect bookkeeping
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the first input indexed-array pointer across constructor and append helper calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the second input indexed-array pointer across constructor and append helper calls
    emitter.instruction("mov rdi, QWORD PTR [rdi + 8]");                        // pass the first input indexed-array capacity as the result indexed-array capacity to the shared constructor
    emitter.instruction("mov rsi, 8");                                          // request 8-byte payload slots for the refcounted result indexed array
    emitter.instruction("call __rt_array_new");                                 // allocate the refcounted result indexed array through the shared x86_64 constructor
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the result indexed-array pointer across the refcounted append helper calls
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // initialize the outer loop index to the first payload slot of the first input indexed array

    emitter.label("__rt_array_isect_ref_outer_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the outer loop index before reading the next candidate refcounted payload from the first input indexed array
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the first input indexed-array pointer before reading its logical length and current payload
    emitter.instruction("cmp rcx, QWORD PTR [r10]");                            // compare the outer loop index against the first input indexed-array logical length
    emitter.instruction("jge __rt_array_isect_ref_done_x86");                   // finish once every payload slot from the first input indexed array has been examined
    emitter.instruction("lea r11, [r10 + 24]");                                 // compute the payload base address for the first input indexed array
    emitter.instruction("mov r8, QWORD PTR [r11 + rcx * 8]");                   // load the current candidate refcounted payload from the first input indexed array
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the second input indexed-array pointer before scanning it for the candidate payload
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the second input indexed-array logical length before scanning it for the candidate payload
    emitter.instruction("lea r10, [r10 + 24]");                                 // compute the payload base address for the second input indexed array
    emitter.instruction("xor r9, r9");                                          // initialize the inner scan index to the first payload slot of the second input indexed array

    emitter.label("__rt_array_isect_ref_inner_x86");
    emitter.instruction("cmp r9, r11");                                         // compare the inner scan index against the second input indexed-array logical length
    emitter.instruction("jge __rt_array_isect_ref_skip_x86");                   // skip appending once the full second input indexed array has been scanned without finding a match
    emitter.instruction("cmp r8, QWORD PTR [r10 + r9 * 8]");                    // compare the candidate refcounted payload pointer against the current payload from the second input indexed array
    emitter.instruction("je __rt_array_isect_ref_match_x86");                   // append the candidate payload once it is found in the second input indexed array
    emitter.instruction("add r9, 1");                                           // advance the inner scan index after checking one payload slot in the second input indexed array
    emitter.instruction("jmp __rt_array_isect_ref_inner_x86");                  // continue scanning the second input indexed array for the current candidate payload

    emitter.label("__rt_array_isect_ref_match_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the result indexed-array pointer before appending the matching candidate payload
    emitter.instruction("mov rsi, r8");                                         // place the matching candidate refcounted payload in the second x86_64 append helper argument register
    emitter.instruction("call __rt_array_push_refcounted");                     // append the retained matching candidate payload into the result indexed array
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // persist the possibly-grown result indexed-array pointer after the append helper returns

    emitter.label("__rt_array_isect_ref_skip_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the outer loop index after helper calls clobbered caller-saved registers
    emitter.instruction("add rcx, 1");                                          // advance the outer loop index to examine the next payload from the first input indexed array
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // persist the updated outer loop index across the next iteration
    emitter.instruction("jmp __rt_array_isect_ref_outer_x86");                  // continue examining refcounted payloads from the first input indexed array

    emitter.label("__rt_array_isect_ref_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the result indexed-array pointer in the standard x86_64 integer result register
    emitter.instruction("add rsp, 32");                                         // release the refcounted array-intersect spill slots before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to the caller
    emitter.instruction("ret");                                                 // return the refcounted array-intersect result indexed-array pointer in rax
}
