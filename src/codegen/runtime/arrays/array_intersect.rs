use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// array_intersect: return elements present in both arr1 and arr2 (int arrays).
/// Input:  x0=arr1, x1=arr2
/// Output: x0=new array containing elements found in both arrays
pub fn emit_array_intersect(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_intersect_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_intersect ---");
    emitter.label_global("__rt_array_intersect");

    // -- set up stack frame, save arguments --
    // Stack layout:
    //   [sp, #0]  = arr1 pointer
    //   [sp, #8]  = arr2 pointer
    //   [sp, #16] = result array pointer
    //   [sp, #24] = outer loop index i
    //   [sp, #32] = saved x29
    //   [sp, #40] = saved x30
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save arr1 pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save arr2 pointer

    // -- create result array with same capacity as arr1 --
    emitter.instruction("ldr x0, [x0, #8]");                                    // x0 = arr1 capacity
    emitter.instruction("mov x1, #8");                                          // x1 = elem_size (8 bytes per int)
    emitter.instruction("bl __rt_array_new");                                   // allocate result array, x0 = result ptr
    emitter.instruction("str x0, [sp, #16]");                                   // save result array pointer

    // -- outer loop: iterate over each element in arr1 --
    emitter.instruction("str xzr, [sp, #24]");                                  // i = 0

    emitter.label("__rt_array_isect_outer");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload arr1 pointer
    emitter.instruction("ldr x3, [x0]");                                        // x3 = arr1 length
    emitter.instruction("ldr x4, [sp, #24]");                                   // x4 = i
    emitter.instruction("cmp x4, x3");                                          // compare i with arr1 length
    emitter.instruction("b.ge __rt_array_isect_done");                          // if i >= length, we're done

    // -- load arr1[i] --
    emitter.instruction("add x5, x0, #24");                                     // x5 = arr1 data base
    emitter.instruction("ldr x6, [x5, x4, lsl #3]");                            // x6 = arr1[i]

    // -- inner loop: scan arr2 for this element --
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload arr2 pointer
    emitter.instruction("ldr x7, [x1]");                                        // x7 = arr2 length
    emitter.instruction("add x8, x1, #24");                                     // x8 = arr2 data base
    emitter.instruction("mov x9, #0");                                          // j = 0

    emitter.label("__rt_array_isect_inner");
    emitter.instruction("cmp x9, x7");                                          // compare j with arr2 length
    emitter.instruction("b.ge __rt_array_isect_skip");                          // if j >= length, element not in arr2

    emitter.instruction("ldr x10, [x8, x9, lsl #3]");                           // x10 = arr2[j]
    emitter.instruction("cmp x6, x10");                                         // compare arr1[i] with arr2[j]
    emitter.instruction("b.eq __rt_array_isect_match");                         // if equal, element is in both arrays

    emitter.instruction("add x9, x9, #1");                                      // j += 1
    emitter.instruction("b __rt_array_isect_inner");                            // continue scanning arr2

    // -- element found in arr2: add to result --
    emitter.label("__rt_array_isect_match");
    emitter.instruction("ldr x0, [sp, #16]");                                   // x0 = result array pointer
    emitter.instruction("mov x1, x6");                                          // x1 = value to push
    emitter.instruction("bl __rt_array_push_int");                              // push element to result array

    // -- not found or already pushed: advance outer loop --
    emitter.label("__rt_array_isect_skip");
    emitter.instruction("ldr x4, [sp, #24]");                                   // reload i
    emitter.instruction("add x4, x4, #1");                                      // i += 1
    emitter.instruction("str x4, [sp, #24]");                                   // save updated i
    emitter.instruction("b __rt_array_isect_outer");                            // continue outer loop

    // -- return result array --
    emitter.label("__rt_array_isect_done");
    emitter.instruction("ldr x0, [sp, #16]");                                   // x0 = result array pointer
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = result array
}

fn emit_array_intersect_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_intersect ---");
    emitter.label_global("__rt_array_intersect");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving array-intersect spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for both inputs, the result array, and the outer loop index
    emitter.instruction("sub rsp, 32");                                         // reserve aligned spill slots for the integer array-intersect bookkeeping
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the first input indexed-array pointer across nested constructor and append helper calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // preserve the second input indexed-array pointer across nested constructor and append helper calls
    emitter.instruction("mov rdi, QWORD PTR [rdi + 8]");                        // pass the first input indexed-array capacity as the result indexed-array capacity to the constructor
    emitter.instruction("mov rsi, 8");                                          // use 8-byte payload slots because this helper currently computes integer array intersections
    emitter.instruction("call __rt_array_new");                                 // allocate the destination indexed array through the shared x86_64 constructor
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the destination indexed-array pointer across the nested append helper calls
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // initialize the outer loop index to the first payload slot of the first input indexed array
    emitter.label("__rt_array_isect_outer_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the outer loop index before reading the next candidate payload from the first input indexed array
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the first input indexed-array pointer before reading the loop bound and candidate payload
    emitter.instruction("cmp rcx, QWORD PTR [r10]");                            // compare the outer loop index against the first input indexed-array logical length
    emitter.instruction("jge __rt_array_isect_done_x86");                       // finish once every payload slot from the first input indexed array has been examined
    emitter.instruction("lea r11, [r10 + 24]");                                 // compute the payload base address for the first input indexed array
    emitter.instruction("mov r8, QWORD PTR [r11 + rcx * 8]");                   // load the current candidate integer payload from the first input indexed array
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the second input indexed-array pointer before scanning it for the candidate payload
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the second input indexed-array logical length before scanning it for the candidate payload
    emitter.instruction("lea r10, [r10 + 24]");                                 // compute the payload base address for the second input indexed array
    emitter.instruction("xor r9, r9");                                          // initialize the inner scan index to the first payload slot of the second input indexed array
    emitter.label("__rt_array_isect_inner_x86");
    emitter.instruction("cmp r9, r11");                                         // compare the inner scan index against the second input indexed-array logical length
    emitter.instruction("jge __rt_array_isect_skip_x86");                       // skip appending once the full second input indexed array has been scanned without finding a match
    emitter.instruction("cmp r8, QWORD PTR [r10 + r9 * 8]");                    // compare the candidate integer payload against the current payload from the second input indexed array
    emitter.instruction("je __rt_array_isect_match_x86");                       // append the candidate payload once it is found in the second input indexed array
    emitter.instruction("add r9, 1");                                           // advance the inner scan index after checking one payload slot in the second input indexed array
    emitter.instruction("jmp __rt_array_isect_inner_x86");                      // continue scanning the second input indexed array for the current candidate payload
    emitter.label("__rt_array_isect_match_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the destination indexed-array pointer before appending the matching candidate payload
    emitter.instruction("mov rsi, r8");                                         // place the matching candidate integer payload in the second x86_64 append helper argument register
    emitter.instruction("call __rt_array_push_int");                            // append the matching candidate integer payload into the destination indexed array
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // persist the possibly-grown destination indexed-array pointer after appending a matching payload
    emitter.label("__rt_array_isect_skip_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the outer loop index after helper calls clobbered caller-saved registers
    emitter.instruction("add rcx, 1");                                          // advance the outer loop index to examine the next payload from the first input indexed array
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // persist the updated outer loop index across the next iteration
    emitter.instruction("jmp __rt_array_isect_outer_x86");                      // continue examining payloads from the first input indexed array
    emitter.label("__rt_array_isect_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the destination indexed-array pointer in the standard x86_64 integer result register
    emitter.instruction("add rsp, 32");                                         // release the integer array-intersect spill slots before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                 // return the destination indexed-array pointer in rax
}
