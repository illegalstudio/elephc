use crate::codegen::emit::Emitter;

/// itoa: convert signed 64-bit integer to decimal string.
/// Input:  x0 = integer value
/// Output: x1 = pointer to string, x2 = length
pub fn emit_itoa(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: itoa ---");
    emitter.label("__rt_itoa");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #16");                                     // allocate 16 bytes on the stack
    emitter.instruction("stp x29, x30, [sp]");                                  // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish new frame pointer

    // -- get concat_buf write position --
    emitter.instruction("adrp x6, _concat_off@PAGE");                           // load page address of concat buffer offset
    emitter.instruction("add x6, x6, _concat_off@PAGEOFF");                     // add page offset to get exact address
    emitter.instruction("ldr x8, [x6]");                                        // load current offset into concat_buf
    emitter.instruction("adrp x7, _concat_buf@PAGE");                           // load page address of concat buffer
    emitter.instruction("add x7, x7, _concat_buf@PAGEOFF");                     // add page offset to get buffer base
    emitter.instruction("add x9, x7, x8");                                      // compute write position: buf + offset
    emitter.instruction("add x9, x9, #20");                                     // advance to end of 21-byte scratch area (digits written right-to-left)

    // -- initialize counters --
    emitter.instruction("mov x10, #0");                                         // digit count = 0
    emitter.instruction("mov x11, #0");                                         // negative flag = 0 (not negative)

    // -- handle sign --
    emitter.instruction("cmp x0, #0");                                          // check if input is negative
    emitter.instruction("b.ge __rt_itoa_positive");                             // skip negation if >= 0
    emitter.instruction("mov x11, #1");                                         // set negative flag
    emitter.instruction("neg x0, x0");                                          // negate to make value positive

    // -- handle zero special case --
    emitter.label("__rt_itoa_positive");
    emitter.instruction("cbnz x0, __rt_itoa_loop");                             // if value != 0, start digit extraction loop
    emitter.instruction("mov w12, #48");                                        // ASCII '0'
    emitter.instruction("strb w12, [x9]");                                      // store '0' at current position
    emitter.instruction("sub x9, x9, #1");                                      // move write cursor left
    emitter.instruction("mov x10, #1");                                         // digit count = 1
    emitter.instruction("b __rt_itoa_done");                                    // skip to finalization

    // -- extract digits right-to-left via repeated division by 10 --
    emitter.label("__rt_itoa_loop");
    emitter.instruction("cbz x0, __rt_itoa_sign");                              // if quotient is 0, all digits extracted
    emitter.instruction("mov x12, #10");                                        // divisor = 10
    emitter.instruction("udiv x13, x0, x12");                                   // quotient = value / 10
    emitter.instruction("msub x14, x13, x12, x0");                              // remainder = value - (quotient * 10)
    emitter.instruction("add x14, x14, #48");                                   // convert remainder to ASCII digit
    emitter.instruction("strb w14, [x9]");                                      // store digit at current position
    emitter.instruction("sub x9, x9, #1");                                      // move write cursor left (right-to-left)
    emitter.instruction("add x10, x10, #1");                                    // increment digit count
    emitter.instruction("mov x0, x13");                                         // value = quotient for next iteration
    emitter.instruction("b __rt_itoa_loop");                                    // continue extracting digits

    // -- prepend minus sign if negative --
    emitter.label("__rt_itoa_sign");
    emitter.instruction("cbz x11, __rt_itoa_done");                             // skip if not negative
    emitter.instruction("mov w12, #45");                                        // ASCII '-'
    emitter.instruction("strb w12, [x9]");                                      // store minus sign
    emitter.instruction("sub x9, x9, #1");                                      // move cursor left past the sign
    emitter.instruction("add x10, x10, #1");                                    // count the sign in total length

    // -- finalize: update concat_buf offset and return ptr/len --
    emitter.label("__rt_itoa_done");
    emitter.instruction("add x8, x8, #21");                                     // advance concat_off by scratch area size
    emitter.instruction("str x8, [x6]");                                        // store updated offset back to _concat_off
    emitter.instruction("add x1, x9, #1");                                      // result ptr = one past last written position
    emitter.instruction("mov x2, x10");                                         // result length = digit count

    // -- restore frame and return --
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // deallocate stack frame
    emitter.instruction("ret");                                                 // return to caller
}
