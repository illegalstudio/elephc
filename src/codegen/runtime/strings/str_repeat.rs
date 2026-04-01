use crate::codegen::emit::Emitter;

/// str_repeat: repeat a string N times into concat_buf.
/// Input: x1=ptr, x2=len, x3=times
/// Output: x1=result_ptr, x2=result_len
pub fn emit_str_repeat(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_repeat ---");
    emitter.label_global("__rt_str_repeat");

    // -- set up stack frame (48 bytes) --
    emitter.instruction("sub sp, sp, #48");                                     // allocate 48 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish new frame pointer
    emitter.instruction("stp x1, x2, [sp]");                                    // save source pointer and length
    emitter.instruction("str x3, [sp, #16]");                                   // save repetition count

    // -- get concat_buf write position --
    emitter.instruction("adrp x6, _concat_off@PAGE");                           // load page address of concat buffer offset
    emitter.instruction("add x6, x6, _concat_off@PAGEOFF");                     // resolve exact address of offset variable
    emitter.instruction("ldr x8, [x6]");                                        // load current write offset
    emitter.instruction("adrp x7, _concat_buf@PAGE");                           // load page address of concat buffer
    emitter.instruction("add x7, x7, _concat_buf@PAGEOFF");                     // resolve exact buffer base address
    emitter.instruction("add x9, x7, x8");                                      // compute destination pointer
    emitter.instruction("str x9, [sp, #24]");                                   // save result start pointer

    // -- outer loop: repeat N times --
    emitter.instruction("mov x10, x3");                                         // initialize repetition counter
    emitter.label("__rt_str_repeat_loop");
    emitter.instruction("cbz x10, __rt_str_repeat_done");                       // if counter is 0, done repeating
    emitter.instruction("ldp x1, x2, [sp]");                                    // reload source pointer and length
    emitter.instruction("mov x11, x2");                                         // copy length as inner loop counter

    // -- inner loop: copy one instance of the string --
    emitter.label("__rt_str_repeat_copy");
    emitter.instruction("cbz x11, __rt_str_repeat_next");                       // if no bytes remain, move to next repetition
    emitter.instruction("ldrb w12, [x1], #1");                                  // load byte from source, advance src ptr
    emitter.instruction("strb w12, [x9], #1");                                  // store byte to dest, advance dest ptr
    emitter.instruction("sub x11, x11, #1");                                    // decrement inner byte counter
    emitter.instruction("b __rt_str_repeat_copy");                              // continue copying bytes
    emitter.label("__rt_str_repeat_next");
    emitter.instruction("sub x10, x10, #1");                                    // decrement repetition counter
    emitter.instruction("b __rt_str_repeat_loop");                              // continue to next repetition

    // -- finalize: compute result length and update concat_off --
    emitter.label("__rt_str_repeat_done");
    emitter.instruction("ldr x1, [sp, #24]");                                   // load result start pointer
    emitter.instruction("sub x2, x9, x1");                                      // result length = dest_end - dest_start
    emitter.instruction("ldr x8, [x6]");                                        // reload current concat_off
    emitter.instruction("add x8, x8, x2");                                      // advance offset by result length
    emitter.instruction("str x8, [x6]");                                        // store updated concat_off

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
