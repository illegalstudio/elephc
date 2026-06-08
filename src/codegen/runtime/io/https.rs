//! Purpose:
//! Emits the `https://` wrapper runtime helper `__rt_https_open`.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::io`.
//! - `__rt_https_open` is the entry point invoked by the `https://` `fopen` lowering.
//!
//! Key details:
//! - v1 issues a single HTTP/1.0 `GET` over a TLS-secured TCP connection: it
//!   establishes the TLS session through elephc-tls, sends the compile-time-built
//!   request, reads the whole response until the server closes, locates the
//!   `CRLFCRLF` header/body separator, and copies the body into an anonymous
//!   temp file whose descriptor is the readable stream.
//! - The response is buffered in the 1 MiB `_https_resp_buf`; a larger response
//!   is truncated. HTTP/1.0 + `Connection: close` keeps the body close-framed.
//! - All elephc-tls C entry points are invoked through the runtime function
//!   pointers (`_elephc_tls_*_fn`) so the shared runtime carries no direct
//!   elephc-tls symbol reference — only programs that actually open https URLs
//!   pull in `-lelephc_tls` at link time.

use crate::codegen::{abi, emit::Emitter, platform::Arch};

/// Capacity of the `_https_resp_buf` response buffer, in bytes.
const HTTPS_RESP_BUF_SIZE: i64 = 1048576;

/// Emits the `__rt_https_open` runtime helper.
/// Input:  AArch64 x0/x1 = host, x2 = port, x3/x4 = request.
///         x86_64  rdi/rsi = host, rdx = port, rcx/r8 = request.
/// Output: a readable descriptor for the response body, or -1 on failure.
pub fn emit_https(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_https_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: https_open ---");
    emitter.label_global("__rt_https_open");

    // Frame (80 bytes): [0]=tls handle [8]=req ptr [16]=req len
    //                   [24]=response len [32]=body start [40]=temp fd.
    emitter.instruction("sub sp, sp, #80");                                     // allocate the helper frame
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish the helper frame pointer
    emitter.instruction("str x3, [sp, #8]");                                    // save the HTTP request pointer
    emitter.instruction("str x4, [sp, #16]");                                   // save the HTTP request length

    // -- elephc_tls_connect(host, host_len, port, cafile_ptr, cafile_len) --
    //    Pick the connect variant from the ssl context: ssl.cafile (custom CA
    //    bundle) wins, else ssl.verify_peer = "0" (insecure), else the secure
    //    default. The call always passes the cafile args; the secure/insecure
    //    variants ignore them, so one call site serves all three.
    emitter.instruction("stp x0, x1, [sp, #-16]!");                             // save host_ptr/host_len across the context lookups
    emitter.instruction("str x2, [sp, #-16]!");                                 // save port (16 bytes for stack alignment)
    emitter.instruction("sub sp, sp, #16");                                     // string-lookup out slots: [sp+0]=ptr, [sp+8]=len
    // ssl.cafile lookup — a custom CA bundle path
    emitter.instruction("str xzr, [sp, #0]");                                   // out ptr default
    emitter.instruction("str xzr, [sp, #8]");                                   // out len default
    abi::emit_symbol_address(emitter, "x0", "_ssl_key_str");
    emitter.instruction("mov x1, #3");                                          // strlen("ssl")
    abi::emit_symbol_address(emitter, "x2", "_ssl_cafile_key_str");
    emitter.instruction("mov x3, #6");                                          // strlen("cafile")
    emitter.instruction("mov x4, sp");                                          // out_ptr_addr
    emitter.instruction("add x5, sp, #8");                                      // out_len_addr
    emitter.instruction("bl __rt_get_string_context_option");                   // x0 = 1 on hit
    emitter.instruction("cbz x0, __rt_https_open_no_cafile_aarch64");           // no cafile → verify_peer dispatch
    emitter.instruction("ldr x3, [sp, #0]");                                    // cafile path ptr → connect arg 3
    emitter.instruction("ldr x4, [sp, #8]");                                    // cafile path len → connect arg 4
    abi::emit_symbol_address(emitter, "x9", "_elephc_tls_connect_cafile_fn");
    emitter.instruction("ldr x9, [x9]");                                        // cafile connect variant address
    emitter.instruction("b __rt_https_open_have_fn_aarch64");                   // continue at target label
    emitter.label("__rt_https_open_no_cafile_aarch64");
    // ssl.capath lookup — a directory of CA certificates (checked after cafile)
    emitter.instruction("str xzr, [sp, #0]");                                   // reset out ptr
    emitter.instruction("str xzr, [sp, #8]");                                   // reset out len
    abi::emit_symbol_address(emitter, "x0", "_ssl_key_str");
    emitter.instruction("mov x1, #3");                                          // strlen("ssl")
    abi::emit_symbol_address(emitter, "x2", "_ssl_capath_key_str");
    emitter.instruction("mov x3, #6");                                          // strlen("capath")
    emitter.instruction("mov x4, sp");                                          // out_ptr_addr
    emitter.instruction("add x5, sp, #8");                                      // out_len_addr
    emitter.instruction("bl __rt_get_string_context_option");                   // x0 = 1 on hit
    emitter.instruction("cbz x0, __rt_https_open_no_capath_aarch64");           // no capath → verify_peer dispatch
    emitter.instruction("ldr x3, [sp, #0]");                                    // capath dir ptr → connect arg 3
    emitter.instruction("ldr x4, [sp, #8]");                                    // capath dir len → connect arg 4
    abi::emit_symbol_address(emitter, "x9", "_elephc_tls_connect_capath_fn");
    emitter.instruction("ldr x9, [x9]");                                        // capath connect variant address
    emitter.instruction("b __rt_https_open_have_fn_aarch64");                   // continue at target label
    emitter.label("__rt_https_open_no_capath_aarch64");
    // ssl.verify_peer = "0" / ssl.allow_self_signed / ssl.verify_peer_name = "0"
    //   → insecure variant (encrypted, peer identity relaxed); else secure default
    emitter.instruction("str xzr, [sp, #0]");                                   // reset out ptr
    emitter.instruction("str xzr, [sp, #8]");                                   // reset out len
    abi::emit_symbol_address(emitter, "x0", "_ssl_key_str");
    emitter.instruction("mov x1, #3");                                          // strlen("ssl")
    abi::emit_symbol_address(emitter, "x2", "_ssl_verify_peer_key_str");
    emitter.instruction("mov x3, #11");                                         // strlen("verify_peer")
    emitter.instruction("mov x4, sp");                                          // out_ptr_addr
    emitter.instruction("add x5, sp, #8");                                      // out_len_addr
    emitter.instruction("bl __rt_get_string_context_option");                   // x0 = 1 on hit
    emitter.instruction("cbz x0, __rt_https_open_self_signed_aarch64");         // verify_peer miss → check allow_self_signed
    emitter.instruction("ldr x9, [sp, #0]");                                    // verify_peer string ptr
    emitter.instruction("ldrb w10, [x9]");                                      // first byte
    emitter.instruction("cmp w10, #48");                                        // '0'?
    emitter.instruction("b.eq __rt_https_open_insecure_aarch64");               // "0" → relaxed (insecure) variant
    // ssl.allow_self_signed present (any value) → relaxed peer verification
    emitter.label("__rt_https_open_self_signed_aarch64");
    emitter.instruction("str xzr, [sp, #0]");                                   // reset out ptr
    emitter.instruction("str xzr, [sp, #8]");                                   // reset out len
    abi::emit_symbol_address(emitter, "x0", "_ssl_key_str");
    emitter.instruction("mov x1, #3");                                          // strlen("ssl")
    abi::emit_symbol_address(emitter, "x2", "_ssl_allow_self_signed_key_str");
    emitter.instruction("mov x3, #17");                                         // strlen("allow_self_signed")
    emitter.instruction("mov x4, sp");                                          // out_ptr_addr
    emitter.instruction("add x5, sp, #8");                                      // out_len_addr
    emitter.instruction("bl __rt_get_string_context_option");                   // x0 = 1 on hit
    emitter.instruction("cbnz x0, __rt_https_open_insecure_aarch64");           // present → relaxed (insecure) variant
    // ssl.verify_peer_name = "0" → relaxed peer verification
    emitter.instruction("str xzr, [sp, #0]");                                   // reset out ptr
    emitter.instruction("str xzr, [sp, #8]");                                   // reset out len
    abi::emit_symbol_address(emitter, "x0", "_ssl_key_str");
    emitter.instruction("mov x1, #3");                                          // strlen("ssl")
    abi::emit_symbol_address(emitter, "x2", "_ssl_verify_peer_name_key_str");
    emitter.instruction("mov x3, #16");                                         // strlen("verify_peer_name")
    emitter.instruction("mov x4, sp");                                          // out_ptr_addr
    emitter.instruction("add x5, sp, #8");                                      // out_len_addr
    emitter.instruction("bl __rt_get_string_context_option");                   // x0 = 1 on hit
    emitter.instruction("cbz x0, __rt_https_open_peer_name_aarch64");           // miss → check peer_name
    emitter.instruction("ldr x9, [sp, #0]");                                    // verify_peer_name string ptr
    emitter.instruction("ldrb w10, [x9]");                                      // first byte
    emitter.instruction("cmp w10, #48");                                        // '0'?
    emitter.instruction("b.eq __rt_https_open_insecure_aarch64");               // "0" → relaxed (insecure) variant
    // ssl.peer_name set → verify the certificate for that name (secure)
    emitter.label("__rt_https_open_peer_name_aarch64");
    emitter.instruction("str xzr, [sp, #0]");                                   // reset out ptr
    emitter.instruction("str xzr, [sp, #8]");                                   // reset out len
    abi::emit_symbol_address(emitter, "x0", "_ssl_key_str");
    emitter.instruction("mov x1, #3");                                          // strlen("ssl")
    abi::emit_symbol_address(emitter, "x2", "_ssl_peer_name_key_str");
    emitter.instruction("mov x3, #9");                                          // strlen("peer_name")
    emitter.instruction("mov x4, sp");                                          // out_ptr_addr
    emitter.instruction("add x5, sp, #8");                                      // out_len_addr
    emitter.instruction("bl __rt_get_string_context_option");                   // x0 = 1 on hit
    emitter.instruction("cbz x0, __rt_https_open_secure_aarch64");              // no peer_name → secure default
    emitter.instruction("ldr x3, [sp, #0]");                                    // peer_name ptr → connect arg 3
    emitter.instruction("ldr x4, [sp, #8]");                                    // peer_name len → connect arg 4
    abi::emit_symbol_address(emitter, "x9", "_elephc_tls_connect_peer_name_fn");
    emitter.instruction("ldr x9, [x9]");                                        // peer_name connect variant address
    emitter.instruction("b __rt_https_open_have_fn_aarch64");                   // continue at target label
    emitter.label("__rt_https_open_insecure_aarch64");
    abi::emit_symbol_address(emitter, "x9", "_elephc_tls_connect_insecure_fn");
    emitter.instruction("ldr x9, [x9]");                                        // insecure variant address
    emitter.instruction("mov x3, #0");                                          // no cafile/capath/peer_name path
    emitter.instruction("mov x4, #0");                                          // no cafile/capath/peer_name path
    emitter.instruction("b __rt_https_open_have_fn_aarch64");                   // continue at target label
    emitter.label("__rt_https_open_secure_aarch64");
    abi::emit_symbol_address(emitter, "x9", "_elephc_tls_connect_fn");
    emitter.instruction("ldr x9, [x9]");                                        // secure variant address
    emitter.instruction("mov x3, #0");                                          // no cafile/capath/peer_name path
    emitter.instruction("mov x4, #0");                                          // no cafile/capath/peer_name path
    emitter.label("__rt_https_open_have_fn_aarch64");
    emitter.instruction("add sp, sp, #16");                                     // discard the string-lookup out slots
    emitter.instruction("ldr x2, [sp], #16");                                   // restore port
    emitter.instruction("ldp x0, x1, [sp], #16");                               // restore host_ptr/host_len
    emitter.instruction("cbz x9, __rt_https_open_fail");                        // missing elephc-tls runtime means HTTPS open fails closed
    emitter.instruction("blr x9");                                              // open the TLS session, x0 = handle
    emitter.instruction("cmp x0, #0");                                          // did the TLS handshake fail?
    emitter.instruction("b.lt __rt_https_open_fail");                           // negative handle means TLS connect failed
    emitter.instruction("str x0, [sp, #0]");                                    // save the TLS session handle

    // -- elephc_tls_write(handle, req_ptr, req_len) --
    emitter.instruction("ldr x1, [sp, #8]");                                    // request pointer for the TLS write
    emitter.instruction("ldr x2, [sp, #16]");                                   // request length for the TLS write
    abi::emit_symbol_address(emitter, "x9", "_elephc_tls_write_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load the elephc_tls_write entry pointer
    emitter.instruction("blr x9");                                              // send the HTTP request through TLS

    // -- read the whole TLS-decrypted response into _https_resp_buf --
    emitter.instruction("str xzr, [sp, #24]");                                  // accumulated response length = 0
    emitter.label("__rt_https_open_read");
    emitter.instruction("ldr x0, [sp, #0]");                                    // TLS handle for the read
    abi::emit_symbol_address(emitter, "x1", "_https_resp_buf");
    emitter.instruction("ldr x9, [sp, #24]");                                   // response bytes already buffered
    emitter.instruction("add x1, x1, x9");                                      // read into the buffer past the buffered bytes
    emitter.instruction(&format!("mov x2, #{}", HTTPS_RESP_BUF_SIZE));          // response buffer capacity
    emitter.instruction("subs x2, x2, x9");                                     // remaining buffer capacity
    emitter.instruction("b.le __rt_https_open_read_done");                      // stop when the response buffer is full
    abi::emit_symbol_address(emitter, "x9", "_elephc_tls_read_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load the elephc_tls_read entry pointer
    emitter.instruction("blr x9");                                              // read more decrypted bytes from the TLS session
    emitter.instruction("cmp x0, #0");                                          // did the read hit EOF or fail?
    emitter.instruction("b.le __rt_https_open_read_done");                      // the server closed the TLS session
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the accumulated response length
    emitter.instruction("add x9, x9, x0");                                      // advance by the bytes just read
    emitter.instruction("str x9, [sp, #24]");                                   // store the updated response length
    emitter.instruction("b __rt_https_open_read");                              // continue reading the TLS response
    emitter.label("__rt_https_open_read_done");

    // -- elephc_tls_close(handle): send close_notify and drop the session --
    emitter.instruction("ldr x0, [sp, #0]");                                    // TLS handle for the close
    abi::emit_symbol_address(emitter, "x9", "_elephc_tls_close_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load the elephc_tls_close entry pointer
    emitter.instruction("blr x9");                                              // shut the TLS session down cleanly

    // -- scan for the CRLFCRLF that separates headers from the body --
    abi::emit_symbol_address(emitter, "x4", "_https_resp_buf");
    emitter.instruction("ldr x5, [sp, #24]");                                   // response length
    emitter.instruction("str xzr, [sp, #32]");                                  // body start = 0 when no separator is found
    emitter.instruction("mov x6, #0");                                          // response scan index
    emitter.label("__rt_https_open_scan");
    emitter.instruction("add x7, x6, #4");                                      // index just past a 4-byte separator
    emitter.instruction("cmp x7, x5");                                          // is there room for CRLFCRLF at this index?
    emitter.instruction("b.gt __rt_https_open_body");                           // no separator found: treat all bytes as body
    emitter.instruction("ldrb w8, [x4, x6]");                                   // separator byte 0
    emitter.instruction("cmp w8, #13");                                         // is it carriage return?
    emitter.instruction("b.ne __rt_https_open_scan_next");                      // not a separator start
    emitter.instruction("add x9, x6, #1");                                      // index of separator byte 1
    emitter.instruction("ldrb w8, [x4, x9]");                                   // separator byte 1
    emitter.instruction("cmp w8, #10");                                         // is it line feed?
    emitter.instruction("b.ne __rt_https_open_scan_next");                      // not the separator
    emitter.instruction("add x9, x6, #2");                                      // index of separator byte 2
    emitter.instruction("ldrb w8, [x4, x9]");                                   // separator byte 2
    emitter.instruction("cmp w8, #13");                                         // is it carriage return?
    emitter.instruction("b.ne __rt_https_open_scan_next");                      // not the separator
    emitter.instruction("add x9, x6, #3");                                      // index of separator byte 3
    emitter.instruction("ldrb w8, [x4, x9]");                                   // separator byte 3
    emitter.instruction("cmp w8, #10");                                         // is it line feed?
    emitter.instruction("b.ne __rt_https_open_scan_next");                      // not the separator
    emitter.instruction("add x6, x6, #4");                                      // the body begins just past CRLFCRLF
    emitter.instruction("str x6, [sp, #32]");                                   // save the body start offset
    emitter.instruction("b __rt_https_open_body");                              // headers are stripped
    emitter.label("__rt_https_open_scan_next");
    emitter.instruction("add x6, x6, #1");                                      // advance the scan index
    emitter.instruction("b __rt_https_open_scan");                              // keep scanning for the separator
    emitter.label("__rt_https_open_body");

    // -- back the body with an anonymous temp file --
    emitter.instruction("bl __rt_tmpfile");                                     // create an unlinked temp file, x0 = fd
    emitter.instruction("cmp x0, #0");                                          // did tmpfile fail?
    emitter.instruction("b.lt __rt_https_open_fail");                           // propagate the failure
    emitter.instruction("str x0, [sp, #40]");                                   // save the temp-file descriptor

    // -- write(temp, body, body length) --
    abi::emit_symbol_address(emitter, "x1", "_https_resp_buf");
    emitter.instruction("ldr x9, [sp, #32]");                                   // body start offset
    emitter.instruction("add x1, x1, x9");                                      // body pointer = buffer + body start
    emitter.instruction("ldr x10, [sp, #24]");                                  // response length
    emitter.instruction("sub x2, x10, x9");                                     // body length = response length - body start
    emitter.syscall(4);

    // -- lseek(temp, 0, SEEK_SET): rewind so the stream reads from the start --
    emitter.instruction("ldr x0, [sp, #40]");                                   // reload the temp-file descriptor
    emitter.instruction("mov x1, #0");                                          // offset = 0
    emitter.instruction("mov x2, #0");                                          // whence = SEEK_SET
    emitter.syscall(199);

    emitter.instruction("ldr x0, [sp, #40]");                                   // return the rewound body descriptor
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the https:// stream descriptor

    emitter.label("__rt_https_open_fail");
    emitter.instruction("mov x0, #-1");                                         // -1 signals a failed https:// open
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the failure result
}

/// Emits the Linux x86_64 stream runtime helper for https.
fn emit_https_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: https_open ---");
    emitter.label_global("__rt_https_open");

    // Frame (rbp-relative): [-8]=tls handle [-16]=host ptr [-24]=host len
    //                       [-32]=port [-40]=req ptr [-48]=req len
    //                       [-56]=response len [-64]=body start [-72]=temp fd.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 96");                                         // reserve the helper spill slots (96 = +16 for verify_peer)
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // save the host pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // save the host length
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // save the port number
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx");                       // save the request pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], r8");                        // save the request length

    // -- pick the connect fn from the ssl context: cafile, else verify_peer --
    //    The call always passes cafile args (rcx/r8); the secure/insecure
    //    variants ignore them, so one call site serves all three.
    emitter.instruction("mov QWORD PTR [rbp - 80], 0");                         // string-lookup out ptr default
    emitter.instruction("mov QWORD PTR [rbp - 88], 0");                         // string-lookup out len default
    emitter.instruction("lea rdi, [rip + _ssl_key_str]");                       // load runtime data address
    emitter.instruction("mov rsi, 3");                                          // strlen("ssl")
    emitter.instruction("lea rdx, [rip + _ssl_cafile_key_str]");                // load runtime data address
    emitter.instruction("mov rcx, 6");                                          // strlen("cafile")
    emitter.instruction("lea r8, [rbp - 80]");                                  // out_ptr_addr
    emitter.instruction("lea r9, [rbp - 88]");                                  // out_len_addr
    emitter.instruction("call __rt_get_string_context_option");                 // rax = 1 on hit
    emitter.instruction("test rax, rax");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_https_open_no_cafile_x");                      // no cafile → verify_peer dispatch
    emitter.instruction("mov r9, QWORD PTR [rip + _elephc_tls_connect_cafile_fn]"); // cafile connect variant
    emitter.instruction("mov rcx, QWORD PTR [rbp - 80]");                       // cafile path ptr → connect arg rcx
    emitter.instruction("mov r8, QWORD PTR [rbp - 88]");                        // cafile path len → connect arg r8
    emitter.instruction("jmp __rt_https_open_have_fn_x");                       // continue at target label
    emitter.label("__rt_https_open_no_cafile_x");
    // ssl.capath lookup — a directory of CA certificates (checked after cafile)
    emitter.instruction("mov QWORD PTR [rbp - 80], 0");                         // reset out ptr
    emitter.instruction("mov QWORD PTR [rbp - 88], 0");                         // reset out len
    emitter.instruction("lea rdi, [rip + _ssl_key_str]");                       // load runtime data address
    emitter.instruction("mov rsi, 3");                                          // strlen("ssl")
    emitter.instruction("lea rdx, [rip + _ssl_capath_key_str]");                // load runtime data address
    emitter.instruction("mov rcx, 6");                                          // strlen("capath")
    emitter.instruction("lea r8, [rbp - 80]");                                  // out_ptr_addr
    emitter.instruction("lea r9, [rbp - 88]");                                  // out_len_addr
    emitter.instruction("call __rt_get_string_context_option");                 // rax = 1 on hit
    emitter.instruction("test rax, rax");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_https_open_no_capath_x");                      // no capath → verify_peer dispatch
    emitter.instruction("mov r9, QWORD PTR [rip + _elephc_tls_connect_capath_fn]"); // capath connect variant
    emitter.instruction("mov rcx, QWORD PTR [rbp - 80]");                       // capath dir ptr → connect arg rcx
    emitter.instruction("mov r8, QWORD PTR [rbp - 88]");                        // capath dir len → connect arg r8
    emitter.instruction("jmp __rt_https_open_have_fn_x");                       // continue at target label
    emitter.label("__rt_https_open_no_capath_x");
    emitter.instruction("mov QWORD PTR [rbp - 80], 0");                         // reset out ptr
    emitter.instruction("mov QWORD PTR [rbp - 88], 0");                         // reset out len
    emitter.instruction("lea rdi, [rip + _ssl_key_str]");                       // load runtime data address
    emitter.instruction("mov rsi, 3");                                          // strlen("ssl")
    emitter.instruction("lea rdx, [rip + _ssl_verify_peer_key_str]");           // load runtime data address
    emitter.instruction("mov rcx, 11");                                         // strlen("verify_peer")
    emitter.instruction("lea r8, [rbp - 80]");                                  // out_ptr_addr
    emitter.instruction("lea r9, [rbp - 88]");                                  // out_len_addr
    emitter.instruction("call __rt_get_string_context_option");                 // rax = 1 on hit
    emitter.instruction("test rax, rax");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_https_open_self_signed_x");                    // verify_peer miss → check allow_self_signed
    emitter.instruction("mov rcx, QWORD PTR [rbp - 80]");                       // verify_peer ptr
    emitter.instruction("movzx eax, BYTE PTR [rcx]");                           // first byte of value
    emitter.instruction("cmp al, 48");                                          // '0'?
    emitter.instruction("je __rt_https_open_insecure_x");                       // "0" → relaxed (insecure) variant
    // ssl.allow_self_signed present (any value) → relaxed peer verification
    emitter.label("__rt_https_open_self_signed_x");
    emitter.instruction("mov QWORD PTR [rbp - 80], 0");                         // reset out ptr
    emitter.instruction("mov QWORD PTR [rbp - 88], 0");                         // reset out len
    emitter.instruction("lea rdi, [rip + _ssl_key_str]");                       // load runtime data address
    emitter.instruction("mov rsi, 3");                                          // strlen("ssl")
    emitter.instruction("lea rdx, [rip + _ssl_allow_self_signed_key_str]");     // load runtime data address
    emitter.instruction("mov rcx, 17");                                         // strlen("allow_self_signed")
    emitter.instruction("lea r8, [rbp - 80]");                                  // out_ptr_addr
    emitter.instruction("lea r9, [rbp - 88]");                                  // out_len_addr
    emitter.instruction("call __rt_get_string_context_option");                 // rax = 1 on hit
    emitter.instruction("test rax, rax");                                       // check whether the runtime value is zero
    emitter.instruction("jnz __rt_https_open_insecure_x");                      // present → relaxed (insecure) variant
    // ssl.verify_peer_name = "0" → relaxed peer verification
    emitter.instruction("mov QWORD PTR [rbp - 80], 0");                         // reset out ptr
    emitter.instruction("mov QWORD PTR [rbp - 88], 0");                         // reset out len
    emitter.instruction("lea rdi, [rip + _ssl_key_str]");                       // load runtime data address
    emitter.instruction("mov rsi, 3");                                          // strlen("ssl")
    emitter.instruction("lea rdx, [rip + _ssl_verify_peer_name_key_str]");      // load runtime data address
    emitter.instruction("mov rcx, 16");                                         // strlen("verify_peer_name")
    emitter.instruction("lea r8, [rbp - 80]");                                  // out_ptr_addr
    emitter.instruction("lea r9, [rbp - 88]");                                  // out_len_addr
    emitter.instruction("call __rt_get_string_context_option");                 // rax = 1 on hit
    emitter.instruction("test rax, rax");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_https_open_peer_name_x");                      // miss → check peer_name
    emitter.instruction("mov rcx, QWORD PTR [rbp - 80]");                       // verify_peer_name ptr
    emitter.instruction("movzx eax, BYTE PTR [rcx]");                           // first byte of value
    emitter.instruction("cmp al, 48");                                          // '0'?
    emitter.instruction("je __rt_https_open_insecure_x");                       // "0" → relaxed (insecure) variant
    // ssl.peer_name set → verify the certificate for that name (secure)
    emitter.label("__rt_https_open_peer_name_x");
    emitter.instruction("mov QWORD PTR [rbp - 80], 0");                         // reset out ptr
    emitter.instruction("mov QWORD PTR [rbp - 88], 0");                         // reset out len
    emitter.instruction("lea rdi, [rip + _ssl_key_str]");                       // load runtime data address
    emitter.instruction("mov rsi, 3");                                          // strlen("ssl")
    emitter.instruction("lea rdx, [rip + _ssl_peer_name_key_str]");             // load runtime data address
    emitter.instruction("mov rcx, 9");                                          // strlen("peer_name")
    emitter.instruction("lea r8, [rbp - 80]");                                  // out_ptr_addr
    emitter.instruction("lea r9, [rbp - 88]");                                  // out_len_addr
    emitter.instruction("call __rt_get_string_context_option");                 // rax = 1 on hit
    emitter.instruction("test rax, rax");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_https_open_secure_x");                         // no peer_name → secure default
    emitter.instruction("mov r9, QWORD PTR [rip + _elephc_tls_connect_peer_name_fn]"); // peer_name connect variant
    emitter.instruction("mov rcx, QWORD PTR [rbp - 80]");                       // peer_name ptr → connect arg rcx
    emitter.instruction("mov r8, QWORD PTR [rbp - 88]");                        // peer_name len → connect arg r8
    emitter.instruction("jmp __rt_https_open_have_fn_x");                       // rcx/r8 already hold the peer_name path
    emitter.label("__rt_https_open_insecure_x");
    emitter.instruction("mov r9, QWORD PTR [rip + _elephc_tls_connect_insecure_fn]"); // relaxed (insecure) variant
    emitter.instruction("xor ecx, ecx");                                        // no cafile/capath/peer_name path
    emitter.instruction("xor r8d, r8d");                                        // no cafile/capath/peer_name path
    emitter.instruction("jmp __rt_https_open_have_fn_x");                       // continue at target label
    emitter.label("__rt_https_open_secure_x");
    emitter.instruction("mov r9, QWORD PTR [rip + _elephc_tls_connect_fn]");    // secure default variant
    emitter.instruction("xor ecx, ecx");                                        // no cafile/capath/peer_name path
    emitter.instruction("xor r8d, r8d");                                        // no cafile/capath/peer_name path
    emitter.label("__rt_https_open_have_fn_x");

    // -- connect(host, host_len, port, cafile_ptr, cafile_len) — through r9 --
    emitter.instruction("test r9, r9");                                         // missing elephc-tls runtime means HTTPS open fails closed
    emitter.instruction("jz __rt_https_open_fail_x86");                         // return a failed HTTPS stream when no TLS entry is available
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // arg 0 = host pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // arg 1 = host length
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // arg 2 = port (rcx/r8 hold cafile args)
    emitter.instruction("call r9");                                             // open the TLS session, rax = handle
    emitter.instruction("cmp rax, 0");                                          // did the TLS handshake fail?
    emitter.instruction("jl __rt_https_open_fail_x86");                         // negative handle means TLS connect failed
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the TLS session handle

    // -- elephc_tls_write(handle, req_ptr, req_len) --
    emitter.instruction("mov rdi, rax");                                        // arg 0 = TLS handle for the write
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // arg 1 = request pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // arg 2 = request length
    emitter.instruction("mov r9, QWORD PTR [rip + _elephc_tls_write_fn]");      // load the elephc_tls_write entry pointer
    emitter.instruction("call r9");                                             // send the HTTP request through TLS

    // -- read the whole TLS-decrypted response into _https_resp_buf --
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // accumulated response length = 0
    emitter.label("__rt_https_open_read_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // TLS handle for the read
    emitter.instruction("lea rsi, [rip + _https_resp_buf]");                    // response buffer base
    emitter.instruction("add rsi, QWORD PTR [rbp - 56]");                       // read past the bytes already buffered
    emitter.instruction(&format!("mov rdx, {}", HTTPS_RESP_BUF_SIZE));          // response buffer capacity
    emitter.instruction("sub rdx, QWORD PTR [rbp - 56]");                       // remaining buffer capacity
    emitter.instruction("cmp rdx, 0");                                          // is the response buffer full?
    emitter.instruction("jle __rt_https_open_read_done_x86");                   // stop when no capacity remains
    emitter.instruction("mov r9, QWORD PTR [rip + _elephc_tls_read_fn]");       // load the elephc_tls_read entry pointer
    emitter.instruction("call r9");                                             // read more decrypted bytes from the TLS session
    emitter.instruction("cmp rax, 0");                                          // did the read hit EOF or fail?
    emitter.instruction("jle __rt_https_open_read_done_x86");                   // the server closed the TLS session
    emitter.instruction("mov r10, QWORD PTR [rbp - 56]");                       // reload the accumulated response length
    emitter.instruction("add r10, rax");                                        // advance by the bytes just read
    emitter.instruction("mov QWORD PTR [rbp - 56], r10");                       // store the updated response length
    emitter.instruction("jmp __rt_https_open_read_x86");                        // continue reading the TLS response
    emitter.label("__rt_https_open_read_done_x86");

    // -- elephc_tls_close(handle): send close_notify and drop the session --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // TLS handle for the close
    emitter.instruction("mov r9, QWORD PTR [rip + _elephc_tls_close_fn]");      // load the elephc_tls_close entry pointer
    emitter.instruction("call r9");                                             // shut the TLS session down cleanly

    // -- scan for the CRLFCRLF that separates headers from the body --
    emitter.instruction("lea r8, [rip + _https_resp_buf]");                     // response buffer base
    emitter.instruction("mov r10, QWORD PTR [rbp - 56]");                       // response length
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // body start = 0 when no separator is found
    emitter.instruction("xor rcx, rcx");                                        // response scan index
    emitter.label("__rt_https_open_scan_x86");
    emitter.instruction("lea rax, [rcx + 4]");                                  // index just past a 4-byte separator
    emitter.instruction("cmp rax, r10");                                        // is there room for CRLFCRLF at this index?
    emitter.instruction("jg __rt_https_open_body_x86");                         // no separator found: treat all bytes as body
    emitter.instruction("movzx eax, BYTE PTR [r8 + rcx]");                      // separator byte 0
    emitter.instruction("cmp al, 13");                                          // is it carriage return?
    emitter.instruction("jne __rt_https_open_scan_next_x86");                   // not a separator start
    emitter.instruction("lea rax, [rcx + 1]");                                  // index of separator byte 1
    emitter.instruction("movzx eax, BYTE PTR [r8 + rax]");                      // separator byte 1
    emitter.instruction("cmp al, 10");                                          // is it line feed?
    emitter.instruction("jne __rt_https_open_scan_next_x86");                   // not the separator
    emitter.instruction("lea rax, [rcx + 2]");                                  // index of separator byte 2
    emitter.instruction("movzx eax, BYTE PTR [r8 + rax]");                      // separator byte 2
    emitter.instruction("cmp al, 13");                                          // is it carriage return?
    emitter.instruction("jne __rt_https_open_scan_next_x86");                   // not the separator
    emitter.instruction("lea rax, [rcx + 3]");                                  // index of separator byte 3
    emitter.instruction("movzx eax, BYTE PTR [r8 + rax]");                      // separator byte 3
    emitter.instruction("cmp al, 10");                                          // is it line feed?
    emitter.instruction("jne __rt_https_open_scan_next_x86");                   // not the separator
    emitter.instruction("lea rax, [rcx + 4]");                                  // the body begins just past CRLFCRLF
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // save the body start offset
    emitter.instruction("jmp __rt_https_open_body_x86");                        // headers are stripped
    emitter.label("__rt_https_open_scan_next_x86");
    emitter.instruction("inc rcx");                                             // advance the scan index
    emitter.instruction("jmp __rt_https_open_scan_x86");                        // keep scanning for the separator
    emitter.label("__rt_https_open_body_x86");

    // -- back the body with an anonymous temp file --
    emitter.instruction("call __rt_tmpfile");                                   // create an unlinked temp file, rax = fd
    emitter.instruction("cmp rax, 0");                                          // did tmpfile fail?
    emitter.instruction("jl __rt_https_open_fail_x86");                         // propagate the failure
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // save the temp-file descriptor

    // -- write(temp, body, body length) --
    emitter.instruction("mov rdi, rax");                                        // temp-file descriptor for the write
    emitter.instruction("lea rsi, [rip + _https_resp_buf]");                    // response buffer base
    emitter.instruction("add rsi, QWORD PTR [rbp - 64]");                       // body pointer = buffer + body start
    emitter.instruction("mov rdx, QWORD PTR [rbp - 56]");                       // response length
    emitter.instruction("sub rdx, QWORD PTR [rbp - 64]");                       // body length = response length - body start
    emitter.instruction("call write");                                          // copy the body into the temp file

    // -- lseek(temp, 0, SEEK_SET): rewind so the stream reads from the start --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 72]");                       // reload the temp-file descriptor
    emitter.instruction("xor esi, esi");                                        // offset = 0
    emitter.instruction("xor edx, edx");                                        // whence = SEEK_SET
    emitter.instruction("call lseek");                                          // rewind the temp file

    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // return the rewound body descriptor
    emitter.instruction("add rsp, 96");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the https:// stream descriptor

    emitter.label("__rt_https_open_fail_x86");
    emitter.instruction("mov rax, -1");                                         // -1 signals a failed https:// open
    emitter.instruction("add rsp, 96");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the failure result
}
