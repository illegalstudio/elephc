use crate::codegen::emit::Emitter;

/// array_filter: filter elements of an integer array using a callback, returning a new array.
/// Input: x0 = callback function address, x1 = source array pointer
/// Output: x0 = pointer to new array with only elements where callback returned truthy
pub fn emit_array_filter(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_filter ---");
    emitter.label("__rt_array_filter");

    // -- set up stack frame, save callee-saved registers --
    emitter.instruction("sub sp, sp, #80");                                     // allocate 80 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // set up new frame pointer
    emitter.instruction("stp x19, x20, [sp, #48]");                             // save callee-saved x19, x20
    emitter.instruction("str x21, [sp, #40]");                                  // save callee-saved x21
    emitter.instruction("str x0, [sp, #0]");                                    // save callback address to stack
    emitter.instruction("str x1, [sp, #8]");                                    // save source array pointer to stack
    emitter.instruction("mov x19, x0");                                         // x19 = callback address (callee-saved)

    // -- read source array length and create new array --
    emitter.instruction("ldr x9, [x1]");                                        // x9 = source array length
    emitter.instruction("str x9, [sp, #16]");                                   // save length to stack
    emitter.instruction("mov x0, x9");                                          // x0 = capacity for new array (same size max)
    emitter.instruction("mov x1, #8");                                          // x1 = element size (8 bytes for int)
    emitter.instruction("bl __rt_array_new");                                   // allocate new array → x0=new array ptr
    emitter.instruction("str x0, [sp, #24]");                                   // save new array pointer to stack

    // -- set up loop counters --
    emitter.instruction("mov x20, #0");                                         // x20 = source index i = 0
    emitter.instruction("mov x21, #0");                                         // x21 = dest index j = 0

    // -- loop: test each element with callback --
    emitter.label("__rt_array_filter_loop");
    emitter.instruction("ldr x9, [sp, #16]");                                   // load source length
    emitter.instruction("cmp x20, x9");                                         // compare i with length
    emitter.instruction("b.ge __rt_array_filter_done");                         // if i >= length, loop complete

    // -- load element from source array --
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload source array pointer
    emitter.instruction("add x1, x1, #24");                                     // skip header to data region
    emitter.instruction("ldr x0, [x1, x20, lsl #3]");                           // x0 = source[i]

    // -- save current element for potential copy --
    emitter.instruction("str x0, [sp, #32]");                                   // save element value to stack

    // -- call callback with element as argument --
    emitter.instruction("blr x19");                                             // call callback(element) → result in x0

    // -- check if callback returned truthy --
    emitter.instruction("cbz x0, __rt_array_filter_skip");                      // if callback returned 0, skip element

    // -- callback returned truthy: copy element to new array --
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload saved element value
    emitter.instruction("ldr x1, [sp, #24]");                                   // reload new array pointer
    emitter.instruction("add x2, x1, #24");                                     // skip header to data region
    emitter.instruction("str x9, [x2, x21, lsl #3]");                           // new_array[j] = element
    emitter.instruction("add x21, x21, #1");                                    // j += 1 (advance dest index)

    // -- advance source index --
    emitter.label("__rt_array_filter_skip");
    emitter.instruction("add x20, x20, #1");                                    // i += 1
    emitter.instruction("b __rt_array_filter_loop");                            // continue loop

    // -- set length on new array and return --
    emitter.label("__rt_array_filter_done");
    emitter.instruction("ldr x0, [sp, #24]");                                   // x0 = new array pointer
    emitter.instruction("str x21, [x0]");                                       // set new array length = number of kept elements

    // -- tear down stack frame and return --
    emitter.instruction("ldr x21, [sp, #40]");                                  // restore callee-saved x21
    emitter.instruction("ldp x19, x20, [sp, #48]");                             // restore callee-saved x19, x20
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = new filtered array
}
