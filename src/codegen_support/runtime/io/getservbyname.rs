//! Purpose:
//! Emits the `__rt_getservbyname` runtime helper, which scans the
//! `/etc/services` database for a service by name and protocol.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - Each line is `name port/proto aliases`; a match needs the query name to
//!   equal the canonical name or an alias AND the query protocol to equal the
//!   protocol part of field 1.
//! - Returns the port number, or -1 when no entry matches; the builtin emitter
//!   boxes -1 as PHP `false`.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// getservbyname: look up a service port by name and protocol.
/// Input:  AArch64 x1/x2 = service name, x3/x4 = protocol name
///         x86_64  rdi/rsi = service name, rdx/rcx = protocol name
/// Output: port number, or -1 when no entry matches
pub fn emit_getservbyname(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_getservbyname_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: getservbyname ---");
    emitter.label_global("__rt_getservbyname");

    // -- set up frame and load /etc/services --
    emitter.instruction("stp x29, x30, [sp, #-48]!");                           // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish a new frame pointer
    emitter.instruction("str x1, [sp, #16]");                                   // stash the service pointer across the load call
    emitter.instruction("str x2, [sp, #24]");                                   // stash the service length across the load call
    emitter.instruction("str x3, [sp, #32]");                                   // stash the protocol pointer across the load call
    emitter.instruction("str x4, [sp, #40]");                                   // stash the protocol length across the load call
    emitter.instruction("bl __rt_servent_load");                                // read /etc/services, x0=buffer x1=count
    emitter.instruction("mov x10, x0");                                         // x10 = scan cursor
    emitter.instruction("add x11, x0, x1");                                     // x11 = end-of-buffer pointer
    emitter.instruction("ldr x12, [sp, #16]");                                  // x12 = service pointer
    emitter.instruction("ldr x13, [sp, #24]");                                  // x13 = service length
    emitter.instruction("ldr x14, [sp, #32]");                                  // x14 = protocol pointer
    emitter.instruction("ldr x15, [sp, #40]");                                  // x15 = protocol length

    // -- iterate over each line --
    emitter.label("__rt_gsbn_line");
    emitter.instruction("cmp x10, x11");                                        // reached the end of the buffer?
    emitter.instruction("b.hs __rt_gsbn_notfound");                             // no more lines: report not found

    // -- skip leading spaces and tabs --
    emitter.label("__rt_gsbn_skipws0");
    emitter.instruction("cmp x10, x11");                                        // reached the end while skipping?
    emitter.instruction("b.hs __rt_gsbn_notfound");                             // no more lines: report not found
    emitter.instruction("ldrb w0, [x10]");                                      // load the current byte
    emitter.instruction("cmp w0, #0x20");                                       // is it a space?
    emitter.instruction("b.eq __rt_gsbn_skipws0_adv");                          // skip the space
    emitter.instruction("cmp w0, #0x09");                                       // is it a tab?
    emitter.instruction("b.eq __rt_gsbn_skipws0_adv");                          // skip the tab
    emitter.instruction("b __rt_gsbn_linestart");                               // first non-blank byte of the line
    emitter.label("__rt_gsbn_skipws0_adv");
    emitter.instruction("add x10, x10, #1");                                    // advance past the whitespace byte
    emitter.instruction("b __rt_gsbn_skipws0");                                 // keep skipping leading whitespace

    // -- classify the line: blank, comment, or data --
    emitter.label("__rt_gsbn_linestart");
    emitter.instruction("ldrb w0, [x10]");                                      // load the first non-blank byte
    emitter.instruction("cmp w0, #0x0A");                                       // is the line empty (newline)?
    emitter.instruction("b.eq __rt_gsbn_eol");                                  // consume the blank line
    emitter.instruction("cmp w0, #0x0D");                                       // is it a carriage return?
    emitter.instruction("b.eq __rt_gsbn_eol");                                  // consume the blank line
    emitter.instruction("cmp w0, #0x23");                                       // does the line start with '#'?
    emitter.instruction("b.eq __rt_gsbn_skipeol");                              // skip the comment line

    // -- field 0: scan the service name and compare it to the query --
    emitter.instruction("mov x7, #0");                                          // name-matched flag = 0
    emitter.instruction("mov x1, x10");                                         // x1 = name token start
    emitter.label("__rt_gsbn_namescan");
    emitter.instruction("cmp x10, x11");                                        // reached the end of the buffer?
    emitter.instruction("b.hs __rt_gsbn_nameend");                              // end the token at end-of-buffer
    emitter.instruction("ldrb w0, [x10]");                                      // load the current name byte
    emitter.instruction("cmp w0, #0x20");                                       // space ends the field?
    emitter.instruction("b.eq __rt_gsbn_nameend");                              // end the token
    emitter.instruction("cmp w0, #0x09");                                       // tab ends the field?
    emitter.instruction("b.eq __rt_gsbn_nameend");                              // end the token
    emitter.instruction("cmp w0, #0x0A");                                       // newline ends the field?
    emitter.instruction("b.eq __rt_gsbn_nameend");                              // end the token
    emitter.instruction("cmp w0, #0x0D");                                       // carriage return ends the field?
    emitter.instruction("b.eq __rt_gsbn_nameend");                              // end the token
    emitter.instruction("cmp w0, #0x23");                                       // comment ends the field?
    emitter.instruction("b.eq __rt_gsbn_nameend");                              // end the token
    emitter.instruction("add x10, x10, #1");                                    // advance to the next name byte
    emitter.instruction("b __rt_gsbn_namescan");                                // keep scanning the name
    emitter.label("__rt_gsbn_nameend");
    emitter.instruction("sub x2, x10, x1");                                     // x2 = name token length
    emitter.instruction("cmp x2, x13");                                         // name and service lengths must match
    emitter.instruction("b.ne __rt_gsbn_skipws1");                              // different length: not the service name
    emitter.instruction("mov x3, #0");                                          // byte compare index = 0
    emitter.label("__rt_gsbn_namecmp");
    emitter.instruction("cmp x3, x2");                                          // compared every byte?
    emitter.instruction("b.hs __rt_gsbn_namematched");                          // all bytes equal: the name matched
    emitter.instruction("ldrb w4, [x1, x3]");                                   // load a name byte
    emitter.instruction("ldrb w9, [x12, x3]");                                  // load the matching service byte
    emitter.instruction("cmp w4, w9");                                          // do the bytes differ?
    emitter.instruction("b.ne __rt_gsbn_skipws1");                              // mismatch: not the service name
    emitter.instruction("add x3, x3, #1");                                      // advance to the next byte
    emitter.instruction("b __rt_gsbn_namecmp");                                 // keep comparing
    emitter.label("__rt_gsbn_namematched");
    emitter.instruction("mov x7, #1");                                          // record that the service name matched

    // -- skip whitespace between the name and the port/proto field --
    emitter.label("__rt_gsbn_skipws1");
    emitter.instruction("cmp x10, x11");                                        // reached the end of the buffer?
    emitter.instruction("b.hs __rt_gsbn_skipeol");                              // no port field: move to the next line
    emitter.instruction("ldrb w0, [x10]");                                      // load the current byte
    emitter.instruction("cmp w0, #0x20");                                       // is it a space?
    emitter.instruction("b.eq __rt_gsbn_skipws1_adv");                          // skip the space
    emitter.instruction("cmp w0, #0x09");                                       // is it a tab?
    emitter.instruction("b.eq __rt_gsbn_skipws1_adv");                          // skip the tab
    emitter.instruction("b __rt_gsbn_portfield");                               // first byte of the port/proto field
    emitter.label("__rt_gsbn_skipws1_adv");
    emitter.instruction("add x10, x10, #1");                                    // advance past the whitespace byte
    emitter.instruction("b __rt_gsbn_skipws1");                                 // keep skipping whitespace
    emitter.label("__rt_gsbn_portfield");
    emitter.instruction("ldrb w0, [x10]");                                      // load the first port-field byte
    emitter.instruction("cmp w0, #0x0A");                                       // end of line before a port field?
    emitter.instruction("b.eq __rt_gsbn_skipeol");                              // no port field: move to the next line
    emitter.instruction("cmp w0, #0x0D");                                       // carriage return before a port field?
    emitter.instruction("b.eq __rt_gsbn_skipeol");                              // no port field: move to the next line
    emitter.instruction("cmp w0, #0x23");                                       // comment before a port field?
    emitter.instruction("b.eq __rt_gsbn_skipeol");                              // no port field: move to the next line

    // -- parse the port digits up to the '/' separator --
    emitter.instruction("mov x8, #0");                                          // parsed port = 0
    emitter.instruction("mov x9, #10");                                         // decimal base
    emitter.label("__rt_gsbn_portloop");
    emitter.instruction("cmp x10, x11");                                        // reached the end of the buffer?
    emitter.instruction("b.hs __rt_gsbn_skipeol");                              // malformed field: move to the next line
    emitter.instruction("ldrb w0, [x10]");                                      // load a port-field byte
    emitter.instruction("cmp w0, #0x2F");                                       // is it the '/' separator?
    emitter.instruction("b.eq __rt_gsbn_portdone");                             // the port number ends at '/'
    emitter.instruction("cmp w0, #0x30");                                       // is the byte below ASCII '0'?
    emitter.instruction("b.lt __rt_gsbn_skipeol");                              // malformed port digit: skip the line
    emitter.instruction("cmp w0, #0x39");                                       // is the byte above ASCII '9'?
    emitter.instruction("b.gt __rt_gsbn_skipeol");                              // malformed port digit: skip the line
    emitter.instruction("sub w0, w0, #0x30");                                   // convert ASCII digit to its value
    emitter.instruction("madd x8, x8, x9, x0");                                 // port = port * 10 + digit
    emitter.instruction("add x10, x10, #1");                                    // advance to the next digit
    emitter.instruction("b __rt_gsbn_portloop");                                // keep parsing port digits
    emitter.label("__rt_gsbn_portdone");
    emitter.instruction("add x10, x10, #1");                                    // consume the '/' separator

    // -- scan the protocol part and compare it to the query protocol --
    emitter.instruction("mov x1, x10");                                         // x1 = protocol token start
    emitter.label("__rt_gsbn_protoscan");
    emitter.instruction("cmp x10, x11");                                        // reached the end of the buffer?
    emitter.instruction("b.hs __rt_gsbn_protoend");                             // end the token at end-of-buffer
    emitter.instruction("ldrb w0, [x10]");                                      // load the current protocol byte
    emitter.instruction("cmp w0, #0x20");                                       // space ends the field?
    emitter.instruction("b.eq __rt_gsbn_protoend");                             // end the token
    emitter.instruction("cmp w0, #0x09");                                       // tab ends the field?
    emitter.instruction("b.eq __rt_gsbn_protoend");                             // end the token
    emitter.instruction("cmp w0, #0x0A");                                       // newline ends the field?
    emitter.instruction("b.eq __rt_gsbn_protoend");                             // end the token
    emitter.instruction("cmp w0, #0x0D");                                       // carriage return ends the field?
    emitter.instruction("b.eq __rt_gsbn_protoend");                             // end the token
    emitter.instruction("cmp w0, #0x23");                                       // comment ends the field?
    emitter.instruction("b.eq __rt_gsbn_protoend");                             // end the token
    emitter.instruction("add x10, x10, #1");                                    // advance to the next protocol byte
    emitter.instruction("b __rt_gsbn_protoscan");                               // keep scanning the protocol
    emitter.label("__rt_gsbn_protoend");
    emitter.instruction("sub x2, x10, x1");                                     // x2 = protocol token length
    emitter.instruction("cmp x2, x15");                                         // protocol lengths must match
    emitter.instruction("b.ne __rt_gsbn_skipeol");                              // different length: this line cannot match
    emitter.instruction("mov x3, #0");                                          // byte compare index = 0
    emitter.label("__rt_gsbn_protocmp");
    emitter.instruction("cmp x3, x2");                                          // compared every byte?
    emitter.instruction("b.hs __rt_gsbn_protook");                              // all bytes equal: the protocol matched
    emitter.instruction("ldrb w4, [x1, x3]");                                   // load a protocol byte
    emitter.instruction("ldrb w9, [x14, x3]");                                  // load the matching query-protocol byte
    emitter.instruction("cmp w4, w9");                                          // do the bytes differ?
    emitter.instruction("b.ne __rt_gsbn_skipeol");                              // protocol mismatch: skip the line
    emitter.instruction("add x3, x3, #1");                                      // advance to the next byte
    emitter.instruction("b __rt_gsbn_protocmp");                                // keep comparing
    emitter.label("__rt_gsbn_protook");
    emitter.instruction("cmp x7, #1");                                          // did the canonical name already match?
    emitter.instruction("b.eq __rt_gsbn_found");                                // name and protocol matched: return the port

    // -- protocol matched: scan the alias fields for the service name --
    emitter.label("__rt_gsbn_alias");
    emitter.instruction("cmp x10, x11");                                        // reached the end of the buffer?
    emitter.instruction("b.hs __rt_gsbn_skipeol");                              // no aliases left: move to the next line
    emitter.instruction("ldrb w0, [x10]");                                      // load the current byte
    emitter.instruction("cmp w0, #0x20");                                       // is it a space?
    emitter.instruction("b.eq __rt_gsbn_alias_adv");                            // skip the space
    emitter.instruction("cmp w0, #0x09");                                       // is it a tab?
    emitter.instruction("b.eq __rt_gsbn_alias_adv");                            // skip the tab
    emitter.instruction("cmp w0, #0x0A");                                       // end of line?
    emitter.instruction("b.eq __rt_gsbn_skipeol");                              // no more aliases: move to the next line
    emitter.instruction("cmp w0, #0x0D");                                       // carriage return ends the line?
    emitter.instruction("b.eq __rt_gsbn_skipeol");                              // no more aliases: move to the next line
    emitter.instruction("cmp w0, #0x23");                                       // does a comment start here?
    emitter.instruction("b.eq __rt_gsbn_skipeol");                              // no more aliases: move to the next line
    emitter.instruction("b __rt_gsbn_aliasscan");                               // scan the next alias token
    emitter.label("__rt_gsbn_alias_adv");
    emitter.instruction("add x10, x10, #1");                                    // advance past the whitespace byte
    emitter.instruction("b __rt_gsbn_alias");                                   // keep skipping whitespace before the alias

    // -- scan one alias token and compare it to the service name --
    emitter.label("__rt_gsbn_aliasscan");
    emitter.instruction("mov x1, x10");                                         // x1 = alias token start
    emitter.label("__rt_gsbn_aliasloop");
    emitter.instruction("cmp x10, x11");                                        // reached the end of the buffer?
    emitter.instruction("b.hs __rt_gsbn_aliasend");                             // end the token at end-of-buffer
    emitter.instruction("ldrb w0, [x10]");                                      // load the current alias byte
    emitter.instruction("cmp w0, #0x20");                                       // space ends the field?
    emitter.instruction("b.eq __rt_gsbn_aliasend");                             // end the token
    emitter.instruction("cmp w0, #0x09");                                       // tab ends the field?
    emitter.instruction("b.eq __rt_gsbn_aliasend");                             // end the token
    emitter.instruction("cmp w0, #0x0A");                                       // newline ends the field?
    emitter.instruction("b.eq __rt_gsbn_aliasend");                             // end the token
    emitter.instruction("cmp w0, #0x0D");                                       // carriage return ends the field?
    emitter.instruction("b.eq __rt_gsbn_aliasend");                             // end the token
    emitter.instruction("cmp w0, #0x23");                                       // comment ends the field?
    emitter.instruction("b.eq __rt_gsbn_aliasend");                             // end the token
    emitter.instruction("add x10, x10, #1");                                    // advance to the next alias byte
    emitter.instruction("b __rt_gsbn_aliasloop");                               // keep scanning the alias
    emitter.label("__rt_gsbn_aliasend");
    emitter.instruction("sub x2, x10, x1");                                     // x2 = alias token length
    emitter.instruction("cmp x2, x13");                                         // alias and service lengths must match
    emitter.instruction("b.ne __rt_gsbn_alias");                                // different length: try the next alias
    emitter.instruction("mov x3, #0");                                          // byte compare index = 0
    emitter.label("__rt_gsbn_aliascmp");
    emitter.instruction("cmp x3, x2");                                          // compared every byte?
    emitter.instruction("b.hs __rt_gsbn_found");                                // all bytes equal: the alias matched
    emitter.instruction("ldrb w4, [x1, x3]");                                   // load an alias byte
    emitter.instruction("ldrb w9, [x12, x3]");                                  // load the matching service byte
    emitter.instruction("cmp w4, w9");                                          // do the bytes differ?
    emitter.instruction("b.ne __rt_gsbn_alias");                                // mismatch: try the next alias
    emitter.instruction("add x3, x3, #1");                                      // advance to the next byte
    emitter.instruction("b __rt_gsbn_aliascmp");                                // keep comparing

    // -- match: return the parsed port number --
    emitter.label("__rt_gsbn_found");
    emitter.instruction("mov x0, x8");                                          // return the matched service port
    emitter.instruction("b __rt_gsbn_return");                                  // done

    // -- advance the cursor past the end of the current line --
    emitter.label("__rt_gsbn_skipeol");
    emitter.instruction("cmp x10, x11");                                        // reached the end of the buffer?
    emitter.instruction("b.hs __rt_gsbn_notfound");                             // no trailing line: report not found
    emitter.instruction("ldrb w0, [x10]");                                      // load the current byte
    emitter.instruction("add x10, x10, #1");                                    // consume the byte
    emitter.instruction("cmp w0, #0x0A");                                       // was it the line terminator?
    emitter.instruction("b.ne __rt_gsbn_skipeol");                              // keep skipping until the newline
    emitter.instruction("b __rt_gsbn_line");                                    // scan the next line

    // -- consume a single blank-line byte --
    emitter.label("__rt_gsbn_eol");
    emitter.instruction("add x10, x10, #1");                                    // consume the blank-line byte
    emitter.instruction("b __rt_gsbn_line");                                    // scan the next line

    // -- no entry matched --
    emitter.label("__rt_gsbn_notfound");
    emitter.instruction("mov x0, #-1");                                         // -1 sentinel: the builtin boxes PHP false

    emitter.label("__rt_gsbn_return");
    emitter.instruction("ldp x29, x30, [sp], #48");                             // restore frame pointer and return address
    emitter.instruction("ret");                                                 // return the port number or -1
}

/// Emits the Linux x86_64 stream runtime helper for getservbyname.
fn emit_getservbyname_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: getservbyname ---");
    emitter.label_global("__rt_getservbyname");

    // -- set up frame and load /etc/services --
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("push r12");                                            // save callee-saved register (service pointer)
    emitter.instruction("push r13");                                            // save callee-saved register (service length)
    emitter.instruction("push r14");                                            // save callee-saved register (protocol pointer)
    emitter.instruction("push r15");                                            // save callee-saved register (protocol length)
    emitter.instruction("sub rsp, 32");                                         // reserve four spill slots for the query strings
    emitter.instruction("mov QWORD PTR [rsp], rdi");                            // stash the service pointer across the load call
    emitter.instruction("mov QWORD PTR [rsp + 8], rsi");                        // stash the service length across the load call
    emitter.instruction("mov QWORD PTR [rsp + 16], rdx");                       // stash the protocol pointer across the load call
    emitter.instruction("mov QWORD PTR [rsp + 24], rcx");                       // stash the protocol length across the load call
    emitter.instruction("call __rt_servent_load");                              // read /etc/services, rax=buffer rdx=count
    emitter.instruction("mov r8, rax");                                         // r8 = scan cursor
    emitter.instruction("lea r9, [rax + rdx]");                                 // r9 = end-of-buffer pointer
    emitter.instruction("mov r12, QWORD PTR [rsp]");                            // r12 = service pointer
    emitter.instruction("mov r13, QWORD PTR [rsp + 8]");                        // r13 = service length
    emitter.instruction("mov r14, QWORD PTR [rsp + 16]");                       // r14 = protocol pointer
    emitter.instruction("mov r15, QWORD PTR [rsp + 24]");                       // r15 = protocol length

    // -- iterate over each line --
    emitter.label("__rt_gsbn_line");
    emitter.instruction("cmp r8, r9");                                          // reached the end of the buffer?
    emitter.instruction("jae __rt_gsbn_notfound");                              // no more lines: report not found

    // -- skip leading spaces and tabs --
    emitter.label("__rt_gsbn_skipws0");
    emitter.instruction("cmp r8, r9");                                          // reached the end while skipping?
    emitter.instruction("jae __rt_gsbn_notfound");                              // no more lines: report not found
    emitter.instruction("movzx eax, BYTE PTR [r8]");                            // load the current byte
    emitter.instruction("cmp eax, 0x20");                                       // is it a space?
    emitter.instruction("je __rt_gsbn_skipws0_adv");                            // skip the space
    emitter.instruction("cmp eax, 0x09");                                       // is it a tab?
    emitter.instruction("je __rt_gsbn_skipws0_adv");                            // skip the tab
    emitter.instruction("jmp __rt_gsbn_linestart");                             // first non-blank byte of the line
    emitter.label("__rt_gsbn_skipws0_adv");
    emitter.instruction("inc r8");                                              // advance past the whitespace byte
    emitter.instruction("jmp __rt_gsbn_skipws0");                               // keep skipping leading whitespace

    // -- classify the line: blank, comment, or data --
    emitter.label("__rt_gsbn_linestart");
    emitter.instruction("movzx eax, BYTE PTR [r8]");                            // load the first non-blank byte
    emitter.instruction("cmp eax, 0x0A");                                       // is the line empty (newline)?
    emitter.instruction("je __rt_gsbn_eol");                                    // consume the blank line
    emitter.instruction("cmp eax, 0x0D");                                       // is it a carriage return?
    emitter.instruction("je __rt_gsbn_eol");                                    // consume the blank line
    emitter.instruction("cmp eax, 0x23");                                       // does the line start with '#'?
    emitter.instruction("je __rt_gsbn_skipeol");                                // skip the comment line

    // -- field 0: scan the service name and compare it to the query --
    emitter.instruction("xor r10d, r10d");                                      // name-matched flag = 0
    emitter.instruction("mov rsi, r8");                                         // rsi = name token start
    emitter.label("__rt_gsbn_namescan");
    emitter.instruction("cmp r8, r9");                                          // reached the end of the buffer?
    emitter.instruction("jae __rt_gsbn_nameend");                               // end the token at end-of-buffer
    emitter.instruction("movzx eax, BYTE PTR [r8]");                            // load the current name byte
    emitter.instruction("cmp eax, 0x20");                                       // space ends the field?
    emitter.instruction("je __rt_gsbn_nameend");                                // end the token
    emitter.instruction("cmp eax, 0x09");                                       // tab ends the field?
    emitter.instruction("je __rt_gsbn_nameend");                                // end the token
    emitter.instruction("cmp eax, 0x0A");                                       // newline ends the field?
    emitter.instruction("je __rt_gsbn_nameend");                                // end the token
    emitter.instruction("cmp eax, 0x0D");                                       // carriage return ends the field?
    emitter.instruction("je __rt_gsbn_nameend");                                // end the token
    emitter.instruction("cmp eax, 0x23");                                       // comment ends the field?
    emitter.instruction("je __rt_gsbn_nameend");                                // end the token
    emitter.instruction("inc r8");                                              // advance to the next name byte
    emitter.instruction("jmp __rt_gsbn_namescan");                              // keep scanning the name
    emitter.label("__rt_gsbn_nameend");
    emitter.instruction("mov rcx, r8");                                         // copy the name token end pointer
    emitter.instruction("sub rcx, rsi");                                        // rcx = name token length
    emitter.instruction("cmp rcx, r13");                                        // name and service lengths must match
    emitter.instruction("jne __rt_gsbn_skipws1");                               // different length: not the service name
    emitter.instruction("xor edi, edi");                                        // byte compare index = 0
    emitter.label("__rt_gsbn_namecmp");
    emitter.instruction("cmp rdi, rcx");                                        // compared every byte?
    emitter.instruction("jae __rt_gsbn_namematched");                           // all bytes equal: the name matched
    emitter.instruction("movzx eax, BYTE PTR [rsi + rdi]");                     // load a name byte
    emitter.instruction("movzx edx, BYTE PTR [r12 + rdi]");                     // load the matching service byte
    emitter.instruction("cmp al, dl");                                          // do the bytes differ?
    emitter.instruction("jne __rt_gsbn_skipws1");                               // mismatch: not the service name
    emitter.instruction("inc rdi");                                             // advance to the next byte
    emitter.instruction("jmp __rt_gsbn_namecmp");                               // keep comparing
    emitter.label("__rt_gsbn_namematched");
    emitter.instruction("mov r10d, 1");                                         // record that the service name matched

    // -- skip whitespace between the name and the port/proto field --
    emitter.label("__rt_gsbn_skipws1");
    emitter.instruction("cmp r8, r9");                                          // reached the end of the buffer?
    emitter.instruction("jae __rt_gsbn_skipeol");                               // no port field: move to the next line
    emitter.instruction("movzx eax, BYTE PTR [r8]");                            // load the current byte
    emitter.instruction("cmp eax, 0x20");                                       // is it a space?
    emitter.instruction("je __rt_gsbn_skipws1_adv");                            // skip the space
    emitter.instruction("cmp eax, 0x09");                                       // is it a tab?
    emitter.instruction("je __rt_gsbn_skipws1_adv");                            // skip the tab
    emitter.instruction("jmp __rt_gsbn_portfield");                             // first byte of the port/proto field
    emitter.label("__rt_gsbn_skipws1_adv");
    emitter.instruction("inc r8");                                              // advance past the whitespace byte
    emitter.instruction("jmp __rt_gsbn_skipws1");                               // keep skipping whitespace
    emitter.label("__rt_gsbn_portfield");
    emitter.instruction("movzx eax, BYTE PTR [r8]");                            // load the first port-field byte
    emitter.instruction("cmp eax, 0x0A");                                       // end of line before a port field?
    emitter.instruction("je __rt_gsbn_skipeol");                                // no port field: move to the next line
    emitter.instruction("cmp eax, 0x0D");                                       // carriage return before a port field?
    emitter.instruction("je __rt_gsbn_skipeol");                                // no port field: move to the next line
    emitter.instruction("cmp eax, 0x23");                                       // comment before a port field?
    emitter.instruction("je __rt_gsbn_skipeol");                                // no port field: move to the next line

    // -- parse the port digits up to the '/' separator --
    emitter.instruction("xor r11d, r11d");                                      // parsed port = 0
    emitter.label("__rt_gsbn_portloop");
    emitter.instruction("cmp r8, r9");                                          // reached the end of the buffer?
    emitter.instruction("jae __rt_gsbn_skipeol");                               // malformed field: move to the next line
    emitter.instruction("movzx eax, BYTE PTR [r8]");                            // load a port-field byte
    emitter.instruction("cmp eax, 0x2F");                                       // is it the '/' separator?
    emitter.instruction("je __rt_gsbn_portdone");                               // the port number ends at '/'
    emitter.instruction("cmp eax, 0x30");                                       // is the byte below ASCII '0'?
    emitter.instruction("jl __rt_gsbn_skipeol");                                // malformed port digit: skip the line
    emitter.instruction("cmp eax, 0x39");                                       // is the byte above ASCII '9'?
    emitter.instruction("jg __rt_gsbn_skipeol");                                // malformed port digit: skip the line
    emitter.instruction("sub eax, 0x30");                                       // convert ASCII digit to its value
    emitter.instruction("imul r11, r11, 10");                                   // port *= 10
    emitter.instruction("add r11, rax");                                        // port += digit
    emitter.instruction("inc r8");                                              // advance to the next digit
    emitter.instruction("jmp __rt_gsbn_portloop");                              // keep parsing port digits
    emitter.label("__rt_gsbn_portdone");
    emitter.instruction("inc r8");                                              // consume the '/' separator

    // -- scan the protocol part and compare it to the query protocol --
    emitter.instruction("mov rsi, r8");                                         // rsi = protocol token start
    emitter.label("__rt_gsbn_protoscan");
    emitter.instruction("cmp r8, r9");                                          // reached the end of the buffer?
    emitter.instruction("jae __rt_gsbn_protoend");                              // end the token at end-of-buffer
    emitter.instruction("movzx eax, BYTE PTR [r8]");                            // load the current protocol byte
    emitter.instruction("cmp eax, 0x20");                                       // space ends the field?
    emitter.instruction("je __rt_gsbn_protoend");                               // end the token
    emitter.instruction("cmp eax, 0x09");                                       // tab ends the field?
    emitter.instruction("je __rt_gsbn_protoend");                               // end the token
    emitter.instruction("cmp eax, 0x0A");                                       // newline ends the field?
    emitter.instruction("je __rt_gsbn_protoend");                               // end the token
    emitter.instruction("cmp eax, 0x0D");                                       // carriage return ends the field?
    emitter.instruction("je __rt_gsbn_protoend");                               // end the token
    emitter.instruction("cmp eax, 0x23");                                       // comment ends the field?
    emitter.instruction("je __rt_gsbn_protoend");                               // end the token
    emitter.instruction("inc r8");                                              // advance to the next protocol byte
    emitter.instruction("jmp __rt_gsbn_protoscan");                             // keep scanning the protocol
    emitter.label("__rt_gsbn_protoend");
    emitter.instruction("mov rcx, r8");                                         // copy the protocol token end pointer
    emitter.instruction("sub rcx, rsi");                                        // rcx = protocol token length
    emitter.instruction("cmp rcx, r15");                                        // protocol lengths must match
    emitter.instruction("jne __rt_gsbn_skipeol");                               // different length: this line cannot match
    emitter.instruction("xor edi, edi");                                        // byte compare index = 0
    emitter.label("__rt_gsbn_protocmp");
    emitter.instruction("cmp rdi, rcx");                                        // compared every byte?
    emitter.instruction("jae __rt_gsbn_protook");                               // all bytes equal: the protocol matched
    emitter.instruction("movzx eax, BYTE PTR [rsi + rdi]");                     // load a protocol byte
    emitter.instruction("movzx edx, BYTE PTR [r14 + rdi]");                     // load the matching query-protocol byte
    emitter.instruction("cmp al, dl");                                          // do the bytes differ?
    emitter.instruction("jne __rt_gsbn_skipeol");                               // protocol mismatch: skip the line
    emitter.instruction("inc rdi");                                             // advance to the next byte
    emitter.instruction("jmp __rt_gsbn_protocmp");                              // keep comparing
    emitter.label("__rt_gsbn_protook");
    emitter.instruction("cmp r10d, 1");                                         // did the canonical name already match?
    emitter.instruction("je __rt_gsbn_found");                                  // name and protocol matched: return the port

    // -- protocol matched: scan the alias fields for the service name --
    emitter.label("__rt_gsbn_alias");
    emitter.instruction("cmp r8, r9");                                          // reached the end of the buffer?
    emitter.instruction("jae __rt_gsbn_skipeol");                               // no aliases left: move to the next line
    emitter.instruction("movzx eax, BYTE PTR [r8]");                            // load the current byte
    emitter.instruction("cmp eax, 0x20");                                       // is it a space?
    emitter.instruction("je __rt_gsbn_alias_adv");                              // skip the space
    emitter.instruction("cmp eax, 0x09");                                       // is it a tab?
    emitter.instruction("je __rt_gsbn_alias_adv");                              // skip the tab
    emitter.instruction("cmp eax, 0x0A");                                       // end of line?
    emitter.instruction("je __rt_gsbn_skipeol");                                // no more aliases: move to the next line
    emitter.instruction("cmp eax, 0x0D");                                       // carriage return ends the line?
    emitter.instruction("je __rt_gsbn_skipeol");                                // no more aliases: move to the next line
    emitter.instruction("cmp eax, 0x23");                                       // does a comment start here?
    emitter.instruction("je __rt_gsbn_skipeol");                                // no more aliases: move to the next line
    emitter.instruction("jmp __rt_gsbn_aliasscan");                             // scan the next alias token
    emitter.label("__rt_gsbn_alias_adv");
    emitter.instruction("inc r8");                                              // advance past the whitespace byte
    emitter.instruction("jmp __rt_gsbn_alias");                                 // keep skipping whitespace before the alias

    // -- scan one alias token and compare it to the service name --
    emitter.label("__rt_gsbn_aliasscan");
    emitter.instruction("mov rsi, r8");                                         // rsi = alias token start
    emitter.label("__rt_gsbn_aliasloop");
    emitter.instruction("cmp r8, r9");                                          // reached the end of the buffer?
    emitter.instruction("jae __rt_gsbn_aliasend");                              // end the token at end-of-buffer
    emitter.instruction("movzx eax, BYTE PTR [r8]");                            // load the current alias byte
    emitter.instruction("cmp eax, 0x20");                                       // space ends the field?
    emitter.instruction("je __rt_gsbn_aliasend");                               // end the token
    emitter.instruction("cmp eax, 0x09");                                       // tab ends the field?
    emitter.instruction("je __rt_gsbn_aliasend");                               // end the token
    emitter.instruction("cmp eax, 0x0A");                                       // newline ends the field?
    emitter.instruction("je __rt_gsbn_aliasend");                               // end the token
    emitter.instruction("cmp eax, 0x0D");                                       // carriage return ends the field?
    emitter.instruction("je __rt_gsbn_aliasend");                               // end the token
    emitter.instruction("cmp eax, 0x23");                                       // comment ends the field?
    emitter.instruction("je __rt_gsbn_aliasend");                               // end the token
    emitter.instruction("inc r8");                                              // advance to the next alias byte
    emitter.instruction("jmp __rt_gsbn_aliasloop");                             // keep scanning the alias
    emitter.label("__rt_gsbn_aliasend");
    emitter.instruction("mov rcx, r8");                                         // copy the alias token end pointer
    emitter.instruction("sub rcx, rsi");                                        // rcx = alias token length
    emitter.instruction("cmp rcx, r13");                                        // alias and service lengths must match
    emitter.instruction("jne __rt_gsbn_alias");                                 // different length: try the next alias
    emitter.instruction("xor edi, edi");                                        // byte compare index = 0
    emitter.label("__rt_gsbn_aliascmp");
    emitter.instruction("cmp rdi, rcx");                                        // compared every byte?
    emitter.instruction("jae __rt_gsbn_found");                                 // all bytes equal: the alias matched
    emitter.instruction("movzx eax, BYTE PTR [rsi + rdi]");                     // load an alias byte
    emitter.instruction("movzx edx, BYTE PTR [r12 + rdi]");                     // load the matching service byte
    emitter.instruction("cmp al, dl");                                          // do the bytes differ?
    emitter.instruction("jne __rt_gsbn_alias");                                 // mismatch: try the next alias
    emitter.instruction("inc rdi");                                             // advance to the next byte
    emitter.instruction("jmp __rt_gsbn_aliascmp");                              // keep comparing

    // -- match: return the parsed port number --
    emitter.label("__rt_gsbn_found");
    emitter.instruction("mov rax, r11");                                        // return the matched service port
    emitter.instruction("jmp __rt_gsbn_return");                                // done

    // -- advance the cursor past the end of the current line --
    emitter.label("__rt_gsbn_skipeol");
    emitter.instruction("cmp r8, r9");                                          // reached the end of the buffer?
    emitter.instruction("jae __rt_gsbn_notfound");                              // no trailing line: report not found
    emitter.instruction("movzx eax, BYTE PTR [r8]");                            // load the current byte
    emitter.instruction("inc r8");                                              // consume the byte
    emitter.instruction("cmp eax, 0x0A");                                       // was it the line terminator?
    emitter.instruction("jne __rt_gsbn_skipeol");                               // keep skipping until the newline
    emitter.instruction("jmp __rt_gsbn_line");                                  // scan the next line

    // -- consume a single blank-line byte --
    emitter.label("__rt_gsbn_eol");
    emitter.instruction("inc r8");                                              // consume the blank-line byte
    emitter.instruction("jmp __rt_gsbn_line");                                  // scan the next line

    // -- no entry matched --
    emitter.label("__rt_gsbn_notfound");
    emitter.instruction("mov rax, -1");                                         // -1 sentinel: the builtin boxes PHP false

    emitter.label("__rt_gsbn_return");
    emitter.instruction("add rsp, 32");                                         // release the query spill slots
    emitter.instruction("pop r15");                                             // restore callee-saved register
    emitter.instruction("pop r14");                                             // restore callee-saved register
    emitter.instruction("pop r13");                                             // restore callee-saved register
    emitter.instruction("pop r12");                                             // restore callee-saved register
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the port number or -1
}
