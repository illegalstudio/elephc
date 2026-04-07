use crate::codegen::emit::Emitter;

/// concat: concatenate two strings.
/// Input:  x1=left_ptr, x2=left_len, x3=right_ptr, x4=right_len
/// Output: x1=result_ptr, x2=result_len
pub fn emit_concat(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: concat ---");
    emitter.label_global("__rt_concat");

    // -- set up stack frame (64 bytes) --
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish new frame pointer

    // -- save input arguments to stack --
    emitter.instruction("stp x1, x2, [sp, #0]");                                // save left string ptr and length
    emitter.instruction("stp x3, x4, [sp, #16]");                               // save right string ptr and length
    emitter.instruction("add x5, x2, x4");                                      // compute total result length
    emitter.instruction("str x5, [sp, #32]");                                   // save total length on stack

    // -- get concat_buf write position --
    emitter.adrp("x6", "_concat_off");                           // load page address of concat buffer offset
    emitter.add_lo12("x6", "x6", "_concat_off");                     // resolve exact address of offset variable
    emitter.instruction("ldr x8, [x6]");                                        // load current write offset
    emitter.adrp("x7", "_concat_buf");                           // load page address of concat buffer
    emitter.add_lo12("x7", "x7", "_concat_buf");                     // resolve exact buffer base address
    emitter.instruction("add x9, x7, x8");                                      // compute destination pointer: buf + offset
    emitter.instruction("str x9, [sp, #40]");                                   // save result start pointer on stack

    // -- copy left string bytes --
    emitter.instruction("ldp x1, x2, [sp, #0]");                                // reload left ptr and length
    emitter.instruction("mov x10, x9");                                         // set dest cursor to start of output
    emitter.label("__rt_concat_cl");
    emitter.instruction("cbz x2, __rt_concat_cr_setup");                        // if no bytes left, move to right string
    emitter.instruction("ldrb w11, [x1], #1");                                  // load byte from left string, advance src
    emitter.instruction("strb w11, [x10], #1");                                 // store byte to dest, advance dest
    emitter.instruction("sub x2, x2, #1");                                      // decrement remaining left bytes
    emitter.instruction("b __rt_concat_cl");                                    // continue copying left string

    // -- copy right string bytes --
    emitter.label("__rt_concat_cr_setup");
    emitter.instruction("ldp x3, x4, [sp, #16]");                               // reload right ptr and length
    emitter.label("__rt_concat_cr");
    emitter.instruction("cbz x4, __rt_concat_done");                            // if no bytes left, concatenation complete
    emitter.instruction("ldrb w11, [x3], #1");                                  // load byte from right string, advance src
    emitter.instruction("strb w11, [x10], #1");                                 // store byte to dest, advance dest
    emitter.instruction("sub x4, x4, #1");                                      // decrement remaining right bytes
    emitter.instruction("b __rt_concat_cr");                                    // continue copying right string

    // -- update concat_buf offset and return result --
    emitter.label("__rt_concat_done");
    emitter.instruction("ldr x5, [sp, #32]");                                   // reload total result length
    emitter.adrp("x6", "_concat_off");                           // load page address of concat offset
    emitter.add_lo12("x6", "x6", "_concat_off");                     // resolve exact address
    emitter.instruction("ldr x8, [x6]");                                        // load current offset
    emitter.instruction("add x8, x8, x5");                                      // advance offset by total length written
    emitter.instruction("str x8, [x6]");                                        // store updated offset

    // -- set return values and restore frame --
    emitter.instruction("ldr x1, [sp, #40]");                                   // return result pointer (start of output)
    emitter.instruction("ldr x2, [sp, #32]");                                   // return result length
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
