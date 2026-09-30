//! Purpose:
//! Emits the runtime parser that opens a dynamic `http://` URL as a live
//! response-body descriptor without prebuffering its contents.
//!
//! Called from:
//! - Dynamic `fopen()` lowering after its runtime `http://` prefix check.
//!
//! Key details:
//! - Parsing and request construction mirror the dynamic HTTP branch used by
//!   `file_get_contents`, but this helper returns the fd from `__rt_http_open`.
//! - Inputs use elephc's string-result ABI and failures return descriptor `-1`.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits the target-specific `__rt_http_open_url` runtime entry point.
pub fn emit_http_open_url(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_http_open_url_aarch64(emitter),
        Arch::X86_64 => emit_http_open_url_x86_64(emitter),
    }
}

/// Emits dynamic HTTP URL parsing and fd opening for AArch64 targets.
fn emit_http_open_url_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: open a dynamic http URL without prebuffering ---");
    emitter.label_global("__rt_http_open_url");

    // Frame: [0]=url ptr [8]=url len [16]=host ptr [24]=host len
    //        [32]=path ptr [40]=path len [48]=addr len [64]=x29/x30.
    emitter.instruction("sub sp, sp, #80");                                     // allocate dynamic HTTP URL parsing state
    emitter.instruction("stp x29, x30, [sp, #64]");                             // preserve the caller frame and link register
    emitter.instruction("add x29, sp, #64");                                    // establish the helper frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // preserve the full URL pointer
    emitter.instruction("str x2, [sp, #8]");                                    // preserve the full URL byte length

    // -- split authority and path --
    emitter.instruction("mov x11, #7");                                         // scan immediately after the http:// prefix
    emitter.label("__rt_http_open_url_slash_scan");
    emitter.instruction("cmp x11, x2");                                         // did the scan reach the URL end?
    emitter.instruction("b.ge __rt_http_open_url_no_path");                     // synthesize slash when no path was supplied
    emitter.instruction("ldrb w12, [x1, x11]");                                 // load the next URL byte
    emitter.instruction("cmp w12, #0x2f");                                      // does this byte begin the path?
    emitter.instruction("b.eq __rt_http_open_url_have_path");                   // preserve the path slice from this slash
    emitter.instruction("add x11, x11, #1");                                    // advance the authority scan
    emitter.instruction("b __rt_http_open_url_slash_scan");                     // continue until slash or end
    emitter.label("__rt_http_open_url_no_path");
    abi::emit_symbol_address(emitter, "x12", "_fgc_url_slash");
    emitter.instruction("str x12, [sp, #32]");                                  // publish the synthesized slash pointer
    emitter.instruction("mov x12, #1");                                         // synthesized path contains one byte
    emitter.instruction("str x12, [sp, #40]");                                  // publish the synthesized path length
    emitter.instruction("ldr x11, [sp, #8]");                                   // use URL end as authority end
    emitter.instruction("b __rt_http_open_url_after_path");                     // join explicit and synthesized paths
    emitter.label("__rt_http_open_url_have_path");
    emitter.instruction("add x12, x1, x11");                                    // address the path inside the URL bytes
    emitter.instruction("str x12, [sp, #32]");                                  // preserve the path pointer
    emitter.instruction("sub x12, x2, x11");                                    // derive path length from the URL suffix
    emitter.instruction("str x12, [sp, #40]");                                  // preserve the path byte length
    emitter.label("__rt_http_open_url_after_path");

    // -- drop optional userinfo and validate authority --
    emitter.instruction("mov x13, #7");                                         // default host starts after http://
    emitter.instruction("mov x14, #7");                                         // scan authority for the last userinfo separator
    emitter.label("__rt_http_open_url_userinfo_scan");
    emitter.instruction("cmp x14, x11");                                        // did the scan reach authority end?
    emitter.instruction("b.ge __rt_http_open_url_userinfo_done");               // host start is now final
    emitter.instruction("ldrb w12, [x1, x14]");                                 // load one authority byte
    emitter.instruction("cmp w12, #0x40");                                      // is this an at-sign userinfo separator?
    emitter.instruction("b.ne __rt_http_open_url_userinfo_next");               // leave host start unchanged otherwise
    emitter.instruction("add x13, x14, #1");                                    // host begins after the latest separator
    emitter.label("__rt_http_open_url_userinfo_next");
    emitter.instruction("add x14, x14, #1");                                    // advance the userinfo scan
    emitter.instruction("b __rt_http_open_url_userinfo_scan");                  // continue through the authority
    emitter.label("__rt_http_open_url_userinfo_done");
    emitter.instruction("sub x14, x11, x13");                                   // derive Host header byte length
    emitter.instruction("cmp x14, #0");                                         // did the URL contain a non-empty host?
    emitter.instruction("b.le __rt_http_open_url_fail");                        // reject empty authorities
    emitter.instruction("cmp x14, #503");                                       // can tcp://host:80 fit the 512-byte address scratch?
    emitter.instruction("b.gt __rt_http_open_url_fail");                        // reject oversized dynamic authorities without overflowing scratch
    emitter.instruction("add x12, x1, x13");                                    // address the host inside the URL bytes
    emitter.instruction("str x12, [sp, #16]");                                  // preserve Host header pointer
    emitter.instruction("str x14, [sp, #24]");                                  // preserve Host header byte length

    // -- build tcp://host[:port] --
    abi::emit_symbol_address(emitter, "x9", "_fgc_url_addr");
    abi::emit_symbol_address(emitter, "x10", "_ftp_tcp_prefix");
    emitter.instruction("mov x15, #0");                                         // initialize address write offset
    emitter.label("__rt_http_open_url_prefix_copy");
    emitter.instruction("cmp x15, #6");                                         // were all tcp:// bytes copied?
    emitter.instruction("b.ge __rt_http_open_url_prefix_done");                 // begin host copying after the prefix
    emitter.instruction("ldrb w12, [x10, x15]");                                // load one tcp:// prefix byte
    emitter.instruction("strb w12, [x9, x15]");                                 // append the prefix byte
    emitter.instruction("add x15, x15, #1");                                    // advance the address write offset
    emitter.instruction("b __rt_http_open_url_prefix_copy");                    // copy the remaining prefix bytes
    emitter.label("__rt_http_open_url_prefix_done");
    emitter.instruction("ldr x11, [sp, #16]");                                  // reload the Host header pointer for byte copying
    emitter.instruction("mov x16, #0");                                         // initialize host copy offset
    emitter.instruction("mov x17, #0");                                         // clear the explicit-port predicate
    emitter.label("__rt_http_open_url_host_copy");
    emitter.instruction("cmp x16, x14");                                        // were all Host header bytes copied?
    emitter.instruction("b.ge __rt_http_open_url_host_done");                   // append the default port when needed
    emitter.instruction("ldrb w12, [x11, x16]");                                // load the next host byte
    emitter.instruction("cmp w12, #0x3a");                                      // does the authority carry an explicit port?
    emitter.instruction("b.ne __rt_http_open_url_no_port");                     // retain the current port predicate otherwise
    emitter.instruction("mov x17, #1");                                         // remember the explicit port separator
    emitter.label("__rt_http_open_url_no_port");
    emitter.instruction("strb w12, [x9, x15]");                                 // append the host byte to the TCP address
    emitter.instruction("add x16, x16, #1");                                    // advance host copy offset
    emitter.instruction("add x15, x15, #1");                                    // advance address write offset
    emitter.instruction("b __rt_http_open_url_host_copy");                      // continue copying host bytes
    emitter.label("__rt_http_open_url_host_done");
    emitter.instruction("cbnz x17, __rt_http_open_url_addr_done");              // preserve an explicitly supplied port
    emitter.instruction("mov w12, #0x3a");                                      // materialize the default port separator
    emitter.instruction("strb w12, [x9, x15]");                                 // append the default port separator
    emitter.instruction("add x15, x15, #1");                                    // advance the address write offset
    emitter.instruction("mov w12, #0x38");                                      // materialize default port digit eight
    emitter.instruction("strb w12, [x9, x15]");                                 // append default port digit eight
    emitter.instruction("add x15, x15, #1");                                    // advance the address write offset
    emitter.instruction("mov w12, #0x30");                                      // materialize default port digit zero
    emitter.instruction("strb w12, [x9, x15]");                                 // append default port digit zero
    emitter.instruction("add x15, x15, #1");                                    // advance the address write offset
    emitter.label("__rt_http_open_url_addr_done");
    emitter.instruction("str x15, [sp, #48]");                                  // preserve the completed TCP address length

    // -- build request and return the live response-body fd --
    emitter.instruction("ldr x0, [sp, #16]");                                   // pass Host header pointer
    emitter.instruction("ldr x1, [sp, #24]");                                   // pass Host header byte length
    emitter.instruction("ldr x2, [sp, #32]");                                   // pass request path pointer
    emitter.instruction("ldr x3, [sp, #40]");                                   // pass request path byte length
    emitter.instruction("bl __rt_http_build_request");                          // build the context-aware HTTP request
    emitter.instruction("mov x3, x0");                                          // pass the generated request byte length
    abi::emit_symbol_address(emitter, "x0", "_fgc_url_addr");
    emitter.instruction("ldr x1, [sp, #48]");                                   // pass the TCP address byte length
    abi::emit_symbol_address(emitter, "x2", "_http_req_scratch");
    emitter.instruction("bl __rt_http_open");                                   // return the live HTTP response-body descriptor
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore the caller frame and link register
    emitter.instruction("add sp, sp, #80");                                     // release URL parsing state
    emitter.instruction("ret");                                                 // return the opened descriptor or failure sentinel

    emitter.label("__rt_http_open_url_fail");
    emitter.instruction("mov x0, #-1");                                         // report an invalid dynamic HTTP URL
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore the caller frame and link register
    emitter.instruction("add sp, sp, #80");                                     // release URL parsing state
    emitter.instruction("ret");                                                 // return the failure sentinel
}

/// Emits dynamic HTTP URL parsing and fd opening for Linux x86_64.
fn emit_http_open_url_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: open a dynamic http URL without prebuffering ---");
    emitter.label_global("__rt_http_open_url");

    // Frame: [-8]=url ptr [-16]=url len [-24]=host ptr [-32]=host len
    //        [-40]=path ptr [-48]=path len [-56]=addr len
    //        [-64]=authority end [-72]=has port.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable URL parsing frame
    emitter.instruction("sub rsp, 80");                                         // reserve aligned dynamic HTTP URL state
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the full URL pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // preserve the full URL byte length

    // -- split authority and path --
    emitter.instruction("mov r8, 7");                                           // scan immediately after the http:// prefix
    emitter.label("__rt_http_open_url_slash_scan_x86");
    emitter.instruction("cmp r8, rdx");                                         // did the scan reach the URL end?
    emitter.instruction("jge __rt_http_open_url_no_path_x86");                  // synthesize slash when no path was supplied
    emitter.instruction("cmp BYTE PTR [rax + r8], 0x2f");                       // does this byte begin the path?
    emitter.instruction("je __rt_http_open_url_have_path_x86");                 // preserve the path slice from this slash
    emitter.instruction("inc r8");                                              // advance the authority scan
    emitter.instruction("jmp __rt_http_open_url_slash_scan_x86");               // continue until slash or end
    emitter.label("__rt_http_open_url_no_path_x86");
    abi::emit_symbol_address(emitter, "r10", "_fgc_url_slash");
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // publish the synthesized slash pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], 1");                         // publish the synthesized path length
    emitter.instruction("mov r8, rdx");                                         // use URL end as authority end
    emitter.instruction("jmp __rt_http_open_url_after_path_x86");               // join explicit and synthesized paths
    emitter.label("__rt_http_open_url_have_path_x86");
    emitter.instruction("lea r10, [rax + r8]");                                 // address the path inside the URL bytes
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // preserve the path pointer
    emitter.instruction("mov r10, rdx");                                        // copy the full URL byte length
    emitter.instruction("sub r10, r8");                                         // derive path length from the URL suffix
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // preserve the path byte length
    emitter.label("__rt_http_open_url_after_path_x86");
    emitter.instruction("mov QWORD PTR [rbp - 64], r8");                        // preserve authority end

    // -- drop optional userinfo and validate authority --
    emitter.instruction("mov r9, 7");                                           // default host starts after http://
    emitter.instruction("mov rcx, 7");                                          // scan authority for the latest separator
    emitter.label("__rt_http_open_url_userinfo_scan_x86");
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 64]");                       // did the scan reach authority end?
    emitter.instruction("jge __rt_http_open_url_userinfo_done_x86");            // host start is now final
    emitter.instruction("cmp BYTE PTR [rax + rcx], 0x40");                      // is this an at-sign userinfo separator?
    emitter.instruction("jne __rt_http_open_url_userinfo_next_x86");            // leave host start unchanged otherwise
    emitter.instruction("lea r9, [rcx + 1]");                                   // host begins after the latest separator
    emitter.label("__rt_http_open_url_userinfo_next_x86");
    emitter.instruction("inc rcx");                                             // advance the userinfo scan
    emitter.instruction("jmp __rt_http_open_url_userinfo_scan_x86");            // continue through the authority
    emitter.label("__rt_http_open_url_userinfo_done_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 64]");                       // reload authority end
    emitter.instruction("sub r10, r9");                                         // derive Host header byte length
    emitter.instruction("cmp r10, 0");                                          // did the URL contain a non-empty host?
    emitter.instruction("jle __rt_http_open_url_fail_x86");                     // reject empty authorities
    emitter.instruction("cmp r10, 503");                                        // can tcp://host:80 fit the 512-byte address scratch?
    emitter.instruction("jg __rt_http_open_url_fail_x86");                      // reject oversized dynamic authorities without overflowing scratch
    emitter.instruction("lea r11, [rax + r9]");                                 // address the host inside the URL bytes
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // preserve Host header pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // preserve Host header byte length

    // -- build tcp://host[:port] --
    abi::emit_symbol_address(emitter, "r10", "_fgc_url_addr");
    abi::emit_symbol_address(emitter, "r11", "_ftp_tcp_prefix");
    emitter.instruction("xor rcx, rcx");                                        // initialize address write offset
    emitter.label("__rt_http_open_url_prefix_copy_x86");
    emitter.instruction("cmp rcx, 6");                                          // were all tcp:// bytes copied?
    emitter.instruction("jge __rt_http_open_url_prefix_done_x86");              // begin host copying after the prefix
    emitter.instruction("mov r8b, BYTE PTR [r11 + rcx]");                       // load one tcp:// prefix byte
    emitter.instruction("mov BYTE PTR [r10 + rcx], r8b");                       // append the prefix byte
    emitter.instruction("inc rcx");                                             // advance the address write offset
    emitter.instruction("jmp __rt_http_open_url_prefix_copy_x86");              // copy the remaining prefix bytes
    emitter.label("__rt_http_open_url_prefix_done_x86");
    emitter.instruction("mov QWORD PTR [rbp - 72], 0");                         // clear the explicit-port predicate
    emitter.instruction("xor r8, r8");                                          // initialize host copy offset
    emitter.label("__rt_http_open_url_host_copy_x86");
    emitter.instruction("cmp r8, QWORD PTR [rbp - 32]");                        // were all Host header bytes copied?
    emitter.instruction("jge __rt_http_open_url_host_done_x86");                // append the default port when needed
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the Host header pointer
    emitter.instruction("mov r9b, BYTE PTR [r11 + r8]");                        // load the next host byte
    emitter.instruction("cmp r9b, 0x3a");                                       // does the authority carry an explicit port?
    emitter.instruction("jne __rt_http_open_url_no_port_x86");                  // retain the current port predicate otherwise
    emitter.instruction("mov QWORD PTR [rbp - 72], 1");                         // remember the explicit port separator
    emitter.label("__rt_http_open_url_no_port_x86");
    emitter.instruction("mov BYTE PTR [r10 + rcx], r9b");                       // append the host byte to the TCP address
    emitter.instruction("inc r8");                                              // advance host copy offset
    emitter.instruction("inc rcx");                                             // advance address write offset
    emitter.instruction("jmp __rt_http_open_url_host_copy_x86");                // continue copying host bytes
    emitter.label("__rt_http_open_url_host_done_x86");
    emitter.instruction("cmp QWORD PTR [rbp - 72], 0");                         // was an explicit port supplied?
    emitter.instruction("jne __rt_http_open_url_addr_done_x86");                // preserve an explicitly supplied port
    emitter.instruction("mov BYTE PTR [r10 + rcx], 0x3a");                      // append the default port separator
    emitter.instruction("inc rcx");                                             // advance the address write offset
    emitter.instruction("mov BYTE PTR [r10 + rcx], 0x38");                      // append default port digit eight
    emitter.instruction("inc rcx");                                             // advance the address write offset
    emitter.instruction("mov BYTE PTR [r10 + rcx], 0x30");                      // append default port digit zero
    emitter.instruction("inc rcx");                                             // advance the address write offset
    emitter.label("__rt_http_open_url_addr_done_x86");
    emitter.instruction("mov QWORD PTR [rbp - 56], rcx");                       // preserve the completed TCP address length

    // -- build request and return the live response-body fd --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // pass Host header pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // pass Host header byte length
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // pass request path pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // pass request path byte length
    emitter.instruction("call __rt_http_build_request");                        // build the context-aware HTTP request
    abi::emit_symbol_address(emitter, "rdi", "_fgc_url_addr");
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // pass the TCP address byte length
    abi::emit_symbol_address(emitter, "rdx", "_http_req_scratch");
    emitter.instruction("mov rcx, rax");                                        // pass the generated request byte length
    emitter.instruction("call __rt_http_open");                                 // return the live HTTP response-body descriptor
    emitter.instruction("add rsp, 80");                                         // release URL parsing state
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the opened descriptor or failure sentinel

    emitter.label("__rt_http_open_url_fail_x86");
    emitter.instruction("mov rax, -1");                                         // report an invalid dynamic HTTP URL
    emitter.instruction("add rsp, 80");                                         // release URL parsing state
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the failure sentinel
}
