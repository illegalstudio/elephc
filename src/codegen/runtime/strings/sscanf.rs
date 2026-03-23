use crate::codegen::emit::Emitter;

/// sscanf: parse a string according to a format, returning matched values as string array.
/// Input: x1/x2=input string, x3/x4=format string
/// Output: x0=array pointer (array of strings)
/// Supports: %d (digits), %s (non-whitespace word), %% (literal %)
/// Literal chars in format must match input exactly.
pub fn emit_sscanf(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: sscanf ---");
    emitter.label("__rt_sscanf");
    emitter.instruction("sub sp, sp, #80");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #64]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // set frame pointer
    emitter.instruction("stp x1, x2, [sp]");                                   // save input ptr/len
    emitter.instruction("stp x3, x4, [sp, #16]");                              // save format ptr/len

    // -- create result array --
    emitter.instruction("mov x0, #8");                                          // initial capacity
    emitter.instruction("mov x1, #16");                                         // elem_size = 16 (string ptr + len)
    emitter.instruction("bl __rt_array_new");                                   // allocate array
    emitter.instruction("str x0, [sp, #32]");                                   // save array pointer

    // -- scan loop: walk format string --
    emitter.instruction("ldp x1, x2, [sp]");                                   // reload input
    emitter.instruction("ldp x3, x4, [sp, #16]");                              // reload format

    emitter.label("__rt_sscanf_loop");
    emitter.instruction("cbz x4, __rt_sscanf_done");                           // format exhausted → done
    emitter.instruction("ldrb w9, [x3], #1");                                   // load format char, advance
    emitter.instruction("sub x4, x4, #1");                                      // decrement format remaining
    emitter.instruction("cmp w9, #37");                                         // is it '%'?
    emitter.instruction("b.eq __rt_sscanf_spec");                               // yes → process specifier

    // -- literal char: must match input --
    emitter.instruction("cbz x2, __rt_sscanf_done");                           // input exhausted → done
    emitter.instruction("ldrb w10, [x1], #1");                                  // load input char, advance
    emitter.instruction("sub x2, x2, #1");                                      // decrement input remaining
    emitter.instruction("cmp w9, w10");                                         // format char == input char?
    emitter.instruction("b.eq __rt_sscanf_loop");                               // yes → continue
    emitter.instruction("b __rt_sscanf_done");                                  // no → stop (mismatch)

    // -- format specifier --
    emitter.label("__rt_sscanf_spec");
    emitter.instruction("cbz x4, __rt_sscanf_done");                           // no char after % → done
    emitter.instruction("ldrb w9, [x3], #1");                                   // load specifier
    emitter.instruction("sub x4, x4, #1");                                      // decrement format

    // -- %% literal percent --
    emitter.instruction("cmp w9, #37");                                         // is it '%'?
    emitter.instruction("b.ne __rt_sscanf_check_d");                            // no → check %d
    emitter.instruction("cbz x2, __rt_sscanf_done");                           // input exhausted
    emitter.instruction("ldrb w10, [x1], #1");                                  // consume input '%'
    emitter.instruction("sub x2, x2, #1");                                      // decrement
    emitter.instruction("b __rt_sscanf_loop");                                  // continue

    // -- %d: extract digits --
    emitter.label("__rt_sscanf_check_d");
    emitter.instruction("cmp w9, #100");                                        // 'd'?
    emitter.instruction("b.ne __rt_sscanf_check_s");                            // no → check %s
    // Save state
    emitter.instruction("stp x3, x4, [sp, #16]");                              // save format state
    // Mark start of digits
    emitter.instruction("mov x5, x1");                                          // start of match
    emitter.instruction("mov x6, #0");                                          // digit count
    // Skip optional minus sign
    emitter.instruction("cbz x2, __rt_sscanf_d_end");                          // no input
    emitter.instruction("ldrb w10, [x1]");                                      // peek
    emitter.instruction("cmp w10, #45");                                        // '-'?
    emitter.instruction("b.ne __rt_sscanf_d_loop");                             // no → digits
    emitter.instruction("add x1, x1, #1");                                      // skip '-'
    emitter.instruction("sub x2, x2, #1");                                      // decrement
    emitter.instruction("add x6, x6, #1");                                      // count '-'
    // Scan digits
    emitter.label("__rt_sscanf_d_loop");
    emitter.instruction("cbz x2, __rt_sscanf_d_end");                          // input exhausted
    emitter.instruction("ldrb w10, [x1]");                                      // peek at char
    emitter.instruction("cmp w10, #48");                                        // < '0'?
    emitter.instruction("b.lt __rt_sscanf_d_end");                              // not a digit
    emitter.instruction("cmp w10, #57");                                        // > '9'?
    emitter.instruction("b.gt __rt_sscanf_d_end");                              // not a digit
    emitter.instruction("add x1, x1, #1");                                      // consume digit
    emitter.instruction("sub x2, x2, #1");                                      // decrement
    emitter.instruction("add x6, x6, #1");                                      // count
    emitter.instruction("b __rt_sscanf_d_loop");                                // continue
    emitter.label("__rt_sscanf_d_end");
    // Push matched string (x5=start, x6=len) into array
    emitter.instruction("stp x1, x2, [sp]");                                   // save input state
    emitter.instruction("ldr x0, [sp, #32]");                                  // array ptr
    emitter.instruction("mov x1, x5");                                          // matched start
    emitter.instruction("mov x2, x6");                                          // matched length
    emitter.instruction("bl __rt_array_push_str");                              // push to array
    emitter.instruction("ldp x1, x2, [sp]");                                   // restore input state
    emitter.instruction("ldp x3, x4, [sp, #16]");                              // restore format state
    emitter.instruction("b __rt_sscanf_loop");                                  // continue

    // -- %s: extract non-whitespace word --
    emitter.label("__rt_sscanf_check_s");
    emitter.instruction("cmp w9, #115");                                        // 's'?
    emitter.instruction("b.ne __rt_sscanf_loop");                               // unknown specifier → skip
    emitter.instruction("stp x3, x4, [sp, #16]");                              // save format state
    emitter.instruction("mov x5, x1");                                          // start of match
    emitter.instruction("mov x6, #0");                                          // char count
    emitter.label("__rt_sscanf_s_loop");
    emitter.instruction("cbz x2, __rt_sscanf_s_end");                          // input exhausted
    emitter.instruction("ldrb w10, [x1]");                                      // peek
    emitter.instruction("cmp w10, #32");                                        // space?
    emitter.instruction("b.eq __rt_sscanf_s_end");                              // stop on whitespace
    emitter.instruction("cmp w10, #9");                                         // tab?
    emitter.instruction("b.eq __rt_sscanf_s_end");                              // stop
    emitter.instruction("cmp w10, #10");                                        // newline?
    emitter.instruction("b.eq __rt_sscanf_s_end");                              // stop
    emitter.instruction("add x1, x1, #1");                                      // consume char
    emitter.instruction("sub x2, x2, #1");                                      // decrement
    emitter.instruction("add x6, x6, #1");                                      // count
    emitter.instruction("b __rt_sscanf_s_loop");                                // continue
    emitter.label("__rt_sscanf_s_end");
    emitter.instruction("stp x1, x2, [sp]");                                   // save input state
    emitter.instruction("ldr x0, [sp, #32]");                                  // array ptr
    emitter.instruction("mov x1, x5");                                          // matched start
    emitter.instruction("mov x2, x6");                                          // matched length
    emitter.instruction("bl __rt_array_push_str");                              // push to array
    emitter.instruction("ldp x1, x2, [sp]");                                   // restore input state
    emitter.instruction("ldp x3, x4, [sp, #16]");                              // restore format state
    emitter.instruction("b __rt_sscanf_loop");                                  // continue

    // -- done --
    emitter.label("__rt_sscanf_done");
    emitter.instruction("ldr x0, [sp, #32]");                                  // return array pointer
    emitter.instruction("ldp x29, x30, [sp, #64]");                            // restore frame
    emitter.instruction("add sp, sp, #80");                                     // deallocate
    emitter.instruction("ret");                                                 // return
}
