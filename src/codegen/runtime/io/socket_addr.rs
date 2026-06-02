//! Purpose:
//! Emits the `__rt_inet_addr_parse` runtime helper assembly shared by the
//! TCP socket builtins.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::io`.
//!
//! Key details:
//! - Parses an `[transport://]A.B.C.D:port` string into a packed IPv4 integer
//!   and a port number; reuses `__rt_ip2long` for the address octets.

use crate::codegen::{emit::Emitter, platform::Arch};

/// inet_addr_parse: split `[scheme://]A.B.C.D:port` into address and port.
/// Input:  x0 = string pointer, x1 = string length
/// Output: x0 = packed IPv4 integer (or -1 when invalid), x1 = port number
pub fn emit_inet_addr_parse(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_inet_addr_parse_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: inet_addr_parse ---");
    emitter.label_global("__rt_inet_addr_parse");

    // Frame: [0..16) saved regs, [16) ptr, [24) len, [32) colon, [40) packed
    //        addr, [48) address start.
    emitter.instruction("sub sp, sp, #64");                                     // frame for saved registers and parse state
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #16]");                                   // keep the original string pointer
    emitter.instruction("str x1, [sp, #24]");                                   // keep the original string length

    // -- locate the address start: skip an optional "scheme://" prefix --
    emitter.instruction("mov x3, #0");                                          // x3 = address start index
    emitter.instruction("mov x4, #0");                                          // x4 = scan index
    emitter.label("__rt_inet_addr_parse_scheme");
    emitter.instruction("add x5, x4, #2");                                      // need three bytes for "://"
    emitter.instruction("cmp x5, x1");                                          // is there room for a "://" here?
    emitter.instruction("b.ge __rt_inet_addr_parse_scheme_done");               // stop scanning near the end
    emitter.instruction("ldrb w6, [x0, x4]");                                   // load the candidate ':' byte
    emitter.instruction("cmp w6, #58");                                         // is it a ':'?
    emitter.instruction("b.ne __rt_inet_addr_parse_scheme_next");               // keep scanning otherwise
    emitter.instruction("add x7, x4, #1");                                      // index of the byte after ':'
    emitter.instruction("ldrb w6, [x0, x7]");                                   // load it
    emitter.instruction("cmp w6, #47");                                         // is it a '/'?
    emitter.instruction("b.ne __rt_inet_addr_parse_scheme_next");               // not a scheme separator
    emitter.instruction("add x7, x4, #2");                                      // index of the second byte after ':'
    emitter.instruction("ldrb w6, [x0, x7]");                                   // load it
    emitter.instruction("cmp w6, #47");                                         // is it a '/'?
    emitter.instruction("b.ne __rt_inet_addr_parse_scheme_next");               // not a scheme separator
    emitter.instruction("add x3, x4, #3");                                      // the address begins after "://"
    emitter.instruction("b __rt_inet_addr_parse_scheme_done");                  // the prefix is located
    emitter.label("__rt_inet_addr_parse_scheme_next");
    emitter.instruction("add x4, x4, #1");                                      // advance the scheme scan
    emitter.instruction("b __rt_inet_addr_parse_scheme");                       // continue scanning
    emitter.label("__rt_inet_addr_parse_scheme_done");

    // -- locate the last ':' at or after the address start: the port separator --
    emitter.instruction("mov x8, #-1");                                         // x8 = port-separator index, -1 = none
    emitter.instruction("mov x4, x3");                                          // restart scanning from the address start
    emitter.label("__rt_inet_addr_parse_colon");
    emitter.instruction("cmp x4, x1");                                          // reached the end of the string?
    emitter.instruction("b.ge __rt_inet_addr_parse_colon_done");                // stop scanning
    emitter.instruction("ldrb w6, [x0, x4]");                                   // load the current byte
    emitter.instruction("cmp w6, #58");                                         // is it a ':'?
    emitter.instruction("b.ne __rt_inet_addr_parse_colon_next");                // keep scanning otherwise
    emitter.instruction("mov x8, x4");                                          // remember this ':' as the latest separator
    emitter.label("__rt_inet_addr_parse_colon_next");
    emitter.instruction("add x4, x4, #1");                                      // advance the colon scan
    emitter.instruction("b __rt_inet_addr_parse_colon");                        // continue scanning
    emitter.label("__rt_inet_addr_parse_colon_done");
    emitter.instruction("cmn x8, #1");                                          // was a port separator found?
    emitter.instruction("b.eq __rt_inet_addr_parse_fail");                      // no ':' means an invalid address
    emitter.instruction("str x8, [sp, #32]");                                   // save the colon index across the ip2long call

    // -- parse the address octets through __rt_ip2long --
    emitter.instruction("str x3, [sp, #48]");                                   // save the address start across the calls
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the original string pointer
    emitter.instruction("add x0, x0, x3");                                      // point at the first address byte
    emitter.instruction("sub x1, x8, x3");                                      // length of the address slice
    emitter.instruction("bl __rt_ip2long");                                     // x0 = packed IPv4 integer or -1
    emitter.instruction("cmp x0, #0");                                          // did the numeric address parse?
    emitter.instruction("b.ge __rt_inet_addr_parse_addr_ok");                   // use the dotted-quad address directly

    // -- not a dotted quad: resolve the slice as a host name --
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the original string pointer
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the address start
    emitter.instruction("add x0, x0, x9");                                      // point at the first host byte
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload the colon index
    emitter.instruction("sub x1, x10, x9");                                     // length of the host slice
    emitter.instruction("bl __rt_resolve_host");                                // x0 = packed IPv4 integer or -1
    emitter.instruction("cmp x0, #0");                                          // did the host name resolve?
    emitter.instruction("b.lt __rt_inet_addr_parse_fail");                      // propagate an unresolvable host

    emitter.label("__rt_inet_addr_parse_addr_ok");
    emitter.instruction("str x0, [sp, #40]");                                   // save the packed address

    // -- parse the decimal port that follows the separator --
    emitter.instruction("ldr x0, [sp, #16]");                                   // original string pointer
    emitter.instruction("ldr x1, [sp, #24]");                                   // original string length
    emitter.instruction("ldr x8, [sp, #32]");                                   // reload the colon index
    emitter.instruction("add x4, x8, #1");                                      // first port digit index
    emitter.instruction("mov x9, #0");                                          // accumulated port value
    emitter.instruction("cmp x4, x1");                                          // is there at least one port digit?
    emitter.instruction("b.ge __rt_inet_addr_parse_fail");                      // an empty port is invalid
    emitter.label("__rt_inet_addr_parse_port");
    emitter.instruction("cmp x4, x1");                                          // consumed every port byte?
    emitter.instruction("b.ge __rt_inet_addr_parse_ok");                        // the port is complete
    emitter.instruction("ldrb w6, [x0, x4]");                                   // load the port digit
    emitter.instruction("cmp w6, #48");                                         // below ASCII '0'?
    emitter.instruction("b.lt __rt_inet_addr_parse_fail");                      // a non-digit invalidates the port
    emitter.instruction("cmp w6, #57");                                         // above ASCII '9'?
    emitter.instruction("b.gt __rt_inet_addr_parse_fail");                      // a non-digit invalidates the port
    emitter.instruction("sub w6, w6, #48");                                     // digit value
    emitter.instruction("mov x10, #10");                                        // decimal base
    emitter.instruction("mul x9, x9, x10");                                     // shift the port one decimal place
    emitter.instruction("add x9, x9, x6");                                      // add the new digit
    emitter.instruction("add x4, x4, #1");                                      // advance to the next port byte
    emitter.instruction("b __rt_inet_addr_parse_port");                         // continue parsing the port

    emitter.label("__rt_inet_addr_parse_ok");
    emitter.instruction("ldr x0, [sp, #40]");                                   // x0 = packed IPv4 integer
    emitter.instruction("mov x1, x9");                                          // x1 = parsed port
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the frame
    emitter.instruction("ret");                                                 // return the address and port

    emitter.label("__rt_inet_addr_parse_fail");
    emitter.instruction("mov x0, #-1");                                         // -1 signals an unparseable address
    emitter.instruction("mov x1, #0");                                          // no port for the failure case
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the frame
    emitter.instruction("ret");                                                 // return the failure result
}

fn emit_inet_addr_parse_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: inet_addr_parse ---");
    emitter.label_global("__rt_inet_addr_parse");

    // Frame: [rbp-8) ptr, [rbp-16) len, [rbp-24) addr start, [rbp-32) colon,
    //        [rbp-40) packed address.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 48");                                         // frame for the saved string slice and scratch
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // keep the original string pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // keep the original string length

    // -- locate the address start: skip an optional "scheme://" prefix --
    emitter.instruction("xor r8d, r8d");                                        // r8 = address start index
    emitter.instruction("xor r9d, r9d");                                        // r9 = scan index
    emitter.label("__rt_inet_addr_parse_scheme_x86");
    emitter.instruction("lea rax, [r9 + 2]");                                   // need three bytes for "://"
    emitter.instruction("cmp rax, rsi");                                        // is there room for a "://" here?
    emitter.instruction("jge __rt_inet_addr_parse_scheme_done_x86");            // stop scanning near the end
    emitter.instruction("movzx eax, BYTE PTR [rdi + r9]");                      // load the candidate ':' byte
    emitter.instruction("cmp eax, 58");                                         // is it a ':'?
    emitter.instruction("jne __rt_inet_addr_parse_scheme_next_x86");            // keep scanning otherwise
    emitter.instruction("movzx eax, BYTE PTR [rdi + r9 + 1]");                  // load the next byte
    emitter.instruction("cmp eax, 47");                                         // is it a '/'?
    emitter.instruction("jne __rt_inet_addr_parse_scheme_next_x86");            // not a scheme separator
    emitter.instruction("movzx eax, BYTE PTR [rdi + r9 + 2]");                  // load the second next byte
    emitter.instruction("cmp eax, 47");                                         // is it a '/'?
    emitter.instruction("jne __rt_inet_addr_parse_scheme_next_x86");            // not a scheme separator
    emitter.instruction("lea r8, [r9 + 3]");                                    // the address begins after "://"
    emitter.instruction("jmp __rt_inet_addr_parse_scheme_done_x86");            // the prefix is located
    emitter.label("__rt_inet_addr_parse_scheme_next_x86");
    emitter.instruction("inc r9");                                              // advance the scheme scan
    emitter.instruction("jmp __rt_inet_addr_parse_scheme_x86");                 // continue scanning
    emitter.label("__rt_inet_addr_parse_scheme_done_x86");

    // -- locate the last ':' at or after the address start: the port separator --
    emitter.instruction("mov r10, -1");                                         // r10 = port-separator index, -1 = none
    emitter.instruction("mov r9, r8");                                          // restart scanning from the address start
    emitter.label("__rt_inet_addr_parse_colon_x86");
    emitter.instruction("cmp r9, rsi");                                         // reached the end of the string?
    emitter.instruction("jge __rt_inet_addr_parse_colon_done_x86");             // stop scanning
    emitter.instruction("movzx eax, BYTE PTR [rdi + r9]");                      // load the current byte
    emitter.instruction("cmp eax, 58");                                         // is it a ':'?
    emitter.instruction("jne __rt_inet_addr_parse_colon_next_x86");             // keep scanning otherwise
    emitter.instruction("mov r10, r9");                                         // remember this ':' as the latest separator
    emitter.label("__rt_inet_addr_parse_colon_next_x86");
    emitter.instruction("inc r9");                                              // advance the colon scan
    emitter.instruction("jmp __rt_inet_addr_parse_colon_x86");                  // continue scanning
    emitter.label("__rt_inet_addr_parse_colon_done_x86");
    emitter.instruction("cmp r10, -1");                                         // was a port separator found?
    emitter.instruction("je __rt_inet_addr_parse_fail_x86");                    // no ':' means an invalid address

    // -- parse the address octets through __rt_ip2long --
    emitter.instruction("mov QWORD PTR [rbp - 24], r8");                        // stash the address start
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // stash the colon index
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the string pointer
    emitter.instruction("add rdi, r8");                                         // point at the first address byte
    emitter.instruction("mov rsi, r10");                                        // colon index
    emitter.instruction("sub rsi, r8");                                         // length of the address slice
    emitter.instruction("call __rt_ip2long");                                   // rax = packed IPv4 integer or -1
    emitter.instruction("test rax, rax");                                       // did the numeric address parse?
    emitter.instruction("jns __rt_inet_addr_parse_addr_ok_x86");                // use the dotted-quad address directly

    // -- not a dotted quad: resolve the slice as a host name --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the string pointer
    emitter.instruction("mov r8, QWORD PTR [rbp - 24]");                        // reload the address start
    emitter.instruction("add rdi, r8");                                         // point at the first host byte
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // reload the colon index
    emitter.instruction("sub rsi, r8");                                         // length of the host slice
    emitter.instruction("call __rt_resolve_host");                              // rax = packed IPv4 integer or -1
    emitter.instruction("test rax, rax");                                       // did the host name resolve?
    emitter.instruction("js __rt_inet_addr_parse_fail_x86");                    // propagate an unresolvable host

    emitter.label("__rt_inet_addr_parse_addr_ok_x86");
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // stash the packed address

    // -- parse the decimal port that follows the separator --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // original string pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // original string length
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // colon index
    emitter.instruction("lea r9, [r10 + 1]");                                   // first port digit index
    emitter.instruction("xor ecx, ecx");                                        // accumulated port value
    emitter.instruction("cmp r9, rsi");                                         // is there at least one port digit?
    emitter.instruction("jge __rt_inet_addr_parse_fail_x86");                   // an empty port is invalid
    emitter.label("__rt_inet_addr_parse_port_x86");
    emitter.instruction("cmp r9, rsi");                                         // consumed every port byte?
    emitter.instruction("jge __rt_inet_addr_parse_ok_x86");                     // the port is complete
    emitter.instruction("movzx eax, BYTE PTR [rdi + r9]");                      // load the port digit
    emitter.instruction("cmp eax, 48");                                         // below ASCII '0'?
    emitter.instruction("jl __rt_inet_addr_parse_fail_x86");                    // a non-digit invalidates the port
    emitter.instruction("cmp eax, 57");                                         // above ASCII '9'?
    emitter.instruction("jg __rt_inet_addr_parse_fail_x86");                    // a non-digit invalidates the port
    emitter.instruction("sub eax, 48");                                         // digit value
    emitter.instruction("imul rcx, rcx, 10");                                   // shift the port one decimal place
    emitter.instruction("add rcx, rax");                                        // add the new digit
    emitter.instruction("inc r9");                                              // advance to the next port byte
    emitter.instruction("jmp __rt_inet_addr_parse_port_x86");                   // continue parsing the port

    emitter.label("__rt_inet_addr_parse_ok_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // rax = packed IPv4 integer
    emitter.instruction("mov rdx, rcx");                                        // rdx = parsed port
    emitter.instruction("add rsp, 48");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the address and port

    emitter.label("__rt_inet_addr_parse_fail_x86");
    emitter.instruction("mov rax, -1");                                         // -1 signals an unparseable address
    emitter.instruction("xor edx, edx");                                        // no port for the failure case
    emitter.instruction("add rsp, 48");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the failure result
}
