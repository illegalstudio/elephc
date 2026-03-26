use crate::codegen::emit::Emitter;

/// array_map_str: apply a callback to each element of an integer array, returning a new string array.
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

    // -- read source array length and create new array with elem_size=16 --
    emitter.instruction("ldr x9, [x1]");                                        // x9 = source array length
    emitter.instruction("str x9, [sp, #16]");                                   // save length to stack
    emitter.instruction("mov x0, x9");                                          // x0 = capacity for new array
    emitter.instruction("mov x1, #16");                                         // x1 = element size (16 bytes for string ptr+len)
    emitter.instruction("bl __rt_array_new");                                   // allocate new array → x0=new array ptr
    emitter.instruction("str x0, [sp, #24]");                                   // save new array pointer to stack

    // -- set up loop counter --
    emitter.instruction("mov x20, #0");                                         // x20 = loop index i = 0

    // -- loop: apply callback to each element --
    emitter.label("__rt_array_map_str_loop");
    emitter.instruction("ldr x9, [sp, #16]");                                   // load source length
    emitter.instruction("cmp x20, x9");                                         // compare i with length
    emitter.instruction("b.ge __rt_array_map_str_done");                        // if i >= length, loop complete

    // -- load element from source array --
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload source array pointer
    emitter.instruction("add x1, x1, #24");                                     // skip header to data region
    emitter.instruction("ldr x0, [x1, x20, lsl #3]");                           // x0 = source[i]

    // -- call callback with element as argument --
    emitter.instruction("blr x19");                                             // call callback(element) → string result in x1=ptr, x2=len

    // -- store string result (x1=ptr, x2=len) in new array --
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload new array pointer
    emitter.instruction("add x9, x9, #24");                                     // skip header to data region
    emitter.instruction("lsl x10, x20, #4");                                    // x10 = i * 16 (string element stride)
    emitter.instruction("str x1, [x9, x10]");                                   // store string pointer at new_array[i].ptr
    emitter.instruction("add x10, x10, #8");                                    // advance to length slot
    emitter.instruction("str x2, [x9, x10]");                                   // store string length at new_array[i].len

    // -- advance loop --
    emitter.instruction("add x20, x20, #1");                                    // i += 1
    emitter.instruction("b __rt_array_map_str_loop");                           // continue loop

    // -- set length on new array and return --
    emitter.label("__rt_array_map_str_done");
    emitter.instruction("ldr x0, [sp, #24]");                                   // x0 = new array pointer
    emitter.instruction("ldr x9, [sp, #16]");                                   // x9 = length
    emitter.instruction("str x9, [x0]");                                        // set new array length

    // -- tear down stack frame and return --
    emitter.instruction("ldp x19, x20, [sp, #32]");                             // restore callee-saved x19, x20
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return with x0 = new mapped string array
}
