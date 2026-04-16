use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// sscanf: parse a string according to a format, returning matched values as string array.
/// Input: x1/x2=input string, x3/x4=format string
/// Output: x0=array pointer (array of strings)
/// Supports: %d (digits), %s (non-whitespace word), %% (literal %)
/// Literal chars in format must match input exactly.
pub fn emit_sscanf(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_sscanf_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: sscanf ---");
    emitter.label_global("__rt_sscanf");
    emitter.instruction("sub sp, sp, #80");                                     // allocate stack frame
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // set frame pointer
    emitter.instruction("stp x1, x2, [sp]");                                    // save input ptr/len
    emitter.instruction("stp x3, x4, [sp, #16]");                               // save format ptr/len

    // -- create result array --
    emitter.instruction("mov x0, #8");                                          // initial capacity
    emitter.instruction("mov x1, #16");                                         // elem_size = 16 (string ptr + len)
    emitter.instruction("bl __rt_array_new");                                   // allocate array
    emitter.instruction("str x0, [sp, #32]");                                   // save array pointer

    // -- scan loop: walk format string --
    emitter.instruction("ldp x1, x2, [sp]");                                    // reload input
    emitter.instruction("ldp x3, x4, [sp, #16]");                               // reload format

    emitter.label("__rt_sscanf_loop");
    emitter.instruction("cbz x4, __rt_sscanf_done");                            // format exhausted → done
    emitter.instruction("ldrb w9, [x3], #1");                                   // load format char, advance
    emitter.instruction("sub x4, x4, #1");                                      // decrement format remaining
    emitter.instruction("cmp w9, #37");                                         // is it '%'?
    emitter.instruction("b.eq __rt_sscanf_spec");                               // yes → process specifier

    // -- literal char: must match input --
    emitter.instruction("cbz x2, __rt_sscanf_done");                            // input exhausted → done
    emitter.instruction("ldrb w10, [x1], #1");                                  // load input char, advance
    emitter.instruction("sub x2, x2, #1");                                      // decrement input remaining
    emitter.instruction("cmp w9, w10");                                         // format char == input char?
    emitter.instruction("b.eq __rt_sscanf_loop");                               // yes → continue
    emitter.instruction("b __rt_sscanf_done");                                  // no → stop (mismatch)

    // -- format specifier --
    emitter.label("__rt_sscanf_spec");
    emitter.instruction("cbz x4, __rt_sscanf_done");                            // no char after % → done
    emitter.instruction("ldrb w9, [x3], #1");                                   // load specifier
    emitter.instruction("sub x4, x4, #1");                                      // decrement format

    // -- %% literal percent --
    emitter.instruction("cmp w9, #37");                                         // is it '%'?
    emitter.instruction("b.ne __rt_sscanf_check_d");                            // no → check %d
    emitter.instruction("cbz x2, __rt_sscanf_done");                            // input exhausted
    emitter.instruction("ldrb w10, [x1], #1");                                  // consume input '%'
    emitter.instruction("sub x2, x2, #1");                                      // decrement
    emitter.instruction("b __rt_sscanf_loop");                                  // continue

    // -- %d: extract digits --
    emitter.label("__rt_sscanf_check_d");
    emitter.instruction("cmp w9, #100");                                        // 'd'?
    emitter.instruction("b.ne __rt_sscanf_check_s");                            // no → check %s
    // Save state
    emitter.instruction("stp x3, x4, [sp, #16]");                               // save format state
    // Mark start of digits
    emitter.instruction("mov x5, x1");                                          // start of match
    emitter.instruction("mov x6, #0");                                          // digit count
    // Skip optional minus sign
    emitter.instruction("cbz x2, __rt_sscanf_d_end");                           // no input
    emitter.instruction("ldrb w10, [x1]");                                      // peek
    emitter.instruction("cmp w10, #45");                                        // '-'?
    emitter.instruction("b.ne __rt_sscanf_d_loop");                             // no → digits
    emitter.instruction("add x1, x1, #1");                                      // skip '-'
    emitter.instruction("sub x2, x2, #1");                                      // decrement
    emitter.instruction("add x6, x6, #1");                                      // count '-'
    // Scan digits
    emitter.label("__rt_sscanf_d_loop");
    emitter.instruction("cbz x2, __rt_sscanf_d_end");                           // input exhausted
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
    emitter.instruction("stp x1, x2, [sp]");                                    // save input state
    emitter.instruction("ldr x0, [sp, #32]");                                   // array ptr
    emitter.instruction("mov x1, x5");                                          // matched start
    emitter.instruction("mov x2, x6");                                          // matched length
    emitter.instruction("bl __rt_array_push_str");                              // push to array
    emitter.instruction("str x0, [sp, #32]");                                   // update array pointer after possible realloc
    emitter.instruction("ldp x1, x2, [sp]");                                    // restore input state
    emitter.instruction("ldp x3, x4, [sp, #16]");                               // restore format state
    emitter.instruction("b __rt_sscanf_loop");                                  // continue

    // -- %s: extract non-whitespace word --
    emitter.label("__rt_sscanf_check_s");
    emitter.instruction("cmp w9, #115");                                        // 's'?
    emitter.instruction("b.ne __rt_sscanf_loop");                               // unknown specifier → skip
    emitter.instruction("stp x3, x4, [sp, #16]");                               // save format state
    emitter.instruction("mov x5, x1");                                          // start of match
    emitter.instruction("mov x6, #0");                                          // char count
    emitter.label("__rt_sscanf_s_loop");
    emitter.instruction("cbz x2, __rt_sscanf_s_end");                           // input exhausted
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
    emitter.instruction("stp x1, x2, [sp]");                                    // save input state
    emitter.instruction("ldr x0, [sp, #32]");                                   // array ptr
    emitter.instruction("mov x1, x5");                                          // matched start
    emitter.instruction("mov x2, x6");                                          // matched length
    emitter.instruction("bl __rt_array_push_str");                              // push to array
    emitter.instruction("str x0, [sp, #32]");                                   // update array pointer after possible realloc
    emitter.instruction("ldp x1, x2, [sp]");                                    // restore input state
    emitter.instruction("ldp x3, x4, [sp, #16]");                               // restore format state
    emitter.instruction("b __rt_sscanf_loop");                                  // continue

    // -- done --
    emitter.label("__rt_sscanf_done");
    emitter.instruction("ldr x0, [sp, #32]");                                   // return array pointer
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame
    emitter.instruction("add sp, sp, #80");                                     // deallocate
    emitter.instruction("ret");                                                 // return
}

fn emit_sscanf_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: sscanf ---");
    emitter.label_global("__rt_sscanf");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving sscanf() spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the input, format, and result-array state
    emitter.instruction("sub rsp, 48");                                         // reserve aligned spill slots for the input string, format string, and result array pointer
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the current input-string pointer across array-allocation and append helper calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // preserve the current input-string length across array-allocation and append helper calls
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // preserve the current format-string pointer across array-allocation and append helper calls
    emitter.instruction("mov QWORD PTR [rbp - 32], rsi");                       // preserve the current format-string length across array-allocation and append helper calls
    emitter.instruction("mov rdi, 8");                                          // request the default sscanf() result-array capacity from the shared x86_64 array constructor
    emitter.instruction("mov rsi, 16");                                         // request 16-byte string slots so the result array can hold ptr+len pairs
    emitter.instruction("call __rt_array_new");                                 // allocate the result indexed array that will collect matched string slices
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve the result indexed-array pointer across the main scanf loop
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the input-string pointer into the active x86_64 string register
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // reload the input-string length into the active x86_64 string register
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // reload the format-string pointer into the active x86_64 string register
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // reload the format-string length into the active x86_64 string register

    emitter.label("__rt_sscanf_loop_linux_x86_64");
    emitter.instruction("test rsi, rsi");                                       // has the format string been fully consumed already?
    emitter.instruction("jz __rt_sscanf_done_linux_x86_64");                    // stop parsing once the format string is exhausted
    emitter.instruction("movzx r8d, BYTE PTR [rdi]");                           // load the next format character before deciding between literal and specifier parsing
    emitter.instruction("add rdi, 1");                                          // advance the format pointer after consuming one format character
    emitter.instruction("sub rsi, 1");                                          // decrement the remaining format length after consuming one format character
    emitter.instruction("cmp r8b, 37");                                         // does the current format character start a '%' specifier?
    emitter.instruction("je __rt_sscanf_spec_linux_x86_64");                    // dispatch to the specifier parser when the current format character is '%'
    emitter.instruction("test rdx, rdx");                                       // is the input already exhausted while a literal format character remains?
    emitter.instruction("jz __rt_sscanf_done_linux_x86_64");                    // stop parsing when a literal format character cannot be matched against empty input
    emitter.instruction("movzx r9d, BYTE PTR [rax]");                           // load the next input character for literal matching
    emitter.instruction("add rax, 1");                                          // advance the input pointer after consuming one literal candidate character
    emitter.instruction("sub rdx, 1");                                          // decrement the remaining input length after consuming one literal candidate character
    emitter.instruction("cmp r8b, r9b");                                        // does the literal format character match the consumed input character?
    emitter.instruction("je __rt_sscanf_loop_linux_x86_64");                    // continue scanning when the literal format character matched successfully
    emitter.instruction("jmp __rt_sscanf_done_linux_x86_64");                   // stop parsing on the first literal mismatch

    emitter.label("__rt_sscanf_spec_linux_x86_64");
    emitter.instruction("test rsi, rsi");                                       // is the format string exhausted immediately after the '%' introducer?
    emitter.instruction("jz __rt_sscanf_done_linux_x86_64");                    // stop parsing when a trailing '%' lacks a following specifier
    emitter.instruction("movzx r8d, BYTE PTR [rdi]");                           // load the actual format specifier character after '%'
    emitter.instruction("add rdi, 1");                                          // advance the format pointer after consuming the specifier character
    emitter.instruction("sub rsi, 1");                                          // decrement the remaining format length after consuming the specifier character
    emitter.instruction("cmp r8b, 37");                                         // is the format specifier '%%' for a literal percent sign?
    emitter.instruction("jne __rt_sscanf_check_d_linux_x86_64");                // fall through to typed scanning when the format specifier is not a literal percent sign
    emitter.instruction("test rdx, rdx");                                       // is the input already exhausted while the format requests a literal percent sign?
    emitter.instruction("jz __rt_sscanf_done_linux_x86_64");                    // stop parsing when the literal percent cannot be matched against empty input
    emitter.instruction("add rax, 1");                                          // consume one input character for the literal percent branch
    emitter.instruction("sub rdx, 1");                                          // decrement the remaining input length after consuming the literal percent
    emitter.instruction("jmp __rt_sscanf_loop_linux_x86_64");                   // continue scanning after the literal percent branch

    emitter.label("__rt_sscanf_check_d_linux_x86_64");
    emitter.instruction("cmp r8b, 100");                                        // is the current format specifier '%d'?
    emitter.instruction("jne __rt_sscanf_check_s_linux_x86_64");                // fall through to '%s' handling when the current specifier is not '%d'
    emitter.instruction("mov r10, rax");                                        // mark the start of the matched integer slice before scanning optional sign and digits
    emitter.instruction("xor r11d, r11d");                                      // start the matched integer-slice length at zero bytes
    emitter.instruction("test rdx, rdx");                                       // is there at least one input byte available for an optional leading minus sign?
    emitter.instruction("jz __rt_sscanf_push_d_linux_x86_64");                  // push the current empty integer match immediately when the input is already exhausted
    emitter.instruction("movzx r9d, BYTE PTR [rax]");                           // peek at the next input character before deciding whether it is a leading minus sign
    emitter.instruction("cmp r9b, 45");                                         // is the next input character a leading minus sign?
    emitter.instruction("jne __rt_sscanf_d_loop_linux_x86_64");                 // begin scanning digits immediately when there is no leading minus sign
    emitter.instruction("add rax, 1");                                          // consume the leading minus sign as part of the matched integer slice
    emitter.instruction("sub rdx, 1");                                          // decrement the remaining input length after consuming the leading minus sign
    emitter.instruction("add r11, 1");                                          // count the leading minus sign as part of the matched integer-slice length

    emitter.label("__rt_sscanf_d_loop_linux_x86_64");
    emitter.instruction("test rdx, rdx");                                       // is the input exhausted before another integer digit can be scanned?
    emitter.instruction("jz __rt_sscanf_push_d_linux_x86_64");                  // push the current integer slice once the input is exhausted
    emitter.instruction("movzx r9d, BYTE PTR [rax]");                           // peek at the next input character before deciding whether it is another digit
    emitter.instruction("cmp r9b, 48");                                         // is the next input character below ASCII '0'?
    emitter.instruction("jl __rt_sscanf_push_d_linux_x86_64");                  // stop the integer scan on the first non-digit input character below '0'
    emitter.instruction("cmp r9b, 57");                                         // is the next input character above ASCII '9'?
    emitter.instruction("jg __rt_sscanf_push_d_linux_x86_64");                  // stop the integer scan on the first non-digit input character above '9'
    emitter.instruction("add rax, 1");                                          // consume the matched integer digit from the input string
    emitter.instruction("sub rdx, 1");                                          // decrement the remaining input length after consuming one integer digit
    emitter.instruction("add r11, 1");                                          // count the consumed integer digit as part of the matched integer-slice length
    emitter.instruction("jmp __rt_sscanf_d_loop_linux_x86_64");                 // continue scanning digits until the integer slice ends

    emitter.label("__rt_sscanf_push_d_linux_x86_64");
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the current input-string pointer before appending the matched integer slice
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // preserve the current input-string length before appending the matched integer slice
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // preserve the current format-string pointer before appending the matched integer slice
    emitter.instruction("mov QWORD PTR [rbp - 32], rsi");                       // preserve the current format-string length before appending the matched integer slice
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // reload the result indexed-array pointer into the x86_64 array-append receiver register
    emitter.instruction("mov rsi, r10");                                        // pass the matched integer-slice pointer to the shared string-array append helper
    emitter.instruction("mov rdx, r11");                                        // pass the matched integer-slice length to the shared string-array append helper
    emitter.instruction("call __rt_array_push_str");                            // persist and append the matched integer slice into the result indexed array
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve the possibly-grown result indexed-array pointer returned by the append helper
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // restore the current input-string pointer after appending the matched integer slice
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // restore the current input-string length after appending the matched integer slice
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // restore the current format-string pointer after appending the matched integer slice
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // restore the current format-string length after appending the matched integer slice
    emitter.instruction("jmp __rt_sscanf_loop_linux_x86_64");                   // continue scanning the remaining format string after pushing the integer slice

    emitter.label("__rt_sscanf_check_s_linux_x86_64");
    emitter.instruction("cmp r8b, 115");                                        // is the current format specifier '%s'?
    emitter.instruction("jne __rt_sscanf_loop_linux_x86_64");                   // skip unknown format specifiers instead of aborting the whole scan
    emitter.instruction("mov r10, rax");                                        // mark the start of the matched word slice before scanning non-whitespace bytes
    emitter.instruction("xor r11d, r11d");                                      // start the matched word-slice length at zero bytes

    emitter.label("__rt_sscanf_s_loop_linux_x86_64");
    emitter.instruction("test rdx, rdx");                                       // is the input exhausted before another non-whitespace byte can be scanned?
    emitter.instruction("jz __rt_sscanf_push_s_linux_x86_64");                  // push the current word slice once the input is exhausted
    emitter.instruction("movzx r9d, BYTE PTR [rax]");                           // peek at the next input character before deciding whether it terminates the word slice
    emitter.instruction("cmp r9b, 32");                                         // is the next input character a space terminator?
    emitter.instruction("je __rt_sscanf_push_s_linux_x86_64");                  // stop the word scan when a space terminator is reached
    emitter.instruction("cmp r9b, 9");                                          // is the next input character a tab terminator?
    emitter.instruction("je __rt_sscanf_push_s_linux_x86_64");                  // stop the word scan when a tab terminator is reached
    emitter.instruction("cmp r9b, 10");                                         // is the next input character a newline terminator?
    emitter.instruction("je __rt_sscanf_push_s_linux_x86_64");                  // stop the word scan when a newline terminator is reached
    emitter.instruction("add rax, 1");                                          // consume the matched non-whitespace input byte
    emitter.instruction("sub rdx, 1");                                          // decrement the remaining input length after consuming one non-whitespace byte
    emitter.instruction("add r11, 1");                                          // count the consumed non-whitespace byte as part of the matched word-slice length
    emitter.instruction("jmp __rt_sscanf_s_loop_linux_x86_64");                 // continue scanning non-whitespace input bytes until the word slice ends

    emitter.label("__rt_sscanf_push_s_linux_x86_64");
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the current input-string pointer before appending the matched word slice
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // preserve the current input-string length before appending the matched word slice
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // preserve the current format-string pointer before appending the matched word slice
    emitter.instruction("mov QWORD PTR [rbp - 32], rsi");                       // preserve the current format-string length before appending the matched word slice
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // reload the result indexed-array pointer into the x86_64 array-append receiver register
    emitter.instruction("mov rsi, r10");                                        // pass the matched word-slice pointer to the shared string-array append helper
    emitter.instruction("mov rdx, r11");                                        // pass the matched word-slice length to the shared string-array append helper
    emitter.instruction("call __rt_array_push_str");                            // persist and append the matched word slice into the result indexed array
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve the possibly-grown result indexed-array pointer returned by the append helper
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // restore the current input-string pointer after appending the matched word slice
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // restore the current input-string length after appending the matched word slice
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // restore the current format-string pointer after appending the matched word slice
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // restore the current format-string length after appending the matched word slice
    emitter.instruction("jmp __rt_sscanf_loop_linux_x86_64");                   // continue scanning the remaining format string after pushing the word slice

    emitter.label("__rt_sscanf_done_linux_x86_64");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // return the result indexed-array pointer in the primary x86_64 integer result register
    emitter.instruction("add rsp, 48");                                         // release the sscanf() spill slots before returning
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to the caller
    emitter.instruction("ret");                                                 // return the result indexed array in rax
}
