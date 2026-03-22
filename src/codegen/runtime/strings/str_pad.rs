use crate::codegen::emit::Emitter;

/// str_pad: pad a string to a target length.
/// Input: x1/x2=input, x3/x4=pad_str, x5=target_len, x7=pad_type (0=left, 1=right, 2=both).
/// Output: x1/x2=result.
pub fn emit_str_pad(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_pad ---");
    emitter.label("__rt_str_pad");
    emitter.instruction("sub sp, sp, #64");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // set frame pointer
    emitter.instruction("stp x1, x2, [sp]");                                    // save input string
    emitter.instruction("stp x3, x4, [sp, #16]");                               // save pad string
    emitter.instruction("str x5, [sp, #32]");                                   // save target length
    emitter.instruction("str x7, [sp, #40]");                                   // save pad type

    // -- if input already >= target, return as-is --
    emitter.instruction("cmp x2, x5");                                          // compare input len with target
    emitter.instruction("b.ge __rt_str_pad_noop");                              // already long enough → return copy

    // -- set up concat_buf destination --
    emitter.instruction("adrp x9, _concat_off@PAGE");                           // load concat offset page
    emitter.instruction("add x9, x9, _concat_off@PAGEOFF");                     // resolve address
    emitter.instruction("ldr x10, [x9]");                                       // load current offset
    emitter.instruction("adrp x11, _concat_buf@PAGE");                          // load concat buffer page
    emitter.instruction("add x11, x11, _concat_buf@PAGEOFF");                   // resolve address
    emitter.instruction("add x12, x11, x10");                                   // destination pointer
    emitter.instruction("mov x13, x12");                                        // save result start

    emitter.instruction("sub x14, x5, x2");                                     // pad_needed = target - input_len
    emitter.instruction("ldr x7, [sp, #40]");                                   // reload pad_type

    // -- compute left_pad and right_pad amounts --
    emitter.instruction("cmp x7, #0");                                          // STR_PAD_LEFT?
    emitter.instruction("b.eq __rt_str_pad_left_all");                          // all padding on left
    emitter.instruction("cmp x7, #2");                                          // STR_PAD_BOTH?
    emitter.instruction("b.eq __rt_str_pad_both");                              // split padding
    // -- STR_PAD_RIGHT (default): all padding on right --
    emitter.instruction("mov x15, #0");                                         // left_pad = 0
    emitter.instruction("mov x16, x14");                                        // right_pad = all
    emitter.instruction("b __rt_str_pad_emit");                                 // start emitting

    emitter.label("__rt_str_pad_left_all");
    emitter.instruction("mov x15, x14");                                        // left_pad = all
    emitter.instruction("mov x16, #0");                                         // right_pad = 0
    emitter.instruction("b __rt_str_pad_emit");                                 // start emitting

    emitter.label("__rt_str_pad_both");
    emitter.instruction("lsr x15, x14, #1");                                    // left_pad = pad_needed / 2
    emitter.instruction("sub x16, x14, x15");                                   // right_pad = pad_needed - left_pad
    // fall through to emit

    // -- emit: left_pad chars, then input, then right_pad chars --
    emitter.label("__rt_str_pad_emit");
    // left padding
    emitter.instruction("mov x17, x15");                                        // left pad counter
    emitter.instruction("mov x18, #0");                                         // pad string index
    emitter.label("__rt_str_pad_lp");
    emitter.instruction("cbz x17, __rt_str_pad_input");                         // left padding done → copy input
    emitter.instruction("ldp x3, x4, [sp, #16]");                               // reload pad string
    emitter.instruction("ldrb w0, [x3, x18]");                                  // load pad char at index
    emitter.instruction("strb w0, [x12], #1");                                  // write to output
    emitter.instruction("sub x17, x17, #1");                                    // decrement left pad remaining
    emitter.instruction("add x18, x18, #1");                                    // advance pad index
    emitter.instruction("cmp x18, x4");                                         // wrap around if past pad string
    emitter.instruction("csel x18, xzr, x18, ge");                              // reset to 0 if >= pad_len
    emitter.instruction("b __rt_str_pad_lp");                                   // continue

    // copy input
    emitter.label("__rt_str_pad_input");
    emitter.instruction("ldp x1, x2, [sp]");                                    // reload input string
    emitter.instruction("mov x17, x2");                                         // input copy counter
    emitter.label("__rt_str_pad_inp_loop");
    emitter.instruction("cbz x17, __rt_str_pad_rp");                            // input done → right padding
    emitter.instruction("ldrb w0, [x1], #1");                                   // load input byte
    emitter.instruction("strb w0, [x12], #1");                                  // write to output
    emitter.instruction("sub x17, x17, #1");                                    // decrement
    emitter.instruction("b __rt_str_pad_inp_loop");                             // continue

    // right padding
    emitter.label("__rt_str_pad_rp");
    emitter.instruction("mov x17, x16");                                        // right pad counter
    emitter.instruction("mov x18, #0");                                         // pad string index
    emitter.label("__rt_str_pad_rp_loop");
    emitter.instruction("cbz x17, __rt_str_pad_done");                          // right padding done
    emitter.instruction("ldp x3, x4, [sp, #16]");                               // reload pad string
    emitter.instruction("ldrb w0, [x3, x18]");                                  // load pad char
    emitter.instruction("strb w0, [x12], #1");                                  // write to output
    emitter.instruction("sub x17, x17, #1");                                    // decrement
    emitter.instruction("add x18, x18, #1");                                    // advance pad index
    emitter.instruction("cmp x18, x4");                                         // wrap around
    emitter.instruction("csel x18, xzr, x18, ge");                              // reset to 0
    emitter.instruction("b __rt_str_pad_rp_loop");                              // continue

    emitter.label("__rt_str_pad_done");
    emitter.instruction("mov x1, x13");                                         // result pointer
    emitter.instruction("sub x2, x12, x13");                                    // result length
    emitter.instruction("adrp x9, _concat_off@PAGE");                           // update concat offset
    emitter.instruction("add x9, x9, _concat_off@PAGEOFF");                     // resolve address
    emitter.instruction("ldr x10, [x9]");                                       // load current offset
    emitter.instruction("add x10, x10, x2");                                    // advance by result length
    emitter.instruction("str x10, [x9]");                                       // store updated offset
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame
    emitter.instruction("add sp, sp, #64");                                     // deallocate
    emitter.instruction("ret");                                                 // return

    emitter.label("__rt_str_pad_noop");
    emitter.instruction("bl __rt_strcopy");                                     // copy input as-is
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame
    emitter.instruction("add sp, sp, #64");                                     // deallocate
    emitter.instruction("ret");                                                 // return
}
