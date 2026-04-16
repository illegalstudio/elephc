use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// usort: sort an integer array in-place using a user-defined comparison callback.
/// Input: x0 = callback function address, x1 = array pointer
/// Output: none (sorts in place)
/// The callback receives (a, b) and returns negative/zero/positive for ordering.
/// Uses bubble sort for simplicity.
pub fn emit_usort(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_usort_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: usort ---");
    emitter.label_global("__rt_usort");

    // -- set up stack frame, save callee-saved registers --
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up new frame pointer
    emitter.instruction("stp x19, x20, [sp, #32]");                             // save callee-saved x19, x20
    emitter.instruction("stp x21, x22, [sp, #16]");                             // save callee-saved x21, x22
    emitter.instruction("mov x19, x0");                                         // x19 = callback address (callee-saved)
    emitter.instruction("str x1, [sp, #0]");                                    // save array pointer to stack

    // -- read array length --
    emitter.instruction("ldr x20, [x1]");                                       // x20 = array length
    emitter.instruction("cmp x20, #2");                                         // check if array has fewer than 2 elements
    emitter.instruction("b.lt __rt_usort_done");                                // if length < 2, already sorted

    // -- outer loop: repeat until no swaps needed --
    emitter.label("__rt_usort_outer");
    emitter.instruction("mov x21, #0");                                         // x21 = swapped flag = 0 (no swaps yet)
    emitter.instruction("mov x22, #0");                                         // x22 = inner index j = 0

    // -- inner loop: compare adjacent pairs --
    emitter.label("__rt_usort_inner");
    emitter.instruction("sub x9, x20, #1");                                     // x9 = length - 1
    emitter.instruction("cmp x22, x9");                                         // compare j with length-1
    emitter.instruction("b.ge __rt_usort_check");                               // if j >= length-1, check if done

    // -- load adjacent elements --
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload array pointer
    emitter.instruction("add x9, x9, #24");                                     // skip header to data region
    emitter.instruction("ldr x0, [x9, x22, lsl #3]");                           // x0 = data[j] (first element)
    emitter.instruction("add x10, x22, #1");                                    // x10 = j + 1
    emitter.instruction("ldr x1, [x9, x10, lsl #3]");                           // x1 = data[j+1] (second element)

    // -- save data base pointer and element values for potential swap --
    emitter.instruction("str x9, [sp, #8]");                                    // save data base pointer

    // -- call comparator callback(a, b) --
    emitter.instruction("blr x19");                                             // call callback(data[j], data[j+1]) → x0=result

    // -- if result > 0, swap elements --
    emitter.instruction("cmp x0, #0");                                          // compare result with 0
    emitter.instruction("b.le __rt_usort_noswap");                              // if result <= 0, no swap needed

    // -- swap data[j] and data[j+1] --
    emitter.instruction("ldr x9, [sp, #8]");                                    // reload data base pointer
    emitter.instruction("ldr x10, [x9, x22, lsl #3]");                          // x10 = data[j]
    emitter.instruction("add x11, x22, #1");                                    // x11 = j + 1
    emitter.instruction("ldr x12, [x9, x11, lsl #3]");                          // x12 = data[j+1]
    emitter.instruction("str x12, [x9, x22, lsl #3]");                          // data[j] = data[j+1]
    emitter.instruction("str x10, [x9, x11, lsl #3]");                          // data[j+1] = data[j] (complete swap)
    emitter.instruction("mov x21, #1");                                         // set swapped flag = 1

    // -- advance inner loop --
    emitter.label("__rt_usort_noswap");
    emitter.instruction("add x22, x22, #1");                                    // j += 1
    emitter.instruction("b __rt_usort_inner");                                  // continue inner loop

    // -- check if any swaps occurred --
    emitter.label("__rt_usort_check");
    emitter.instruction("cbnz x21, __rt_usort_outer");                          // if swaps happened, repeat outer loop

    // -- done --
    emitter.label("__rt_usort_done");

    // -- tear down stack frame and return --
    emitter.instruction("ldp x21, x22, [sp, #16]");                             // restore callee-saved x21, x22
    emitter.instruction("ldp x19, x20, [sp, #32]");                             // restore callee-saved x19, x20
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return (void, array sorted in place)
}

fn emit_usort_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: usort ---");
    emitter.label_global("__rt_usort");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving local usort() state
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the callback pointer, array pointer, and loop state
    emitter.instruction("push rbx");                                            // preserve the inner-loop index register across comparator callback invocations
    emitter.instruction("push r12");                                            // preserve the comparator callback pointer across nested comparator calls
    emitter.instruction("push r13");                                            // preserve the indexed-array pointer across nested comparator calls
    emitter.instruction("push r14");                                            // preserve the indexed-array length across nested comparator calls
    emitter.instruction("push r15");                                            // preserve the swapped flag across nested comparator calls
    emitter.instruction("mov r12, rdi");                                        // preserve the comparator callback address in a callee-saved register for the whole bubble-sort pass
    emitter.instruction("mov r13, rsi");                                        // preserve the indexed-array pointer in a callee-saved register for the whole bubble-sort pass
    emitter.instruction("mov r14, QWORD PTR [r13]");                            // load the indexed-array logical length once before the bubble-sort passes begin
    emitter.instruction("cmp r14, 2");                                          // does the indexed array contain fewer than two elements?
    emitter.instruction("jl __rt_usort_done_linux_x86_64");                     // arrays of length zero or one are already sorted

    emitter.label("__rt_usort_outer_linux_x86_64");
    emitter.instruction("xor r15d, r15d");                                      // clear the swapped flag at the start of each bubble-sort outer pass
    emitter.instruction("xor ebx, ebx");                                        // restart the inner-loop cursor at element index zero for the next bubble-sort pass

    emitter.label("__rt_usort_inner_linux_x86_64");
    emitter.instruction("mov r10, r14");                                        // copy the indexed-array length before deriving the final comparable inner-loop index
    emitter.instruction("sub r10, 1");                                          // derive the final comparable inner-loop index as length - 1 for the adjacent-pair scan
    emitter.instruction("cmp rbx, r10");                                        // has the inner-loop cursor reached the final adjacent pair for this bubble-sort pass?
    emitter.instruction("jge __rt_usort_check_linux_x86_64");                   // finish the current outer pass once every adjacent pair has been compared
    emitter.instruction("lea r10, [r13 + 24]");                                 // point at the indexed-array payload region just after the fixed 24-byte header
    emitter.instruction("mov rdi, QWORD PTR [r10 + rbx * 8]");                  // load the left comparator argument from the current indexed-array slot
    emitter.instruction("lea r11, [rbx + 1]");                                  // derive the right adjacent slot index before loading the second comparator argument
    emitter.instruction("mov rsi, QWORD PTR [r10 + r11 * 8]");                  // load the right comparator argument from the adjacent indexed-array slot
    emitter.instruction("call r12");                                            // invoke the user comparator callback on the current adjacent indexed-array pair
    emitter.instruction("cmp rax, 0");                                          // did the comparator report that the current adjacent pair is already ordered?
    emitter.instruction("jle __rt_usort_noswap_linux_x86_64");                  // skip the swap path when the comparator says the left element should stay before the right element
    emitter.instruction("lea r10, [r13 + 24]");                                 // reload the indexed-array payload base after the comparator call clobbered caller-saved registers
    emitter.instruction("mov rdx, QWORD PTR [r10 + rbx * 8]");                  // reload the left indexed-array element so it can be swapped with its right neighbor
    emitter.instruction("lea r11, [rbx + 1]");                                  // recompute the right adjacent slot index for the swap write-back path
    emitter.instruction("mov rcx, QWORD PTR [r10 + r11 * 8]");                  // reload the right indexed-array element so the adjacent pair can be swapped in place
    emitter.instruction("mov QWORD PTR [r10 + rbx * 8], rcx");                  // write the right element into the left slot after the comparator requested a swap
    emitter.instruction("mov QWORD PTR [r10 + r11 * 8], rdx");                  // write the saved left element into the right slot to complete the in-place swap
    emitter.instruction("mov r15, 1");                                          // remember that this bubble-sort pass performed at least one swap so another pass is required

    emitter.label("__rt_usort_noswap_linux_x86_64");
    emitter.instruction("add rbx, 1");                                          // advance the inner-loop cursor to the next adjacent indexed-array pair
    emitter.instruction("jmp __rt_usort_inner_linux_x86_64");                   // continue scanning adjacent indexed-array pairs within the current bubble-sort pass

    emitter.label("__rt_usort_check_linux_x86_64");
    emitter.instruction("test r15, r15");                                       // did the current bubble-sort pass perform any swaps?
    emitter.instruction("jnz __rt_usort_outer_linux_x86_64");                   // repeat another bubble-sort pass while at least one adjacent pair was swapped

    emitter.label("__rt_usort_done_linux_x86_64");
    emitter.instruction("pop r15");                                             // restore the saved swapped-flag register after the x86_64 usort() helper finishes
    emitter.instruction("pop r14");                                             // restore the saved indexed-array length register after the x86_64 usort() helper finishes
    emitter.instruction("pop r13");                                             // restore the saved indexed-array pointer register after the x86_64 usort() helper finishes
    emitter.instruction("pop r12");                                             // restore the saved comparator callback register after the x86_64 usort() helper finishes
    emitter.instruction("pop rbx");                                             // restore the saved inner-loop index register after the x86_64 usort() helper finishes
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning from the x86_64 usort() helper
    emitter.instruction("ret");                                                 // return after sorting the indexed array in place through the comparator callback
}
