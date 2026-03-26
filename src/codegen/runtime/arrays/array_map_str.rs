use crate::codegen::emit::Emitter;

/// array_map_str: apply a callback to each element of an array, returning a new string array.
/// Handles both int and string source arrays (detects elem_size from header).
/// Input: x0 = callback function address, x1 = source array pointer
/// Output: x0 = pointer to new array with string elements (elem_size=16)
pub fn emit_array_map_str(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_map_str ---");
    emitter.label("__rt_array_map_str");

    // -- set up stack frame, save callee-saved registers --
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set up new frame pointer
    emitter.instruction("stp x19, x20, [sp, #32]");                             // save callee-saved x19, x20
    emitter.instruction("str x0, [sp, #0]");                                    // save callback address to stack
    emitter.instruction("str x1, [sp, #8]");                                    // save source array pointer to stack
    emitter.instruction("mov x19, x0");                                         // x19 = callback address (callee-saved)

    // -- read source array metadata --
    emitter.instruction("ldr x9, [x1]");                                        // x9 = source array length
    emitter.instruction("str x9, [sp, #16]");                                   // save length to stack
    emitter.instruction("ldr x10, [x1, #16]");                                  // x10 = source elem_size (8=int, 16=str)
    emitter.instruction("str x10, [sp, #24]");                                  // save source elem_size to stack

    // -- create new result array with elem_size=16 (string output) --
    emitter.instruction("mov x0, x9");                                          // x0 = capacity for new array
    emitter.instruction("mov x1, #16");                                         // x1 = element size (16 bytes for string)
    emitter.instruction("bl __rt_array_new");                                   // allocate new array → x0
    emitter.instruction("mov x20, x0");                                         // x20 = new array pointer (callee-saved)

    // -- set up loop counter --
    emitter.instruction("mov x0, #0");                                          // x0 = loop index i = 0
    emitter.instruction("str x0, [sp, #0]");                                    // reuse sp+0 for loop index (callback addr in x19)

    // -- loop: apply callback to each element --
    emitter.label("__rt_array_map_str_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // load loop index
    emitter.instruction("ldr x9, [sp, #16]");                                   // load source length
    emitter.instruction("cmp x0, x9");                                          // compare i with length
    emitter.instruction("b.ge __rt_array_map_str_done");                        // if i >= length, loop complete

    // -- load element from source array based on elem_size --
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload source array pointer
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload source elem_size
    emitter.instruction("add x1, x1, #24");                                     // skip header to data region
    emitter.instruction("mul x11, x0, x10");                                    // x11 = i * elem_size
    emitter.instruction("add x11, x1, x11");                                    // x11 = &source_data[i]

    emitter.instruction("cmp x10, #16");                                        // is source a string array?
    emitter.instruction("b.eq __rt_array_map_str_load_str");                    // yes — load ptr+len

    // -- int source: pass element in x0 (first int param) --
    emitter.instruction("ldr x0, [x11]");                                       // x0 = int element
    emitter.instruction("b __rt_array_map_str_call");                           // proceed to call

    // -- string source: pass element in x0/x1 (first string param = 2 int regs) --
    emitter.label("__rt_array_map_str_load_str");
    emitter.instruction("ldr x0, [x11]");                                       // x0 = string pointer (first half)
    emitter.instruction("ldr x1, [x11, #8]");                                   // x1 = string length (second half)

    // -- call callback --
    emitter.label("__rt_array_map_str_call");
    emitter.instruction("blr x19");                                             // call callback → string result in x1=ptr, x2=len

    // -- persist string result to heap --
    emitter.instruction("bl __rt_str_persist");                                 // copy string to heap, x1=heap_ptr, x2=len

    // -- store string result in new array --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload loop index
    emitter.instruction("add x9, x20, #24");                                    // new array data region
    emitter.instruction("lsl x10, x0, #4");                                     // x10 = i * 16 (string stride)
    emitter.instruction("str x1, [x9, x10]");                                   // store string pointer
    emitter.instruction("add x10, x10, #8");                                    // advance to length slot
    emitter.instruction("str x2, [x9, x10]");                                   // store string length

    // -- advance loop --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload loop index
    emitter.instruction("add x0, x0, #1");                                      // i += 1
    emitter.instruction("str x0, [sp, #0]");                                    // save updated index
    emitter.instruction("b __rt_array_map_str_loop");                           // continue loop

    // -- set length on new array and return --
    emitter.label("__rt_array_map_str_done");
    emitter.instruction("mov x0, x20");                                         // x0 = new array pointer
    emitter.instruction("ldr x9, [sp, #16]");                                   // x9 = length
    emitter.instruction("str x9, [x0]");                                        // set new array length

    // -- tear down stack frame and return --
    emitter.instruction("ldp x19, x20, [sp, #32]");                             // restore callee-saved x19, x20
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = new mapped string array
}
