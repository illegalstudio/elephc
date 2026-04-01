use crate::codegen::emit::Emitter;

/// usort: sort an integer array in-place using a user-defined comparison callback.
/// Input: x0 = callback function address, x1 = array pointer
/// Output: none (sorts in place)
/// The callback receives (a, b) and returns negative/zero/positive for ordering.
/// Uses bubble sort for simplicity.
pub fn emit_usort(emitter: &mut Emitter) {
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
