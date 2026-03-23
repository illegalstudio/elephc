use crate::codegen::emit::Emitter;

/// sprintf: format a string with typed arguments from the stack.
/// Input: x0=arg_count, x1=fmt_ptr, x2=fmt_len, args on stack (16 bytes each)
/// Output: x1=result_ptr, x2=result_len
/// Each stack arg is: [value, type_tag] where type_tag: 0=int, 1=str(len<<8), 2=float, 3=bool
/// The runtime pops arg_count*16 bytes from the caller's stack after processing.
pub fn emit_sprintf(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: sprintf ---");
    emitter.label("__rt_sprintf");
    emitter.instruction("sub sp, sp, #80");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #64]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // set frame pointer

    // -- save inputs --
    emitter.instruction("stp x1, x2, [sp]");                                   // save format ptr/len
    emitter.instruction("str x0, [sp, #16]");                                   // save arg count
    emitter.instruction("mov x13, #0");                                         // current arg index
    emitter.instruction("str x13, [sp, #24]");                                  // save arg index

    // -- compute args base pointer (past our frame) --
    // Args start at sp+80 (our frame) in the caller's stack
    emitter.instruction("add x14, sp, #80");                                    // args base pointer
    emitter.instruction("str x14, [sp, #32]");                                  // save args base

    // -- set up concat_buf destination --
    emitter.instruction("adrp x6, _concat_off@PAGE");                          // load concat offset page
    emitter.instruction("add x6, x6, _concat_off@PAGEOFF");                    // resolve address
    emitter.instruction("ldr x8, [x6]");                                        // load current offset
    emitter.instruction("adrp x7, _concat_buf@PAGE");                          // load concat buffer page
    emitter.instruction("add x7, x7, _concat_buf@PAGEOFF");                    // resolve address
    emitter.instruction("add x9, x7, x8");                                      // destination pointer
    emitter.instruction("str x9, [sp, #40]");                                   // save result start
    emitter.instruction("str x6, [sp, #48]");                                   // save concat_off ptr

    // -- scan format string --
    emitter.instruction("ldp x1, x2, [sp]");                                   // reload fmt ptr/len

    emitter.label("__rt_sprintf_loop");
    emitter.instruction("cbz x2, __rt_sprintf_done");                           // no format chars left → done
    emitter.instruction("ldrb w12, [x1], #1");                                  // load format char, advance
    emitter.instruction("sub x2, x2, #1");                                      // decrement remaining
    emitter.instruction("cmp w12, #37");                                        // is it '%'?
    emitter.instruction("b.eq __rt_sprintf_fmt");                               // yes → process format specifier
    // -- literal char: copy to output --
    emitter.instruction("strb w12, [x9], #1");                                  // copy literal char
    emitter.instruction("b __rt_sprintf_loop");                                 // next char

    // -- process format specifier --
    emitter.label("__rt_sprintf_fmt");
    emitter.instruction("cbz x2, __rt_sprintf_done");                           // no char after % → done
    emitter.instruction("ldrb w12, [x1], #1");                                  // load specifier char
    emitter.instruction("sub x2, x2, #1");                                      // decrement remaining

    // -- %% → literal % --
    emitter.instruction("cmp w12, #37");                                        // is it '%'?
    emitter.instruction("b.ne __rt_sprintf_check_d");                           // no → check %d
    emitter.instruction("strb w12, [x9], #1");                                  // write literal '%'
    emitter.instruction("b __rt_sprintf_loop");                                 // next

    // -- %d → format int as decimal --
    emitter.label("__rt_sprintf_check_d");
    emitter.instruction("cmp w12, #100");                                       // is it 'd'?
    emitter.instruction("b.ne __rt_sprintf_check_s");                           // no → check %s
    // Save state, call itoa
    emitter.instruction("stp x1, x2, [sp]");                                   // save fmt state
    emitter.instruction("str x9, [sp, #56]");                                   // save dest ptr
    // -- load next arg (int) --
    emitter.instruction("ldr x13, [sp, #24]");                                  // load arg index
    emitter.instruction("ldr x14, [sp, #32]");                                  // load args base
    emitter.instruction("lsl x15, x13, #4");                                    // arg offset = index * 16
    emitter.instruction("add x15, x14, x15");                                   // arg address
    emitter.instruction("ldr x0, [x15]");                                       // load value
    emitter.instruction("add x13, x13, #1");                                    // increment arg index
    emitter.instruction("str x13, [sp, #24]");                                  // save arg index
    emitter.instruction("bl __rt_itoa");                                        // convert to string → x1=ptr, x2=len
    // Copy itoa result to output
    emitter.instruction("ldr x9, [sp, #56]");                                   // restore dest ptr
    emitter.label("__rt_sprintf_copy_d");
    emitter.instruction("cbz x2, __rt_sprintf_d_done");                         // no bytes left
    emitter.instruction("ldrb w15, [x1], #1");                                  // load itoa byte
    emitter.instruction("strb w15, [x9], #1");                                  // write to output
    emitter.instruction("sub x2, x2, #1");                                      // decrement
    emitter.instruction("b __rt_sprintf_copy_d");                               // continue
    emitter.label("__rt_sprintf_d_done");
    emitter.instruction("ldp x1, x2, [sp]");                                   // restore fmt state
    emitter.instruction("b __rt_sprintf_loop");                                 // next

    // -- %s → insert string --
    emitter.label("__rt_sprintf_check_s");
    emitter.instruction("cmp w12, #115");                                       // is it 's'?
    emitter.instruction("b.ne __rt_sprintf_check_f");                           // no → check %f
    emitter.instruction("stp x1, x2, [sp]");                                   // save fmt state
    // -- load next arg (string) --
    emitter.instruction("ldr x13, [sp, #24]");                                  // arg index
    emitter.instruction("ldr x14, [sp, #32]");                                  // args base
    emitter.instruction("lsl x15, x13, #4");                                    // offset
    emitter.instruction("add x15, x14, x15");                                   // arg address
    emitter.instruction("ldr x3, [x15]");                                       // string pointer
    emitter.instruction("ldr x4, [x15, #8]");                                   // tag|length
    emitter.instruction("lsr x4, x4, #8");                                      // extract length (shift right 8)
    emitter.instruction("add x13, x13, #1");                                    // increment arg index
    emitter.instruction("str x13, [sp, #24]");                                  // save arg index
    // Copy string to output
    emitter.label("__rt_sprintf_copy_s");
    emitter.instruction("cbz x4, __rt_sprintf_s_done");                         // no bytes left
    emitter.instruction("ldrb w15, [x3], #1");                                  // load string byte
    emitter.instruction("strb w15, [x9], #1");                                  // write to output
    emitter.instruction("sub x4, x4, #1");                                      // decrement
    emitter.instruction("b __rt_sprintf_copy_s");                               // continue
    emitter.label("__rt_sprintf_s_done");
    emitter.instruction("ldp x1, x2, [sp]");                                   // restore fmt state
    emitter.instruction("b __rt_sprintf_loop");                                 // next

    // -- %f → format float --
    emitter.label("__rt_sprintf_check_f");
    emitter.instruction("cmp w12, #102");                                       // is it 'f'?
    emitter.instruction("b.ne __rt_sprintf_check_x");                           // no → check %x
    emitter.instruction("stp x1, x2, [sp]");                                   // save fmt state
    emitter.instruction("str x9, [sp, #56]");                                   // save dest ptr
    // -- load next arg (float bits) --
    emitter.instruction("ldr x13, [sp, #24]");                                  // arg index
    emitter.instruction("ldr x14, [sp, #32]");                                  // args base
    emitter.instruction("lsl x15, x13, #4");                                    // offset
    emitter.instruction("add x15, x14, x15");                                   // arg address
    emitter.instruction("ldr x0, [x15]");                                       // load float bits
    emitter.instruction("fmov d0, x0");                                         // move to float register
    emitter.instruction("add x13, x13, #1");                                    // increment arg index
    emitter.instruction("str x13, [sp, #24]");                                  // save arg index
    emitter.instruction("bl __rt_ftoa");                                        // convert to string
    // Copy ftoa result to output
    emitter.instruction("ldr x9, [sp, #56]");                                   // restore dest ptr
    emitter.label("__rt_sprintf_copy_f");
    emitter.instruction("cbz x2, __rt_sprintf_f_done");                         // done
    emitter.instruction("ldrb w15, [x1], #1");                                  // load byte
    emitter.instruction("strb w15, [x9], #1");                                  // write
    emitter.instruction("sub x2, x2, #1");                                      // decrement
    emitter.instruction("b __rt_sprintf_copy_f");                               // continue
    emitter.label("__rt_sprintf_f_done");
    emitter.instruction("ldp x1, x2, [sp]");                                   // restore fmt state
    emitter.instruction("b __rt_sprintf_loop");                                 // next

    // -- %x → format int as hex --
    emitter.label("__rt_sprintf_check_x");
    emitter.instruction("cmp w12, #120");                                       // is it 'x'?
    emitter.instruction("b.ne __rt_sprintf_other");                             // no → unknown, skip
    emitter.instruction("stp x1, x2, [sp]");                                   // save fmt state
    // -- load next arg (int) --
    emitter.instruction("ldr x13, [sp, #24]");                                  // arg index
    emitter.instruction("ldr x14, [sp, #32]");                                  // args base
    emitter.instruction("lsl x15, x13, #4");                                    // offset
    emitter.instruction("add x15, x14, x15");                                   // arg address
    emitter.instruction("ldr x0, [x15]");                                       // load value
    emitter.instruction("add x13, x13, #1");                                    // increment
    emitter.instruction("str x13, [sp, #24]");                                  // save
    // -- convert to hex: emit digits right-to-left --
    // Use a small scratch area at sp+56 (8 bytes enough for 16 hex digits)
    emitter.instruction("add x3, sp, #56");                                     // scratch end
    emitter.instruction("add x3, x3, #8");                                      // point past scratch
    emitter.instruction("mov x4, #0");                                          // digit count
    emitter.instruction("cbz x0, __rt_sprintf_x_zero");                         // handle zero
    emitter.label("__rt_sprintf_x_loop");
    emitter.instruction("cbz x0, __rt_sprintf_x_emit");                         // no more digits
    emitter.instruction("and w15, w0, #0xf");                                   // low nibble
    emitter.instruction("cmp w15, #10");                                        // >= 10?
    emitter.instruction("b.ge __rt_sprintf_x_af");                              // a-f
    emitter.instruction("add w15, w15, #48");                                   // '0'-'9'
    emitter.instruction("b __rt_sprintf_x_store");                              // store
    emitter.label("__rt_sprintf_x_af");
    emitter.instruction("add w15, w15, #87");                                   // 'a'-'f'
    emitter.label("__rt_sprintf_x_store");
    emitter.instruction("sub x3, x3, #1");                                      // move left
    emitter.instruction("strb w15, [x3]");                                      // store digit
    emitter.instruction("add x4, x4, #1");                                      // count++
    emitter.instruction("lsr x0, x0, #4");                                      // shift right 4
    emitter.instruction("b __rt_sprintf_x_loop");                               // next nibble
    emitter.label("__rt_sprintf_x_zero");
    emitter.instruction("mov w15, #48");                                        // '0'
    emitter.instruction("sub x3, x3, #1");                                      // move left
    emitter.instruction("strb w15, [x3]");                                      // store '0'
    emitter.instruction("mov x4, #1");                                          // 1 digit
    // -- copy hex digits to output --
    emitter.label("__rt_sprintf_x_emit");
    emitter.label("__rt_sprintf_x_copy");
    emitter.instruction("cbz x4, __rt_sprintf_x_done");                         // done
    emitter.instruction("ldrb w15, [x3], #1");                                  // load digit
    emitter.instruction("strb w15, [x9], #1");                                  // write to output
    emitter.instruction("sub x4, x4, #1");                                      // decrement
    emitter.instruction("b __rt_sprintf_x_copy");                               // continue
    emitter.label("__rt_sprintf_x_done");
    emitter.instruction("ldp x1, x2, [sp]");                                   // restore fmt state
    emitter.instruction("b __rt_sprintf_loop");                                 // next

    // -- unknown specifier: just write the character --
    emitter.label("__rt_sprintf_other");
    emitter.instruction("strb w12, [x9], #1");                                  // write unknown specifier as-is
    emitter.instruction("b __rt_sprintf_loop");                                 // next

    // -- done: clean up args from caller's stack --
    emitter.label("__rt_sprintf_done");
    emitter.instruction("ldr x1, [sp, #40]");                                  // result start ptr
    emitter.instruction("sub x2, x9, x1");                                      // result length
    // Update concat_off
    emitter.instruction("ldr x6, [sp, #48]");                                  // concat_off ptr
    emitter.instruction("ldr x8, [x6]");                                        // current offset
    emitter.instruction("add x8, x8, x2");                                      // advance
    emitter.instruction("str x8, [x6]");                                        // store

    // -- pop args from caller's stack --
    emitter.instruction("ldr x0, [sp, #16]");                                  // arg_count
    emitter.instruction("lsl x0, x0, #4");                                      // bytes = count * 16

    emitter.instruction("ldp x29, x30, [sp, #64]");                            // restore frame
    emitter.instruction("add sp, sp, #80");                                     // deallocate our frame
    emitter.instruction("add sp, sp, x0");                                      // pop caller's args
    emitter.instruction("ret");                                                 // return
}
