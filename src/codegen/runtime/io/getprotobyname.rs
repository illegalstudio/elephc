//! Purpose:
//! Emits the `__rt_getprotobyname` runtime helper, which scans the
//! `/etc/protocols` database for a protocol name or alias.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::io`.
//!
//! Key details:
//! - The query is matched against the canonical name and every alias on a
//!   line (field index 1 is the number and is never matched as a name).
//! - Returns the protocol number, or -1 when no entry matches; the builtin
//!   emitter boxes -1 as PHP `false`.

use crate::codegen::{emit::Emitter, platform::Arch};

/// getprotobyname: look up a protocol number by name.
/// Input:  AArch64 x0 = query pointer, x1 = query length
///         x86_64  rdi = query pointer, rsi = query length
/// Output: protocol number, or -1 when no entry matches
pub fn emit_getprotobyname(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_getprotobyname_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: getprotobyname ---");
    emitter.label_global("__rt_getprotobyname");

    // -- set up frame and load /etc/protocols --
    emitter.instruction("stp x29, x30, [sp, #-32]!");                           // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish a new frame pointer
    emitter.instruction("str x0, [sp, #16]");                                   // stash the query pointer across the load call
    emitter.instruction("str x1, [sp, #24]");                                   // stash the query length across the load call
    emitter.instruction("bl __rt_protoent_load");                               // read /etc/protocols, x0=buffer x1=count
    emitter.instruction("mov x10, x0");                                         // x10 = scan cursor
    emitter.instruction("add x11, x0, x1");                                     // x11 = end-of-buffer pointer
    emitter.instruction("ldr x12, [sp, #16]");                                  // x12 = query pointer
    emitter.instruction("ldr x13, [sp, #24]");                                  // x13 = query length

    // -- iterate over each line --
    emitter.label("__rt_gpbn_line");
    emitter.instruction("cmp x10, x11");                                        // reached the end of the buffer?
    emitter.instruction("b.hs __rt_gpbn_notfound");                             // no more lines: report not found

    // -- skip leading spaces and tabs on the line --
    emitter.label("__rt_gpbn_skipws0");
    emitter.instruction("cmp x10, x11");                                        // reached the end while skipping?
    emitter.instruction("b.hs __rt_gpbn_notfound");                             // no more lines: report not found
    emitter.instruction("ldrb w0, [x10]");                                      // load the current byte
    emitter.instruction("cmp w0, #0x20");                                       // is it a space?
    emitter.instruction("b.eq __rt_gpbn_skipws0_adv");                          // skip the space
    emitter.instruction("cmp w0, #0x09");                                       // is it a tab?
    emitter.instruction("b.eq __rt_gpbn_skipws0_adv");                          // skip the tab
    emitter.instruction("b __rt_gpbn_linestart");                               // first non-blank byte of the line
    emitter.label("__rt_gpbn_skipws0_adv");
    emitter.instruction("add x10, x10, #1");                                    // advance past the whitespace byte
    emitter.instruction("b __rt_gpbn_skipws0");                                 // keep skipping leading whitespace

    // -- classify the line: blank, comment, or data --
    emitter.label("__rt_gpbn_linestart");
    emitter.instruction("ldrb w0, [x10]");                                      // load the first non-blank byte
    emitter.instruction("cmp w0, #0x0A");                                       // is the line empty (newline)?
    emitter.instruction("b.eq __rt_gpbn_eol");                                  // consume the blank line
    emitter.instruction("cmp w0, #0x0D");                                       // is it a carriage return?
    emitter.instruction("b.eq __rt_gpbn_eol");                                  // consume the blank line
    emitter.instruction("cmp w0, #0x23");                                       // does the line start with '#'?
    emitter.instruction("b.eq __rt_gpbn_skipeol");                              // skip the comment line
    emitter.instruction("mov x14, #0");                                         // token index = 0
    emitter.instruction("mov x15, #-1");                                        // parsed protocol number = -1 (unseen)
    emitter.instruction("mov x9, #0");                                          // name-matched flag = 0

    // -- scan one whitespace-delimited token --
    emitter.label("__rt_gpbn_token");
    emitter.instruction("mov x2, x10");                                         // remember the token start pointer
    emitter.label("__rt_gpbn_tokscan");
    emitter.instruction("cmp x10, x11");                                        // reached the end of the buffer?
    emitter.instruction("b.hs __rt_gpbn_tokend");                               // end the token at end-of-buffer
    emitter.instruction("ldrb w0, [x10]");                                      // load the current token byte
    emitter.instruction("cmp w0, #0x20");                                       // is it a space?
    emitter.instruction("b.eq __rt_gpbn_tokend");                               // end the token on whitespace
    emitter.instruction("cmp w0, #0x09");                                       // is it a tab?
    emitter.instruction("b.eq __rt_gpbn_tokend");                               // end the token on whitespace
    emitter.instruction("cmp w0, #0x0A");                                       // is it a newline?
    emitter.instruction("b.eq __rt_gpbn_tokend");                               // end the token at end-of-line
    emitter.instruction("cmp w0, #0x0D");                                       // is it a carriage return?
    emitter.instruction("b.eq __rt_gpbn_tokend");                               // end the token at end-of-line
    emitter.instruction("cmp w0, #0x23");                                       // does a comment start here?
    emitter.instruction("b.eq __rt_gpbn_tokend");                               // end the token at the comment
    emitter.instruction("add x10, x10, #1");                                    // advance to the next token byte
    emitter.instruction("b __rt_gpbn_tokscan");                                 // keep scanning the token

    // -- token complete: it is either the number field or a name --
    emitter.label("__rt_gpbn_tokend");
    emitter.instruction("sub x3, x10, x2");                                     // x3 = token length
    emitter.instruction("cmp x14, #1");                                         // is this the number field (index 1)?
    emitter.instruction("b.eq __rt_gpbn_parsenum");                             // parse the protocol number

    // -- compare a name/alias token against the query --
    emitter.instruction("cmp x3, x13");                                         // token and query lengths must match
    emitter.instruction("b.ne __rt_gpbn_tokdone");                              // different length cannot match
    emitter.instruction("mov x4, #0");                                          // byte compare index = 0
    emitter.label("__rt_gpbn_cmp");
    emitter.instruction("cmp x4, x3");                                          // compared every byte?
    emitter.instruction("b.hs __rt_gpbn_cmpmatch");                             // all bytes equal: the name matched
    emitter.instruction("ldrb w5, [x2, x4]");                                   // load a token byte
    emitter.instruction("ldrb w6, [x12, x4]");                                  // load the matching query byte
    emitter.instruction("cmp w5, w6");                                          // do the bytes differ?
    emitter.instruction("b.ne __rt_gpbn_tokdone");                              // mismatch: this token is not the query
    emitter.instruction("add x4, x4, #1");                                      // advance to the next byte
    emitter.instruction("b __rt_gpbn_cmp");                                     // keep comparing
    emitter.label("__rt_gpbn_cmpmatch");
    emitter.instruction("mov x9, #1");                                          // record that a name or alias matched
    emitter.instruction("b __rt_gpbn_tokdone");                                 // continue scanning the line

    // -- parse the number field as a decimal integer --
    emitter.label("__rt_gpbn_parsenum");
    emitter.instruction("mov x15, #0");                                         // parsed number = 0
    emitter.instruction("mov x4, #0");                                          // digit index = 0
    emitter.instruction("mov x6, #10");                                         // decimal base
    emitter.label("__rt_gpbn_numloop");
    emitter.instruction("cmp x4, x3");                                          // consumed every digit?
    emitter.instruction("b.hs __rt_gpbn_tokdone");                              // number fully parsed
    emitter.instruction("ldrb w5, [x2, x4]");                                   // load a digit byte
    emitter.instruction("sub w5, w5, #0x30");                                   // convert ASCII digit to its value
    emitter.instruction("madd x15, x15, x6, x5");                               // number = number * 10 + digit
    emitter.instruction("add x4, x4, #1");                                      // advance to the next digit
    emitter.instruction("b __rt_gpbn_numloop");                                 // keep parsing digits

    // -- advance past the token and the whitespace after it --
    emitter.label("__rt_gpbn_tokdone");
    emitter.instruction("add x14, x14, #1");                                    // token index++
    emitter.label("__rt_gpbn_skipws1");
    emitter.instruction("cmp x10, x11");                                        // reached the end of the buffer?
    emitter.instruction("b.hs __rt_gpbn_eval");                                 // evaluate the line at end-of-buffer
    emitter.instruction("ldrb w0, [x10]");                                      // load the current byte
    emitter.instruction("cmp w0, #0x20");                                       // is it a space?
    emitter.instruction("b.eq __rt_gpbn_skipws1_adv");                          // skip the space
    emitter.instruction("cmp w0, #0x09");                                       // is it a tab?
    emitter.instruction("b.eq __rt_gpbn_skipws1_adv");                          // skip the tab
    emitter.instruction("b __rt_gpbn_aftertokws");                              // first byte after the whitespace
    emitter.label("__rt_gpbn_skipws1_adv");
    emitter.instruction("add x10, x10, #1");                                    // advance past the whitespace byte
    emitter.instruction("b __rt_gpbn_skipws1");                                 // keep skipping whitespace

    // -- decide whether more tokens follow or the line is done --
    emitter.label("__rt_gpbn_aftertokws");
    emitter.instruction("ldrb w0, [x10]");                                      // load the byte after the whitespace
    emitter.instruction("cmp w0, #0x0A");                                       // end of line?
    emitter.instruction("b.eq __rt_gpbn_eval");                                 // evaluate the finished line
    emitter.instruction("cmp w0, #0x0D");                                       // carriage return ends the line?
    emitter.instruction("b.eq __rt_gpbn_eval");                                 // evaluate the finished line
    emitter.instruction("cmp w0, #0x23");                                       // does a comment start here?
    emitter.instruction("b.eq __rt_gpbn_eval");                                 // evaluate the line before the comment
    emitter.instruction("b __rt_gpbn_token");                                   // scan the next token

    // -- evaluate the line: a matched name plus a number wins --
    emitter.label("__rt_gpbn_eval");
    emitter.instruction("cmp x9, #1");                                          // did a name or alias match?
    emitter.instruction("b.ne __rt_gpbn_skipeol");                              // no match: move to the next line
    emitter.instruction("cmp x15, #0");                                         // was a protocol number seen?
    emitter.instruction("b.lt __rt_gpbn_skipeol");                              // no number: move to the next line
    emitter.instruction("mov x0, x15");                                         // return the matched protocol number
    emitter.instruction("b __rt_gpbn_return");                                  // done

    // -- advance the cursor past the end of the current line --
    emitter.label("__rt_gpbn_skipeol");
    emitter.instruction("cmp x10, x11");                                        // reached the end of the buffer?
    emitter.instruction("b.hs __rt_gpbn_notfound");                             // no trailing line: report not found
    emitter.instruction("ldrb w0, [x10]");                                      // load the current byte
    emitter.instruction("add x10, x10, #1");                                    // consume the byte
    emitter.instruction("cmp w0, #0x0A");                                       // was it the line terminator?
    emitter.instruction("b.ne __rt_gpbn_skipeol");                              // keep skipping until the newline
    emitter.instruction("b __rt_gpbn_line");                                    // scan the next line

    // -- consume a single blank-line byte --
    emitter.label("__rt_gpbn_eol");
    emitter.instruction("add x10, x10, #1");                                    // consume the blank-line byte
    emitter.instruction("b __rt_gpbn_line");                                    // scan the next line

    // -- no entry matched --
    emitter.label("__rt_gpbn_notfound");
    emitter.instruction("mov x0, #-1");                                         // -1 sentinel: the builtin boxes PHP false

    emitter.label("__rt_gpbn_return");
    emitter.instruction("ldp x29, x30, [sp], #32");                             // restore frame pointer and return address
    emitter.instruction("ret");                                                 // return the protocol number or -1
}

fn emit_getprotobyname_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: getprotobyname ---");
    emitter.label_global("__rt_getprotobyname");

    // -- set up frame and load /etc/protocols --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("push r12");                                            // save callee-saved register (token start)
    emitter.instruction("push r13");                                            // save callee-saved register (query pointer)
    emitter.instruction("push r14");                                            // save callee-saved register (query length)
    emitter.instruction("push r15");                                            // save callee-saved register (matched flag)
    emitter.instruction("mov r13, rdi");                                        // r13 = query pointer, survives the load call
    emitter.instruction("mov r14, rsi");                                        // r14 = query length, survives the load call
    emitter.instruction("call __rt_protoent_load");                             // read /etc/protocols, rax=buffer rdx=count
    emitter.instruction("mov r8, rax");                                         // r8 = scan cursor
    emitter.instruction("lea r9, [rax + rdx]");                                 // r9 = end-of-buffer pointer

    // -- iterate over each line --
    emitter.label("__rt_gpbn_line");
    emitter.instruction("cmp r8, r9");                                          // reached the end of the buffer?
    emitter.instruction("jae __rt_gpbn_notfound");                              // no more lines: report not found

    // -- skip leading spaces and tabs on the line --
    emitter.label("__rt_gpbn_skipws0");
    emitter.instruction("cmp r8, r9");                                          // reached the end while skipping?
    emitter.instruction("jae __rt_gpbn_notfound");                              // no more lines: report not found
    emitter.instruction("movzx eax, BYTE PTR [r8]");                            // load the current byte
    emitter.instruction("cmp eax, 0x20");                                       // is it a space?
    emitter.instruction("je __rt_gpbn_skipws0_adv");                            // skip the space
    emitter.instruction("cmp eax, 0x09");                                       // is it a tab?
    emitter.instruction("je __rt_gpbn_skipws0_adv");                            // skip the tab
    emitter.instruction("jmp __rt_gpbn_linestart");                             // first non-blank byte of the line
    emitter.label("__rt_gpbn_skipws0_adv");
    emitter.instruction("inc r8");                                              // advance past the whitespace byte
    emitter.instruction("jmp __rt_gpbn_skipws0");                               // keep skipping leading whitespace

    // -- classify the line: blank, comment, or data --
    emitter.label("__rt_gpbn_linestart");
    emitter.instruction("movzx eax, BYTE PTR [r8]");                            // load the first non-blank byte
    emitter.instruction("cmp eax, 0x0A");                                       // is the line empty (newline)?
    emitter.instruction("je __rt_gpbn_eol");                                    // consume the blank line
    emitter.instruction("cmp eax, 0x0D");                                       // is it a carriage return?
    emitter.instruction("je __rt_gpbn_eol");                                    // consume the blank line
    emitter.instruction("cmp eax, 0x23");                                       // does the line start with '#'?
    emitter.instruction("je __rt_gpbn_skipeol");                                // skip the comment line
    emitter.instruction("xor r10d, r10d");                                      // token index = 0
    emitter.instruction("mov r11, -1");                                         // parsed protocol number = -1 (unseen)
    emitter.instruction("xor r15d, r15d");                                      // name-matched flag = 0

    // -- scan one whitespace-delimited token --
    emitter.label("__rt_gpbn_token");
    emitter.instruction("mov r12, r8");                                         // remember the token start pointer
    emitter.label("__rt_gpbn_tokscan");
    emitter.instruction("cmp r8, r9");                                          // reached the end of the buffer?
    emitter.instruction("jae __rt_gpbn_tokend");                                // end the token at end-of-buffer
    emitter.instruction("movzx eax, BYTE PTR [r8]");                            // load the current token byte
    emitter.instruction("cmp eax, 0x20");                                       // is it a space?
    emitter.instruction("je __rt_gpbn_tokend");                                 // end the token on whitespace
    emitter.instruction("cmp eax, 0x09");                                       // is it a tab?
    emitter.instruction("je __rt_gpbn_tokend");                                 // end the token on whitespace
    emitter.instruction("cmp eax, 0x0A");                                       // is it a newline?
    emitter.instruction("je __rt_gpbn_tokend");                                 // end the token at end-of-line
    emitter.instruction("cmp eax, 0x0D");                                       // is it a carriage return?
    emitter.instruction("je __rt_gpbn_tokend");                                 // end the token at end-of-line
    emitter.instruction("cmp eax, 0x23");                                       // does a comment start here?
    emitter.instruction("je __rt_gpbn_tokend");                                 // end the token at the comment
    emitter.instruction("inc r8");                                              // advance to the next token byte
    emitter.instruction("jmp __rt_gpbn_tokscan");                               // keep scanning the token

    // -- token complete: it is either the number field or a name --
    emitter.label("__rt_gpbn_tokend");
    emitter.instruction("mov rcx, r8");                                         // copy the token end pointer
    emitter.instruction("sub rcx, r12");                                        // rcx = token length
    emitter.instruction("cmp r10, 1");                                          // is this the number field (index 1)?
    emitter.instruction("je __rt_gpbn_parsenum");                               // parse the protocol number

    // -- compare a name/alias token against the query --
    emitter.instruction("cmp rcx, r14");                                        // token and query lengths must match
    emitter.instruction("jne __rt_gpbn_tokdone");                               // different length cannot match
    emitter.instruction("xor esi, esi");                                        // byte compare index = 0
    emitter.label("__rt_gpbn_cmp");
    emitter.instruction("cmp rsi, rcx");                                        // compared every byte?
    emitter.instruction("jae __rt_gpbn_cmpmatch");                              // all bytes equal: the name matched
    emitter.instruction("movzx eax, BYTE PTR [r12 + rsi]");                     // load a token byte
    emitter.instruction("movzx edx, BYTE PTR [r13 + rsi]");                     // load the matching query byte
    emitter.instruction("cmp al, dl");                                          // do the bytes differ?
    emitter.instruction("jne __rt_gpbn_tokdone");                               // mismatch: this token is not the query
    emitter.instruction("inc rsi");                                             // advance to the next byte
    emitter.instruction("jmp __rt_gpbn_cmp");                                   // keep comparing
    emitter.label("__rt_gpbn_cmpmatch");
    emitter.instruction("mov r15d, 1");                                         // record that a name or alias matched
    emitter.instruction("jmp __rt_gpbn_tokdone");                               // continue scanning the line

    // -- parse the number field as a decimal integer --
    emitter.label("__rt_gpbn_parsenum");
    emitter.instruction("xor r11d, r11d");                                      // parsed number = 0
    emitter.instruction("xor esi, esi");                                        // digit index = 0
    emitter.label("__rt_gpbn_numloop");
    emitter.instruction("cmp rsi, rcx");                                        // consumed every digit?
    emitter.instruction("jae __rt_gpbn_tokdone");                               // number fully parsed
    emitter.instruction("movzx eax, BYTE PTR [r12 + rsi]");                     // load a digit byte
    emitter.instruction("sub eax, 0x30");                                       // convert ASCII digit to its value
    emitter.instruction("imul r11, r11, 10");                                   // number *= 10
    emitter.instruction("add r11, rax");                                        // number += digit
    emitter.instruction("inc rsi");                                             // advance to the next digit
    emitter.instruction("jmp __rt_gpbn_numloop");                               // keep parsing digits

    // -- advance past the token and the whitespace after it --
    emitter.label("__rt_gpbn_tokdone");
    emitter.instruction("inc r10");                                             // token index++
    emitter.label("__rt_gpbn_skipws1");
    emitter.instruction("cmp r8, r9");                                          // reached the end of the buffer?
    emitter.instruction("jae __rt_gpbn_eval");                                  // evaluate the line at end-of-buffer
    emitter.instruction("movzx eax, BYTE PTR [r8]");                            // load the current byte
    emitter.instruction("cmp eax, 0x20");                                       // is it a space?
    emitter.instruction("je __rt_gpbn_skipws1_adv");                            // skip the space
    emitter.instruction("cmp eax, 0x09");                                       // is it a tab?
    emitter.instruction("je __rt_gpbn_skipws1_adv");                            // skip the tab
    emitter.instruction("jmp __rt_gpbn_aftertokws");                            // first byte after the whitespace
    emitter.label("__rt_gpbn_skipws1_adv");
    emitter.instruction("inc r8");                                              // advance past the whitespace byte
    emitter.instruction("jmp __rt_gpbn_skipws1");                               // keep skipping whitespace

    // -- decide whether more tokens follow or the line is done --
    emitter.label("__rt_gpbn_aftertokws");
    emitter.instruction("movzx eax, BYTE PTR [r8]");                            // load the byte after the whitespace
    emitter.instruction("cmp eax, 0x0A");                                       // end of line?
    emitter.instruction("je __rt_gpbn_eval");                                   // evaluate the finished line
    emitter.instruction("cmp eax, 0x0D");                                       // carriage return ends the line?
    emitter.instruction("je __rt_gpbn_eval");                                   // evaluate the finished line
    emitter.instruction("cmp eax, 0x23");                                       // does a comment start here?
    emitter.instruction("je __rt_gpbn_eval");                                   // evaluate the line before the comment
    emitter.instruction("jmp __rt_gpbn_token");                                 // scan the next token

    // -- evaluate the line: a matched name plus a number wins --
    emitter.label("__rt_gpbn_eval");
    emitter.instruction("cmp r15d, 1");                                         // did a name or alias match?
    emitter.instruction("jne __rt_gpbn_skipeol");                               // no match: move to the next line
    emitter.instruction("cmp r11, 0");                                          // was a protocol number seen?
    emitter.instruction("jl __rt_gpbn_skipeol");                                // no number: move to the next line
    emitter.instruction("mov rax, r11");                                        // return the matched protocol number
    emitter.instruction("jmp __rt_gpbn_return");                                // done

    // -- advance the cursor past the end of the current line --
    emitter.label("__rt_gpbn_skipeol");
    emitter.instruction("cmp r8, r9");                                          // reached the end of the buffer?
    emitter.instruction("jae __rt_gpbn_notfound");                              // no trailing line: report not found
    emitter.instruction("movzx eax, BYTE PTR [r8]");                            // load the current byte
    emitter.instruction("inc r8");                                              // consume the byte
    emitter.instruction("cmp eax, 0x0A");                                       // was it the line terminator?
    emitter.instruction("jne __rt_gpbn_skipeol");                               // keep skipping until the newline
    emitter.instruction("jmp __rt_gpbn_line");                                  // scan the next line

    // -- consume a single blank-line byte --
    emitter.label("__rt_gpbn_eol");
    emitter.instruction("inc r8");                                              // consume the blank-line byte
    emitter.instruction("jmp __rt_gpbn_line");                                  // scan the next line

    // -- no entry matched --
    emitter.label("__rt_gpbn_notfound");
    emitter.instruction("mov rax, -1");                                         // -1 sentinel: the builtin boxes PHP false

    emitter.label("__rt_gpbn_return");
    emitter.instruction("pop r15");                                             // restore callee-saved register
    emitter.instruction("pop r14");                                             // restore callee-saved register
    emitter.instruction("pop r13");                                             // restore callee-saved register
    emitter.instruction("pop r12");                                             // restore callee-saved register
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the protocol number or -1
}
