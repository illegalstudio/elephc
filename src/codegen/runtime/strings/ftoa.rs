use crate::codegen::emit::Emitter;

/// ftoa: convert double-precision float to string.
/// Input:  d0 = float value
/// Output: x1 = pointer to string, x2 = length
/// Uses _snprintf with "%.14G" format.
/// On Apple ARM64 variadic ABI, the double goes on the stack.
pub fn emit_ftoa(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ftoa ---");
    emitter.label("__rt_ftoa");

    // -- set up stack frame (64 bytes) --
    emitter.instruction("sub sp, sp, #64");                                     // allocate 64 bytes on the stack
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish new frame pointer

    // -- get current concat_buf position --
    emitter.instruction("adrp x9, _concat_off@PAGE");                           // load page address of concat buffer offset
    emitter.instruction("add x9, x9, _concat_off@PAGEOFF");                     // resolve exact address of offset variable
    emitter.instruction("ldr x10, [x9]");                                       // load current write offset
    emitter.instruction("str x10, [sp, #32]");                                  // save original offset on stack
    emitter.instruction("str x9, [sp, #40]");                                   // save offset variable address on stack

    emitter.instruction("adrp x11, _concat_buf@PAGE");                          // load page address of concat buffer
    emitter.instruction("add x11, x11, _concat_buf@PAGEOFF");                   // resolve exact buffer base address
    emitter.instruction("add x0, x11, x10");                                    // compute output buffer: concat_buf + offset
    emitter.instruction("str x0, [sp, #24]");                                   // save output buffer start on stack

    // -- call snprintf(buf, 32, "%.14G", double) --
    emitter.instruction("mov x1, #32");                                         // buffer size limit = 32 bytes
    emitter.instruction("adrp x2, _fmt_g@PAGE");                                // load page address of format string "%.14G"
    emitter.instruction("add x2, x2, _fmt_g@PAGEOFF");                          // resolve exact address of format string
    // -- Apple ARM64 variadic ABI: float arg goes on stack, not in SIMD reg --
    emitter.instruction("str d0, [sp]");                                        // push double onto stack for variadic call
    emitter.instruction("bl _snprintf");                                        // call snprintf; returns char count in x0

    // -- x0 = number of chars written --
    emitter.instruction("mov x2, x0");                                          // save string length as return value

    // -- update concat_off by chars written --
    emitter.instruction("ldr x9, [sp, #40]");                                   // reload offset variable address
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload original offset
    emitter.instruction("add x10, x10, x2");                                    // new offset = original + chars written
    emitter.instruction("str x10, [x9]");                                       // store updated offset

    // -- set return pointer --
    emitter.instruction("ldr x1, [sp, #24]");                                   // return pointer to start of formatted string

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
