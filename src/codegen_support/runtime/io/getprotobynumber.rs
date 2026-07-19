//! Purpose:
//! Emits the `__rt_getprotobynumber` runtime helper, which scans the
//! `/etc/protocols` database for an entry by protocol number.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - On a match the canonical name (field index 0) is duplicated into owned
//!   heap storage via `__rt_str_persist` so the result survives later calls.
//! - Returns a null pointer when no entry matches; the builtin emitter boxes
//!   that as PHP `false`.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// getprotobynumber: look up a protocol name by number.
/// Input:  AArch64 x0 = protocol number
///         x86_64  rdi = protocol number
/// Output: string pointer/length, or a null pointer when no entry matches
pub fn emit_getprotobynumber(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_getprotobynumber_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: getprotobynumber ---");
    emitter.label_global("__rt_getprotobynumber");

    // -- set up frame and load /etc/protocols --
    emitter.instruction("stp x29, x30, [sp, #-32]!");                           // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish a new frame pointer
    emitter.instruction("str x0, [sp, #16]");                                   // stash the query number across the load call
    emitter.instruction("bl __rt_protoent_load");                               // read /etc/protocols, x0=buffer x1=count
    emitter.instruction("mov x10, x0");                                         // x10 = scan cursor
    emitter.instruction("add x11, x0, x1");                                     // x11 = end-of-buffer pointer
    emitter.instruction("ldr x12, [sp, #16]");                                  // x12 = query protocol number

    // -- iterate over each line --
    emitter.label("__rt_gpbnum_line");
    emitter.instruction("cmp x10, x11");                                        // reached the end of the buffer?
    emitter.instruction("b.hs __rt_gpbnum_notfound");                           // no more lines: report not found

    // -- skip leading spaces and tabs on the line --
    emitter.label("__rt_gpbnum_skipws0");
    emitter.instruction("cmp x10, x11");                                        // reached the end while skipping?
    emitter.instruction("b.hs __rt_gpbnum_notfound");                           // no more lines: report not found
    emitter.instruction("ldrb w0, [x10]");                                      // load the current byte
    emitter.instruction("cmp w0, #0x20");                                       // is it a space?
    emitter.instruction("b.eq __rt_gpbnum_skipws0_adv");                        // skip the space
    emitter.instruction("cmp w0, #0x09");                                       // is it a tab?
    emitter.instruction("b.eq __rt_gpbnum_skipws0_adv");                        // skip the tab
    emitter.instruction("b __rt_gpbnum_linestart");                             // first non-blank byte of the line
    emitter.label("__rt_gpbnum_skipws0_adv");
    emitter.instruction("add x10, x10, #1");                                    // advance past the whitespace byte
    emitter.instruction("b __rt_gpbnum_skipws0");                               // keep skipping leading whitespace

    // -- classify the line: blank, comment, or data --
    emitter.label("__rt_gpbnum_linestart");
    emitter.instruction("ldrb w0, [x10]");                                      // load the first non-blank byte
    emitter.instruction("cmp w0, #0x0A");                                       // is the line empty (newline)?
    emitter.instruction("b.eq __rt_gpbnum_eol");                                // consume the blank line
    emitter.instruction("cmp w0, #0x0D");                                       // is it a carriage return?
    emitter.instruction("b.eq __rt_gpbnum_eol");                                // consume the blank line
    emitter.instruction("cmp w0, #0x23");                                       // does the line start with '#'?
    emitter.instruction("b.eq __rt_gpbnum_skipeol");                            // skip the comment line

    // -- scan field 0: the canonical protocol name --
    emitter.instruction("mov x13, x10");                                        // x13 = name start pointer
    emitter.label("__rt_gpbnum_namescan");
    emitter.instruction("cmp x10, x11");                                        // reached the end of the buffer?
    emitter.instruction("b.hs __rt_gpbnum_nameend");                            // end the name at end-of-buffer
    emitter.instruction("ldrb w0, [x10]");                                      // load the current name byte
    emitter.instruction("cmp w0, #0x20");                                       // is it a space?
    emitter.instruction("b.eq __rt_gpbnum_nameend");                            // end the name on whitespace
    emitter.instruction("cmp w0, #0x09");                                       // is it a tab?
    emitter.instruction("b.eq __rt_gpbnum_nameend");                            // end the name on whitespace
    emitter.instruction("cmp w0, #0x0A");                                       // is it a newline?
    emitter.instruction("b.eq __rt_gpbnum_nameend");                            // end the name at end-of-line
    emitter.instruction("cmp w0, #0x0D");                                       // is it a carriage return?
    emitter.instruction("b.eq __rt_gpbnum_nameend");                            // end the name at end-of-line
    emitter.instruction("cmp w0, #0x23");                                       // does a comment start here?
    emitter.instruction("b.eq __rt_gpbnum_nameend");                            // end the name at the comment
    emitter.instruction("add x10, x10, #1");                                    // advance to the next name byte
    emitter.instruction("b __rt_gpbnum_namescan");                              // keep scanning the name
    emitter.label("__rt_gpbnum_nameend");
    emitter.instruction("sub x14, x10, x13");                                   // x14 = name length

    // -- skip the whitespace between the name and the number --
    emitter.label("__rt_gpbnum_skipws1");
    emitter.instruction("cmp x10, x11");                                        // reached the end of the buffer?
    emitter.instruction("b.hs __rt_gpbnum_skipeol");                            // no number field: move to the next line
    emitter.instruction("ldrb w0, [x10]");                                      // load the current byte
    emitter.instruction("cmp w0, #0x20");                                       // is it a space?
    emitter.instruction("b.eq __rt_gpbnum_skipws1_adv");                        // skip the space
    emitter.instruction("cmp w0, #0x09");                                       // is it a tab?
    emitter.instruction("b.eq __rt_gpbnum_skipws1_adv");                        // skip the tab
    emitter.instruction("b __rt_gpbnum_aftername");                             // first byte after the whitespace
    emitter.label("__rt_gpbnum_skipws1_adv");
    emitter.instruction("add x10, x10, #1");                                    // advance past the whitespace byte
    emitter.instruction("b __rt_gpbnum_skipws1");                               // keep skipping whitespace
    emitter.label("__rt_gpbnum_aftername");
    emitter.instruction("ldrb w0, [x10]");                                      // load the byte after the whitespace
    emitter.instruction("cmp w0, #0x0A");                                       // end of line before a number?
    emitter.instruction("b.eq __rt_gpbnum_skipeol");                            // no number field: move to the next line
    emitter.instruction("cmp w0, #0x0D");                                       // carriage return before a number?
    emitter.instruction("b.eq __rt_gpbnum_skipeol");                            // no number field: move to the next line
    emitter.instruction("cmp w0, #0x23");                                       // comment before a number?
    emitter.instruction("b.eq __rt_gpbnum_skipeol");                            // no number field: move to the next line

    // -- parse field 1: the protocol number --
    emitter.instruction("mov x15, #0");                                         // parsed number = 0
    emitter.instruction("mov x6, #10");                                         // decimal base
    emitter.label("__rt_gpbnum_numloop");
    emitter.instruction("cmp x10, x11");                                        // reached the end of the buffer?
    emitter.instruction("b.hs __rt_gpbnum_numdone");                            // number fully parsed
    emitter.instruction("ldrb w0, [x10]");                                      // load a candidate digit byte
    emitter.instruction("cmp w0, #0x30");                                       // is the byte below ASCII '0'?
    emitter.instruction("b.lt __rt_gpbnum_numdone");                            // non-digit ends the number
    emitter.instruction("cmp w0, #0x39");                                       // is the byte above ASCII '9'?
    emitter.instruction("b.gt __rt_gpbnum_numdone");                            // non-digit ends the number
    emitter.instruction("sub w0, w0, #0x30");                                   // convert ASCII digit to its value
    emitter.instruction("madd x15, x15, x6, x0");                               // number = number * 10 + digit
    emitter.instruction("add x10, x10, #1");                                    // advance to the next digit
    emitter.instruction("b __rt_gpbnum_numloop");                               // keep parsing digits
    emitter.label("__rt_gpbnum_numdone");
    emitter.instruction("cmp x15, x12");                                        // does the entry number match the query?
    emitter.instruction("b.ne __rt_gpbnum_skipeol");                            // mismatch: move to the next line

    // -- match: copy the name into owned heap storage and return it --
    emitter.instruction("mov x1, x13");                                         // transient name pointer into the file buffer
    emitter.instruction("mov x2, x14");                                         // transient name length
    emitter.instruction("bl __rt_str_persist");                                 // duplicate into owned heap storage, x1=ptr x2=len
    emitter.instruction("ldp x29, x30, [sp], #32");                             // restore frame pointer and return address
    emitter.instruction("ret");                                                 // return the owned protocol-name string

    // -- advance the cursor past the end of the current line --
    emitter.label("__rt_gpbnum_skipeol");
    emitter.instruction("cmp x10, x11");                                        // reached the end of the buffer?
    emitter.instruction("b.hs __rt_gpbnum_notfound");                           // no trailing line: report not found
    emitter.instruction("ldrb w0, [x10]");                                      // load the current byte
    emitter.instruction("add x10, x10, #1");                                    // consume the byte
    emitter.instruction("cmp w0, #0x0A");                                       // was it the line terminator?
    emitter.instruction("b.ne __rt_gpbnum_skipeol");                            // keep skipping until the newline
    emitter.instruction("b __rt_gpbnum_line");                                  // scan the next line

    // -- consume a single blank-line byte --
    emitter.label("__rt_gpbnum_eol");
    emitter.instruction("add x10, x10, #1");                                    // consume the blank-line byte
    emitter.instruction("b __rt_gpbnum_line");                                  // scan the next line

    // -- no entry matched --
    emitter.label("__rt_gpbnum_notfound");
    emitter.instruction("mov x1, #0");                                          // a null pointer signals not found
    emitter.instruction("mov x2, #0");                                          // zero length for the not-found case
    emitter.instruction("ldp x29, x30, [sp], #32");                             // restore frame pointer and return address
    emitter.instruction("ret");                                                 // return the not-found result
}

/// Emits the Linux x86_64 stream runtime helper for getprotobynumber.
fn emit_getprotobynumber_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: getprotobynumber ---");
    emitter.label_global("__rt_getprotobynumber");

    // -- set up frame and load /etc/protocols --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("push r12");                                            // save callee-saved register (name start)
    emitter.instruction("push r13");                                            // save callee-saved register (query number)
    emitter.instruction("push r14");                                            // save callee-saved register (name length)
    emitter.instruction("push r15");                                            // save callee-saved register (parsed number)
    emitter.instruction("mov r13, rdi");                                        // r13 = query number, survives the load call
    emitter.instruction("call __rt_protoent_load");                             // read /etc/protocols, rax=buffer rdx=count
    emitter.instruction("mov r8, rax");                                         // r8 = scan cursor
    emitter.instruction("lea r9, [rax + rdx]");                                 // r9 = end-of-buffer pointer

    // -- iterate over each line --
    emitter.label("__rt_gpbnum_line");
    emitter.instruction("cmp r8, r9");                                          // reached the end of the buffer?
    emitter.instruction("jae __rt_gpbnum_notfound");                            // no more lines: report not found

    // -- skip leading spaces and tabs on the line --
    emitter.label("__rt_gpbnum_skipws0");
    emitter.instruction("cmp r8, r9");                                          // reached the end while skipping?
    emitter.instruction("jae __rt_gpbnum_notfound");                            // no more lines: report not found
    emitter.instruction("movzx eax, BYTE PTR [r8]");                            // load the current byte
    emitter.instruction("cmp eax, 0x20");                                       // is it a space?
    emitter.instruction("je __rt_gpbnum_skipws0_adv");                          // skip the space
    emitter.instruction("cmp eax, 0x09");                                       // is it a tab?
    emitter.instruction("je __rt_gpbnum_skipws0_adv");                          // skip the tab
    emitter.instruction("jmp __rt_gpbnum_linestart");                           // first non-blank byte of the line
    emitter.label("__rt_gpbnum_skipws0_adv");
    emitter.instruction("inc r8");                                              // advance past the whitespace byte
    emitter.instruction("jmp __rt_gpbnum_skipws0");                             // keep skipping leading whitespace

    // -- classify the line: blank, comment, or data --
    emitter.label("__rt_gpbnum_linestart");
    emitter.instruction("movzx eax, BYTE PTR [r8]");                            // load the first non-blank byte
    emitter.instruction("cmp eax, 0x0A");                                       // is the line empty (newline)?
    emitter.instruction("je __rt_gpbnum_eol");                                  // consume the blank line
    emitter.instruction("cmp eax, 0x0D");                                       // is it a carriage return?
    emitter.instruction("je __rt_gpbnum_eol");                                  // consume the blank line
    emitter.instruction("cmp eax, 0x23");                                       // does the line start with '#'?
    emitter.instruction("je __rt_gpbnum_skipeol");                              // skip the comment line

    // -- scan field 0: the canonical protocol name --
    emitter.instruction("mov r12, r8");                                         // r12 = name start pointer
    emitter.label("__rt_gpbnum_namescan");
    emitter.instruction("cmp r8, r9");                                          // reached the end of the buffer?
    emitter.instruction("jae __rt_gpbnum_nameend");                             // end the name at end-of-buffer
    emitter.instruction("movzx eax, BYTE PTR [r8]");                            // load the current name byte
    emitter.instruction("cmp eax, 0x20");                                       // is it a space?
    emitter.instruction("je __rt_gpbnum_nameend");                              // end the name on whitespace
    emitter.instruction("cmp eax, 0x09");                                       // is it a tab?
    emitter.instruction("je __rt_gpbnum_nameend");                              // end the name on whitespace
    emitter.instruction("cmp eax, 0x0A");                                       // is it a newline?
    emitter.instruction("je __rt_gpbnum_nameend");                              // end the name at end-of-line
    emitter.instruction("cmp eax, 0x0D");                                       // is it a carriage return?
    emitter.instruction("je __rt_gpbnum_nameend");                              // end the name at end-of-line
    emitter.instruction("cmp eax, 0x23");                                       // does a comment start here?
    emitter.instruction("je __rt_gpbnum_nameend");                              // end the name at the comment
    emitter.instruction("inc r8");                                              // advance to the next name byte
    emitter.instruction("jmp __rt_gpbnum_namescan");                            // keep scanning the name
    emitter.label("__rt_gpbnum_nameend");
    emitter.instruction("mov r14, r8");                                         // copy the name end pointer
    emitter.instruction("sub r14, r12");                                        // r14 = name length

    // -- skip the whitespace between the name and the number --
    emitter.label("__rt_gpbnum_skipws1");
    emitter.instruction("cmp r8, r9");                                          // reached the end of the buffer?
    emitter.instruction("jae __rt_gpbnum_skipeol");                             // no number field: move to the next line
    emitter.instruction("movzx eax, BYTE PTR [r8]");                            // load the current byte
    emitter.instruction("cmp eax, 0x20");                                       // is it a space?
    emitter.instruction("je __rt_gpbnum_skipws1_adv");                          // skip the space
    emitter.instruction("cmp eax, 0x09");                                       // is it a tab?
    emitter.instruction("je __rt_gpbnum_skipws1_adv");                          // skip the tab
    emitter.instruction("jmp __rt_gpbnum_aftername");                           // first byte after the whitespace
    emitter.label("__rt_gpbnum_skipws1_adv");
    emitter.instruction("inc r8");                                              // advance past the whitespace byte
    emitter.instruction("jmp __rt_gpbnum_skipws1");                             // keep skipping whitespace
    emitter.label("__rt_gpbnum_aftername");
    emitter.instruction("movzx eax, BYTE PTR [r8]");                            // load the byte after the whitespace
    emitter.instruction("cmp eax, 0x0A");                                       // end of line before a number?
    emitter.instruction("je __rt_gpbnum_skipeol");                              // no number field: move to the next line
    emitter.instruction("cmp eax, 0x0D");                                       // carriage return before a number?
    emitter.instruction("je __rt_gpbnum_skipeol");                              // no number field: move to the next line
    emitter.instruction("cmp eax, 0x23");                                       // comment before a number?
    emitter.instruction("je __rt_gpbnum_skipeol");                              // no number field: move to the next line

    // -- parse field 1: the protocol number --
    emitter.instruction("xor r15d, r15d");                                      // parsed number = 0
    emitter.label("__rt_gpbnum_numloop");
    emitter.instruction("cmp r8, r9");                                          // reached the end of the buffer?
    emitter.instruction("jae __rt_gpbnum_numdone");                             // number fully parsed
    emitter.instruction("movzx eax, BYTE PTR [r8]");                            // load a candidate digit byte
    emitter.instruction("cmp eax, 0x30");                                       // is the byte below ASCII '0'?
    emitter.instruction("jl __rt_gpbnum_numdone");                              // non-digit ends the number
    emitter.instruction("cmp eax, 0x39");                                       // is the byte above ASCII '9'?
    emitter.instruction("jg __rt_gpbnum_numdone");                              // non-digit ends the number
    emitter.instruction("sub eax, 0x30");                                       // convert ASCII digit to its value
    emitter.instruction("imul r15, r15, 10");                                   // number *= 10
    emitter.instruction("add r15, rax");                                        // number += digit
    emitter.instruction("inc r8");                                              // advance to the next digit
    emitter.instruction("jmp __rt_gpbnum_numloop");                             // keep parsing digits
    emitter.label("__rt_gpbnum_numdone");
    emitter.instruction("cmp r15, r13");                                        // does the entry number match the query?
    emitter.instruction("jne __rt_gpbnum_skipeol");                             // mismatch: move to the next line

    // -- match: copy the name into owned heap storage and return it --
    emitter.instruction("mov rax, r12");                                        // transient name pointer into the file buffer
    emitter.instruction("mov rdx, r14");                                        // transient name length
    emitter.instruction("call __rt_str_persist");                               // duplicate into owned heap storage, rax=ptr rdx=len
    emitter.instruction("jmp __rt_gpbnum_return");                              // share the common epilogue

    // -- advance the cursor past the end of the current line --
    emitter.label("__rt_gpbnum_skipeol");
    emitter.instruction("cmp r8, r9");                                          // reached the end of the buffer?
    emitter.instruction("jae __rt_gpbnum_notfound");                            // no trailing line: report not found
    emitter.instruction("movzx eax, BYTE PTR [r8]");                            // load the current byte
    emitter.instruction("inc r8");                                              // consume the byte
    emitter.instruction("cmp eax, 0x0A");                                       // was it the line terminator?
    emitter.instruction("jne __rt_gpbnum_skipeol");                             // keep skipping until the newline
    emitter.instruction("jmp __rt_gpbnum_line");                                // scan the next line

    // -- consume a single blank-line byte --
    emitter.label("__rt_gpbnum_eol");
    emitter.instruction("inc r8");                                              // consume the blank-line byte
    emitter.instruction("jmp __rt_gpbnum_line");                                // scan the next line

    // -- no entry matched --
    emitter.label("__rt_gpbnum_notfound");
    emitter.instruction("xor eax, eax");                                        // a null pointer signals not found
    emitter.instruction("xor edx, edx");                                        // zero length for the not-found case

    emitter.label("__rt_gpbnum_return");
    emitter.instruction("pop r15");                                             // restore callee-saved register
    emitter.instruction("pop r14");                                             // restore callee-saved register
    emitter.instruction("pop r13");                                             // restore callee-saved register
    emitter.instruction("pop r12");                                             // restore callee-saved register
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the protocol-name string slice
}
