//! Purpose:
//! Emits the `__rt_getservbyport` runtime helper, which scans the
//! `/etc/services` database for a service by port number and protocol.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::io`.
//!
//! Key details:
//! - Each line is `name port/proto aliases`; a match needs the query port to
//!   equal the numeric part of field 1 and the query protocol to equal its
//!   protocol part.
//! - On a match the canonical name is duplicated into owned heap storage via
//!   `__rt_str_persist`; a null pointer is returned when no entry matches.

use crate::codegen::{emit::Emitter, platform::Arch};

/// getservbyport: look up a service name by port number and protocol.
/// Input:  AArch64 x0 = port, x1/x2 = protocol name
///         x86_64  rdi = port, rsi/rdx = protocol name
/// Output: string pointer/length, or a null pointer when no entry matches
pub fn emit_getservbyport(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_getservbyport_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: getservbyport ---");
    emitter.label_global("__rt_getservbyport");

    // -- set up frame and load /etc/services --
    emitter.instruction("stp x29, x30, [sp, #-48]!");                           // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish a new frame pointer
    emitter.instruction("str x0, [sp, #16]");                                   // stash the query port across the load call
    emitter.instruction("str x1, [sp, #24]");                                   // stash the protocol pointer across the load call
    emitter.instruction("str x2, [sp, #32]");                                   // stash the protocol length across the load call
    emitter.instruction("bl __rt_servent_load");                                // read /etc/services, x0=buffer x1=count
    emitter.instruction("mov x10, x0");                                         // x10 = scan cursor
    emitter.instruction("add x11, x0, x1");                                     // x11 = end-of-buffer pointer
    emitter.instruction("ldr x12, [sp, #16]");                                  // x12 = query port
    emitter.instruction("ldr x13, [sp, #24]");                                  // x13 = protocol pointer
    emitter.instruction("ldr x14, [sp, #32]");                                  // x14 = protocol length

    // -- iterate over each line --
    emitter.label("__rt_gsbp_line");
    emitter.instruction("cmp x10, x11");                                        // reached the end of the buffer?
    emitter.instruction("b.hs __rt_gsbp_notfound");                             // no more lines: report not found

    // -- skip leading spaces and tabs --
    emitter.label("__rt_gsbp_skipws0");
    emitter.instruction("cmp x10, x11");                                        // reached the end while skipping?
    emitter.instruction("b.hs __rt_gsbp_notfound");                             // no more lines: report not found
    emitter.instruction("ldrb w0, [x10]");                                      // load the current byte
    emitter.instruction("cmp w0, #0x20");                                       // is it a space?
    emitter.instruction("b.eq __rt_gsbp_skipws0_adv");                          // skip the space
    emitter.instruction("cmp w0, #0x09");                                       // is it a tab?
    emitter.instruction("b.eq __rt_gsbp_skipws0_adv");                          // skip the tab
    emitter.instruction("b __rt_gsbp_linestart");                               // first non-blank byte of the line
    emitter.label("__rt_gsbp_skipws0_adv");
    emitter.instruction("add x10, x10, #1");                                    // advance past the whitespace byte
    emitter.instruction("b __rt_gsbp_skipws0");                                 // keep skipping leading whitespace

    // -- classify the line: blank, comment, or data --
    emitter.label("__rt_gsbp_linestart");
    emitter.instruction("ldrb w0, [x10]");                                      // load the first non-blank byte
    emitter.instruction("cmp w0, #0x0A");                                       // is the line empty (newline)?
    emitter.instruction("b.eq __rt_gsbp_eol");                                  // consume the blank line
    emitter.instruction("cmp w0, #0x0D");                                       // is it a carriage return?
    emitter.instruction("b.eq __rt_gsbp_eol");                                  // consume the blank line
    emitter.instruction("cmp w0, #0x23");                                       // does the line start with '#'?
    emitter.instruction("b.eq __rt_gsbp_skipeol");                              // skip the comment line

    // -- field 0: record the service name span --
    emitter.instruction("mov x5, x10");                                         // x5 = name token start
    emitter.label("__rt_gsbp_namescan");
    emitter.instruction("cmp x10, x11");                                        // reached the end of the buffer?
    emitter.instruction("b.hs __rt_gsbp_nameend");                              // end the token at end-of-buffer
    emitter.instruction("ldrb w0, [x10]");                                      // load the current name byte
    emitter.instruction("cmp w0, #0x20");                                       // space ends the field?
    emitter.instruction("b.eq __rt_gsbp_nameend");                              // end the token
    emitter.instruction("cmp w0, #0x09");                                       // tab ends the field?
    emitter.instruction("b.eq __rt_gsbp_nameend");                              // end the token
    emitter.instruction("cmp w0, #0x0A");                                       // newline ends the field?
    emitter.instruction("b.eq __rt_gsbp_nameend");                              // end the token
    emitter.instruction("cmp w0, #0x0D");                                       // carriage return ends the field?
    emitter.instruction("b.eq __rt_gsbp_nameend");                              // end the token
    emitter.instruction("cmp w0, #0x23");                                       // comment ends the field?
    emitter.instruction("b.eq __rt_gsbp_nameend");                              // end the token
    emitter.instruction("add x10, x10, #1");                                    // advance to the next name byte
    emitter.instruction("b __rt_gsbp_namescan");                                // keep scanning the name
    emitter.label("__rt_gsbp_nameend");
    emitter.instruction("sub x6, x10, x5");                                     // x6 = name token length

    // -- skip whitespace between the name and the port/proto field --
    emitter.label("__rt_gsbp_skipws1");
    emitter.instruction("cmp x10, x11");                                        // reached the end of the buffer?
    emitter.instruction("b.hs __rt_gsbp_skipeol");                              // no port field: move to the next line
    emitter.instruction("ldrb w0, [x10]");                                      // load the current byte
    emitter.instruction("cmp w0, #0x20");                                       // is it a space?
    emitter.instruction("b.eq __rt_gsbp_skipws1_adv");                          // skip the space
    emitter.instruction("cmp w0, #0x09");                                       // is it a tab?
    emitter.instruction("b.eq __rt_gsbp_skipws1_adv");                          // skip the tab
    emitter.instruction("b __rt_gsbp_portfield");                               // first byte of the port/proto field
    emitter.label("__rt_gsbp_skipws1_adv");
    emitter.instruction("add x10, x10, #1");                                    // advance past the whitespace byte
    emitter.instruction("b __rt_gsbp_skipws1");                                 // keep skipping whitespace
    emitter.label("__rt_gsbp_portfield");
    emitter.instruction("ldrb w0, [x10]");                                      // load the first port-field byte
    emitter.instruction("cmp w0, #0x0A");                                       // end of line before a port field?
    emitter.instruction("b.eq __rt_gsbp_skipeol");                              // no port field: move to the next line
    emitter.instruction("cmp w0, #0x0D");                                       // carriage return before a port field?
    emitter.instruction("b.eq __rt_gsbp_skipeol");                              // no port field: move to the next line
    emitter.instruction("cmp w0, #0x23");                                       // comment before a port field?
    emitter.instruction("b.eq __rt_gsbp_skipeol");                              // no port field: move to the next line

    // -- parse the port digits up to the '/' separator --
    emitter.instruction("mov x8, #0");                                          // parsed port = 0
    emitter.instruction("mov x9, #10");                                         // decimal base
    emitter.label("__rt_gsbp_portloop");
    emitter.instruction("cmp x10, x11");                                        // reached the end of the buffer?
    emitter.instruction("b.hs __rt_gsbp_skipeol");                              // malformed field: move to the next line
    emitter.instruction("ldrb w0, [x10]");                                      // load a port-field byte
    emitter.instruction("cmp w0, #0x2F");                                       // is it the '/' separator?
    emitter.instruction("b.eq __rt_gsbp_portdone");                             // the port number ends at '/'
    emitter.instruction("cmp w0, #0x30");                                       // is the byte below ASCII '0'?
    emitter.instruction("b.lt __rt_gsbp_skipeol");                              // malformed port digit: skip the line
    emitter.instruction("cmp w0, #0x39");                                       // is the byte above ASCII '9'?
    emitter.instruction("b.gt __rt_gsbp_skipeol");                              // malformed port digit: skip the line
    emitter.instruction("sub w0, w0, #0x30");                                   // convert ASCII digit to its value
    emitter.instruction("madd x8, x8, x9, x0");                                 // port = port * 10 + digit
    emitter.instruction("add x10, x10, #1");                                    // advance to the next digit
    emitter.instruction("b __rt_gsbp_portloop");                                // keep parsing port digits
    emitter.label("__rt_gsbp_portdone");
    emitter.instruction("add x10, x10, #1");                                    // consume the '/' separator
    emitter.instruction("cmp x8, x12");                                         // does the entry port match the query?
    emitter.instruction("b.ne __rt_gsbp_skipeol");                              // port mismatch: move to the next line

    // -- scan the protocol part and compare it to the query protocol --
    emitter.instruction("mov x1, x10");                                         // x1 = protocol token start
    emitter.label("__rt_gsbp_protoscan");
    emitter.instruction("cmp x10, x11");                                        // reached the end of the buffer?
    emitter.instruction("b.hs __rt_gsbp_protoend");                             // end the token at end-of-buffer
    emitter.instruction("ldrb w0, [x10]");                                      // load the current protocol byte
    emitter.instruction("cmp w0, #0x20");                                       // space ends the field?
    emitter.instruction("b.eq __rt_gsbp_protoend");                             // end the token
    emitter.instruction("cmp w0, #0x09");                                       // tab ends the field?
    emitter.instruction("b.eq __rt_gsbp_protoend");                             // end the token
    emitter.instruction("cmp w0, #0x0A");                                       // newline ends the field?
    emitter.instruction("b.eq __rt_gsbp_protoend");                             // end the token
    emitter.instruction("cmp w0, #0x0D");                                       // carriage return ends the field?
    emitter.instruction("b.eq __rt_gsbp_protoend");                             // end the token
    emitter.instruction("cmp w0, #0x23");                                       // comment ends the field?
    emitter.instruction("b.eq __rt_gsbp_protoend");                             // end the token
    emitter.instruction("add x10, x10, #1");                                    // advance to the next protocol byte
    emitter.instruction("b __rt_gsbp_protoscan");                               // keep scanning the protocol
    emitter.label("__rt_gsbp_protoend");
    emitter.instruction("sub x2, x10, x1");                                     // x2 = protocol token length
    emitter.instruction("cmp x2, x14");                                         // protocol lengths must match
    emitter.instruction("b.ne __rt_gsbp_skipeol");                              // different length: move to the next line
    emitter.instruction("mov x3, #0");                                          // byte compare index = 0
    emitter.label("__rt_gsbp_protocmp");
    emitter.instruction("cmp x3, x2");                                          // compared every byte?
    emitter.instruction("b.hs __rt_gsbp_found");                                // all bytes equal: the entry matched
    emitter.instruction("ldrb w4, [x1, x3]");                                   // load a protocol byte
    emitter.instruction("ldrb w9, [x13, x3]");                                  // load the matching query-protocol byte
    emitter.instruction("cmp w4, w9");                                          // do the bytes differ?
    emitter.instruction("b.ne __rt_gsbp_skipeol");                              // protocol mismatch: move to the next line
    emitter.instruction("add x3, x3, #1");                                      // advance to the next byte
    emitter.instruction("b __rt_gsbp_protocmp");                                // keep comparing

    // -- match: duplicate the service name into owned heap storage --
    emitter.label("__rt_gsbp_found");
    emitter.instruction("mov x1, x5");                                          // transient name pointer into the file buffer
    emitter.instruction("mov x2, x6");                                          // transient name length
    emitter.instruction("bl __rt_str_persist");                                 // duplicate into owned heap storage, x1=ptr x2=len
    emitter.instruction("ldp x29, x30, [sp], #48");                             // restore frame pointer and return address
    emitter.instruction("ret");                                                 // return the owned service-name string

    // -- advance the cursor past the end of the current line --
    emitter.label("__rt_gsbp_skipeol");
    emitter.instruction("cmp x10, x11");                                        // reached the end of the buffer?
    emitter.instruction("b.hs __rt_gsbp_notfound");                             // no trailing line: report not found
    emitter.instruction("ldrb w0, [x10]");                                      // load the current byte
    emitter.instruction("add x10, x10, #1");                                    // consume the byte
    emitter.instruction("cmp w0, #0x0A");                                       // was it the line terminator?
    emitter.instruction("b.ne __rt_gsbp_skipeol");                              // keep skipping until the newline
    emitter.instruction("b __rt_gsbp_line");                                    // scan the next line

    // -- consume a single blank-line byte --
    emitter.label("__rt_gsbp_eol");
    emitter.instruction("add x10, x10, #1");                                    // consume the blank-line byte
    emitter.instruction("b __rt_gsbp_line");                                    // scan the next line

    // -- no entry matched --
    emitter.label("__rt_gsbp_notfound");
    emitter.instruction("mov x1, #0");                                          // a null pointer signals not found
    emitter.instruction("mov x2, #0");                                          // zero length for the not-found case
    emitter.instruction("ldp x29, x30, [sp], #48");                             // restore frame pointer and return address
    emitter.instruction("ret");                                                 // return the not-found result
}

fn emit_getservbyport_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: getservbyport ---");
    emitter.label_global("__rt_getservbyport");

    // -- set up frame and load /etc/services --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("push r12");                                            // save callee-saved register (query port)
    emitter.instruction("push r13");                                            // save callee-saved register (protocol pointer)
    emitter.instruction("push r14");                                            // save callee-saved register (protocol length)
    emitter.instruction("push r15");                                            // save callee-saved register (name start)
    emitter.instruction("sub rsp, 32");                                         // reserve spill slots for the query arguments
    emitter.instruction("mov QWORD PTR [rsp], rdi");                            // stash the query port across the load call
    emitter.instruction("mov QWORD PTR [rsp + 8], rsi");                        // stash the protocol pointer across the load call
    emitter.instruction("mov QWORD PTR [rsp + 16], rdx");                       // stash the protocol length across the load call
    emitter.instruction("call __rt_servent_load");                              // read /etc/services, rax=buffer rdx=count
    emitter.instruction("mov r8, rax");                                         // r8 = scan cursor
    emitter.instruction("lea r9, [rax + rdx]");                                 // r9 = end-of-buffer pointer
    emitter.instruction("mov r12, QWORD PTR [rsp]");                            // r12 = query port
    emitter.instruction("mov r13, QWORD PTR [rsp + 8]");                        // r13 = protocol pointer
    emitter.instruction("mov r14, QWORD PTR [rsp + 16]");                       // r14 = protocol length

    // -- iterate over each line --
    emitter.label("__rt_gsbp_line");
    emitter.instruction("cmp r8, r9");                                          // reached the end of the buffer?
    emitter.instruction("jae __rt_gsbp_notfound");                              // no more lines: report not found

    // -- skip leading spaces and tabs --
    emitter.label("__rt_gsbp_skipws0");
    emitter.instruction("cmp r8, r9");                                          // reached the end while skipping?
    emitter.instruction("jae __rt_gsbp_notfound");                              // no more lines: report not found
    emitter.instruction("movzx eax, BYTE PTR [r8]");                            // load the current byte
    emitter.instruction("cmp eax, 0x20");                                       // is it a space?
    emitter.instruction("je __rt_gsbp_skipws0_adv");                            // skip the space
    emitter.instruction("cmp eax, 0x09");                                       // is it a tab?
    emitter.instruction("je __rt_gsbp_skipws0_adv");                            // skip the tab
    emitter.instruction("jmp __rt_gsbp_linestart");                             // first non-blank byte of the line
    emitter.label("__rt_gsbp_skipws0_adv");
    emitter.instruction("inc r8");                                              // advance past the whitespace byte
    emitter.instruction("jmp __rt_gsbp_skipws0");                               // keep skipping leading whitespace

    // -- classify the line: blank, comment, or data --
    emitter.label("__rt_gsbp_linestart");
    emitter.instruction("movzx eax, BYTE PTR [r8]");                            // load the first non-blank byte
    emitter.instruction("cmp eax, 0x0A");                                       // is the line empty (newline)?
    emitter.instruction("je __rt_gsbp_eol");                                    // consume the blank line
    emitter.instruction("cmp eax, 0x0D");                                       // is it a carriage return?
    emitter.instruction("je __rt_gsbp_eol");                                    // consume the blank line
    emitter.instruction("cmp eax, 0x23");                                       // does the line start with '#'?
    emitter.instruction("je __rt_gsbp_skipeol");                                // skip the comment line

    // -- field 0: record the service name span --
    emitter.instruction("mov r15, r8");                                         // r15 = name token start
    emitter.label("__rt_gsbp_namescan");
    emitter.instruction("cmp r8, r9");                                          // reached the end of the buffer?
    emitter.instruction("jae __rt_gsbp_nameend");                               // end the token at end-of-buffer
    emitter.instruction("movzx eax, BYTE PTR [r8]");                            // load the current name byte
    emitter.instruction("cmp eax, 0x20");                                       // space ends the field?
    emitter.instruction("je __rt_gsbp_nameend");                                // end the token
    emitter.instruction("cmp eax, 0x09");                                       // tab ends the field?
    emitter.instruction("je __rt_gsbp_nameend");                                // end the token
    emitter.instruction("cmp eax, 0x0A");                                       // newline ends the field?
    emitter.instruction("je __rt_gsbp_nameend");                                // end the token
    emitter.instruction("cmp eax, 0x0D");                                       // carriage return ends the field?
    emitter.instruction("je __rt_gsbp_nameend");                                // end the token
    emitter.instruction("cmp eax, 0x23");                                       // comment ends the field?
    emitter.instruction("je __rt_gsbp_nameend");                                // end the token
    emitter.instruction("inc r8");                                              // advance to the next name byte
    emitter.instruction("jmp __rt_gsbp_namescan");                              // keep scanning the name
    emitter.label("__rt_gsbp_nameend");
    emitter.instruction("mov r10, r8");                                         // copy the name token end pointer
    emitter.instruction("sub r10, r15");                                        // r10 = name token length

    // -- skip whitespace between the name and the port/proto field --
    emitter.label("__rt_gsbp_skipws1");
    emitter.instruction("cmp r8, r9");                                          // reached the end of the buffer?
    emitter.instruction("jae __rt_gsbp_skipeol");                               // no port field: move to the next line
    emitter.instruction("movzx eax, BYTE PTR [r8]");                            // load the current byte
    emitter.instruction("cmp eax, 0x20");                                       // is it a space?
    emitter.instruction("je __rt_gsbp_skipws1_adv");                            // skip the space
    emitter.instruction("cmp eax, 0x09");                                       // is it a tab?
    emitter.instruction("je __rt_gsbp_skipws1_adv");                            // skip the tab
    emitter.instruction("jmp __rt_gsbp_portfield");                             // first byte of the port/proto field
    emitter.label("__rt_gsbp_skipws1_adv");
    emitter.instruction("inc r8");                                              // advance past the whitespace byte
    emitter.instruction("jmp __rt_gsbp_skipws1");                               // keep skipping whitespace
    emitter.label("__rt_gsbp_portfield");
    emitter.instruction("movzx eax, BYTE PTR [r8]");                            // load the first port-field byte
    emitter.instruction("cmp eax, 0x0A");                                       // end of line before a port field?
    emitter.instruction("je __rt_gsbp_skipeol");                                // no port field: move to the next line
    emitter.instruction("cmp eax, 0x0D");                                       // carriage return before a port field?
    emitter.instruction("je __rt_gsbp_skipeol");                                // no port field: move to the next line
    emitter.instruction("cmp eax, 0x23");                                       // comment before a port field?
    emitter.instruction("je __rt_gsbp_skipeol");                                // no port field: move to the next line

    // -- parse the port digits up to the '/' separator --
    emitter.instruction("xor r11d, r11d");                                      // parsed port = 0
    emitter.label("__rt_gsbp_portloop");
    emitter.instruction("cmp r8, r9");                                          // reached the end of the buffer?
    emitter.instruction("jae __rt_gsbp_skipeol");                               // malformed field: move to the next line
    emitter.instruction("movzx eax, BYTE PTR [r8]");                            // load a port-field byte
    emitter.instruction("cmp eax, 0x2F");                                       // is it the '/' separator?
    emitter.instruction("je __rt_gsbp_portdone");                               // the port number ends at '/'
    emitter.instruction("cmp eax, 0x30");                                       // is the byte below ASCII '0'?
    emitter.instruction("jl __rt_gsbp_skipeol");                                // malformed port digit: skip the line
    emitter.instruction("cmp eax, 0x39");                                       // is the byte above ASCII '9'?
    emitter.instruction("jg __rt_gsbp_skipeol");                                // malformed port digit: skip the line
    emitter.instruction("sub eax, 0x30");                                       // convert ASCII digit to its value
    emitter.instruction("imul r11, r11, 10");                                   // port *= 10
    emitter.instruction("add r11, rax");                                        // port += digit
    emitter.instruction("inc r8");                                              // advance to the next digit
    emitter.instruction("jmp __rt_gsbp_portloop");                              // keep parsing port digits
    emitter.label("__rt_gsbp_portdone");
    emitter.instruction("inc r8");                                              // consume the '/' separator
    emitter.instruction("cmp r11, r12");                                        // does the entry port match the query?
    emitter.instruction("jne __rt_gsbp_skipeol");                               // port mismatch: move to the next line

    // -- scan the protocol part and compare it to the query protocol --
    emitter.instruction("mov rsi, r8");                                         // rsi = protocol token start
    emitter.label("__rt_gsbp_protoscan");
    emitter.instruction("cmp r8, r9");                                          // reached the end of the buffer?
    emitter.instruction("jae __rt_gsbp_protoend");                              // end the token at end-of-buffer
    emitter.instruction("movzx eax, BYTE PTR [r8]");                            // load the current protocol byte
    emitter.instruction("cmp eax, 0x20");                                       // space ends the field?
    emitter.instruction("je __rt_gsbp_protoend");                               // end the token
    emitter.instruction("cmp eax, 0x09");                                       // tab ends the field?
    emitter.instruction("je __rt_gsbp_protoend");                               // end the token
    emitter.instruction("cmp eax, 0x0A");                                       // newline ends the field?
    emitter.instruction("je __rt_gsbp_protoend");                               // end the token
    emitter.instruction("cmp eax, 0x0D");                                       // carriage return ends the field?
    emitter.instruction("je __rt_gsbp_protoend");                               // end the token
    emitter.instruction("cmp eax, 0x23");                                       // comment ends the field?
    emitter.instruction("je __rt_gsbp_protoend");                               // end the token
    emitter.instruction("inc r8");                                              // advance to the next protocol byte
    emitter.instruction("jmp __rt_gsbp_protoscan");                             // keep scanning the protocol
    emitter.label("__rt_gsbp_protoend");
    emitter.instruction("mov rcx, r8");                                         // copy the protocol token end pointer
    emitter.instruction("sub rcx, rsi");                                        // rcx = protocol token length
    emitter.instruction("cmp rcx, r14");                                        // protocol lengths must match
    emitter.instruction("jne __rt_gsbp_skipeol");                               // different length: move to the next line
    emitter.instruction("xor edi, edi");                                        // byte compare index = 0
    emitter.label("__rt_gsbp_protocmp");
    emitter.instruction("cmp rdi, rcx");                                        // compared every byte?
    emitter.instruction("jae __rt_gsbp_found");                                 // all bytes equal: the entry matched
    emitter.instruction("movzx eax, BYTE PTR [rsi + rdi]");                     // load a protocol byte
    emitter.instruction("movzx edx, BYTE PTR [r13 + rdi]");                     // load the matching query-protocol byte
    emitter.instruction("cmp al, dl");                                          // do the bytes differ?
    emitter.instruction("jne __rt_gsbp_skipeol");                               // protocol mismatch: move to the next line
    emitter.instruction("inc rdi");                                             // advance to the next byte
    emitter.instruction("jmp __rt_gsbp_protocmp");                              // keep comparing

    // -- match: duplicate the service name into owned heap storage --
    emitter.label("__rt_gsbp_found");
    emitter.instruction("mov rax, r15");                                        // transient name pointer into the file buffer
    emitter.instruction("mov rdx, r10");                                        // transient name length
    emitter.instruction("call __rt_str_persist");                               // duplicate into owned heap storage, rax=ptr rdx=len
    emitter.instruction("jmp __rt_gsbp_return");                                // share the common epilogue

    // -- advance the cursor past the end of the current line --
    emitter.label("__rt_gsbp_skipeol");
    emitter.instruction("cmp r8, r9");                                          // reached the end of the buffer?
    emitter.instruction("jae __rt_gsbp_notfound");                              // no trailing line: report not found
    emitter.instruction("movzx eax, BYTE PTR [r8]");                            // load the current byte
    emitter.instruction("inc r8");                                              // consume the byte
    emitter.instruction("cmp eax, 0x0A");                                       // was it the line terminator?
    emitter.instruction("jne __rt_gsbp_skipeol");                               // keep skipping until the newline
    emitter.instruction("jmp __rt_gsbp_line");                                  // scan the next line

    // -- consume a single blank-line byte --
    emitter.label("__rt_gsbp_eol");
    emitter.instruction("inc r8");                                              // consume the blank-line byte
    emitter.instruction("jmp __rt_gsbp_line");                                  // scan the next line

    // -- no entry matched --
    emitter.label("__rt_gsbp_notfound");
    emitter.instruction("xor eax, eax");                                        // a null pointer signals not found
    emitter.instruction("xor edx, edx");                                        // zero length for the not-found case

    emitter.label("__rt_gsbp_return");
    emitter.instruction("add rsp, 32");                                         // release the query spill slots
    emitter.instruction("pop r15");                                             // restore callee-saved register
    emitter.instruction("pop r14");                                             // restore callee-saved register
    emitter.instruction("pop r13");                                             // restore callee-saved register
    emitter.instruction("pop r12");                                             // restore callee-saved register
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the service-name string or null
}
