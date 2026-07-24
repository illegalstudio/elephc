//! Purpose:
//! Emits the `https://` wrapper runtime helper `__rt_https_open`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - `__rt_https_open` is the entry point invoked by the `https://` `fopen` lowering.
//!
//! Key details:
//! - v1 issues a single HTTP/1.0 `GET` over a TLS-secured TCP connection: it
//!   establishes the TLS session through elephc-tls, sends the compile-time-built
//!   request, reads the whole response until the server closes, locates the
//!   `CRLFCRLF` header/body separator, and copies the body into an anonymous
//!   temp file whose descriptor is the readable stream.
//! - The response is buffered in the 1 MiB `_https_resp_buf`; a larger response
//!   fails rather than being returned as a silently truncated stream. HTTP/1.0 +
//!   `Connection: close` keeps the body close-framed.
//! - All elephc-tls C entry points are invoked through the runtime function
//!   pointers (`_elephc_tls_*_fn`) so the shared runtime carries no direct
//!   elephc-tls symbol reference — only programs that actually open https URLs
//!   pull in `-lelephc_tls` at link time.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

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

    // Frame (192 bytes): response bookkeeping at 0..40, the stable 88-byte
    // ElephcTlsClientOptions at 48..135, bool scratch at 136, saved connection
    // inputs at 144..160, I/O progress/failure state at 168, and the saved
    // frame pair at 176..184.
    emitter.instruction("sub sp, sp, #192");                                    // allocate response, TLS options, and connection spill storage
    emitter.instruction("stp x29, x30, [sp, #176]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #176");                                   // establish the helper frame pointer
    emitter.instruction("str x3, [sp, #8]");                                    // save the HTTP request pointer
    emitter.instruction("str x4, [sp, #16]");                                   // save the HTTP request length
    emitter.instruction("str x0, [sp, #144]");                                  // save the connection host pointer
    emitter.instruction("str x1, [sp, #152]");                                  // save the connection host length
    emitter.instruction("str x2, [sp, #160]");                                  // save the connection port

    // -- initialize ElephcTlsClientOptions v1 (88 bytes at sp + 48) --
    for offset in (48..=128).step_by(8) {
        emitter.instruction(&format!("str xzr, [sp, #{offset}]"));              // zero one TLS options word
    }
    emitter.instruction("mov w9, #1");                                          // select ElephcTlsClientOptions ABI v1
    emitter.instruction("str w9, [sp, #48]");                                   // store options.abi_version at byte offset 0
    emitter.instruction("mov w9, #3");                                          // default to verify_peer plus verify_peer_name
    emitter.instruction("str w9, [sp, #52]");                                   // store options.policy at byte offset 4

    // -- collect all combinable string options directly into the ABI struct --
    for (key, key_len, ptr_offset, len_offset) in [
        ("_ssl_peer_name_key_str", 9, 56, 64),
        ("_ssl_cafile_key_str", 6, 72, 80),
        ("_ssl_capath_key_str", 6, 88, 96),
        ("_ssl_local_cert_key_str", 10, 104, 112),
        ("_ssl_local_pk_key_str", 8, 120, 128),
    ] {
        abi::emit_symbol_address(emitter, "x0", "_ssl_key_str");
        emitter.instruction("mov x1, #3");                                      // pass strlen("ssl")
        abi::emit_symbol_address(emitter, "x2", key);
        emitter.instruction(&format!("mov x3, #{key_len}"));                    // pass the TLS option-key length
        emitter.instruction(&format!("add x4, sp, #{ptr_offset}"));             // pass the destination pointer-field address
        emitter.instruction(&format!("add x5, sp, #{len_offset}"));             // pass the destination length-field address
        emitter.instruction("bl __rt_get_string_context_option");               // populate this optional string field when present
    }

    // -- resolve verify_peer with PHP scalar truthiness (default true) --
    emitter.instruction("mov x9, #1");                                          // default verify_peer to true for TLS clients
    emitter.instruction("str x9, [sp, #136]");                                  // seed the scalar lookup output
    abi::emit_symbol_address(emitter, "x0", "_ssl_key_str");
    emitter.instruction("mov x1, #3");                                          // pass strlen("ssl")
    abi::emit_symbol_address(emitter, "x2", "_ssl_verify_peer_key_str");
    emitter.instruction("mov x3, #11");                                         // pass strlen("verify_peer")
    emitter.instruction("add x4, sp, #136");                                    // pass the verify_peer output address
    emitter.instruction("bl __rt_get_bool_context_option");                     // accept bool/int/string PHP truthiness
    emitter.instruction("ldr w9, [sp, #136]");                                  // load the normalized verify_peer value
    emitter.instruction("and w9, w9, #1");                                      // constrain the policy contribution to one bit
    emitter.instruction("ldr w10, [sp, #52]");                                  // load the current TLS policy bitmask
    emitter.instruction("bic w10, w10, #1");                                    // clear the previous verify_peer bit
    emitter.instruction("orr w10, w10, w9");                                    // merge the resolved verify_peer bit
    emitter.instruction("str w10, [sp, #52]");                                  // persist the updated TLS policy

    // -- resolve verify_peer_name independently (default true) --
    emitter.instruction("mov x9, #1");                                          // default verify_peer_name to true for TLS clients
    emitter.instruction("str x9, [sp, #136]");                                  // seed the scalar lookup output
    abi::emit_symbol_address(emitter, "x0", "_ssl_key_str");
    emitter.instruction("mov x1, #3");                                          // pass strlen("ssl")
    abi::emit_symbol_address(emitter, "x2", "_ssl_verify_peer_name_key_str");
    emitter.instruction("mov x3, #16");                                         // pass strlen("verify_peer_name")
    emitter.instruction("add x4, sp, #136");                                    // pass the verify_peer_name output address
    emitter.instruction("bl __rt_get_bool_context_option");                     // accept bool/int/string PHP truthiness
    emitter.instruction("ldr w9, [sp, #136]");                                  // load the normalized verify_peer_name value
    emitter.instruction("and w9, w9, #1");                                      // constrain the policy contribution to one bit
    emitter.instruction("lsl w9, w9, #1");                                      // move the value into policy bit one
    emitter.instruction("ldr w10, [sp, #52]");                                  // load the current TLS policy bitmask
    emitter.instruction("bic w10, w10, #2");                                    // clear the previous verify_peer_name bit
    emitter.instruction("orr w10, w10, w9");                                    // merge the resolved verify_peer_name bit
    emitter.instruction("str w10, [sp, #52]");                                  // persist the updated TLS policy

    // -- resolve allow_self_signed independently (default false) --
    emitter.instruction("str xzr, [sp, #136]");                                 // default allow_self_signed to false
    abi::emit_symbol_address(emitter, "x0", "_ssl_key_str");
    emitter.instruction("mov x1, #3");                                          // pass strlen("ssl")
    abi::emit_symbol_address(emitter, "x2", "_ssl_allow_self_signed_key_str");
    emitter.instruction("mov x3, #17");                                         // pass strlen("allow_self_signed")
    emitter.instruction("add x4, sp, #136");                                    // pass the allow_self_signed output address
    emitter.instruction("bl __rt_get_bool_context_option");                     // accept bool/int/string PHP truthiness
    emitter.instruction("ldr w9, [sp, #136]");                                  // load the normalized allow_self_signed value
    emitter.instruction("and w9, w9, #1");                                      // constrain the policy contribution to one bit
    emitter.instruction("lsl w9, w9, #2");                                      // move the value into policy bit two
    emitter.instruction("ldr w10, [sp, #52]");                                  // load the current TLS policy bitmask
    emitter.instruction("bic w10, w10, #4");                                    // clear the previous allow_self_signed bit
    emitter.instruction("orr w10, w10, w9");                                    // merge the resolved allow_self_signed bit
    emitter.instruction("str w10, [sp, #52]");                                  // persist the updated TLS policy

    // -- connect_with_options(host, host_len, port, &options) --
    abi::emit_symbol_address(emitter, "x9", "_elephc_tls_connect_with_options_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load the policy-aware TLS connect entry
    emitter.instruction("ldr x0, [sp, #144]");                                  // restore the connection host pointer
    emitter.instruction("ldr x1, [sp, #152]");                                  // restore the connection host length
    emitter.instruction("ldr x2, [sp, #160]");                                  // restore the connection port
    emitter.instruction("add x3, sp, #48");                                     // pass the stable TLS options struct address
    emitter.instruction("cbz x9, __rt_https_open_fail");                        // missing elephc-tls runtime means HTTPS open fails closed
    emitter.emit_published_bridge_call("x9");                                  // open the TLS session through the published ABI entry, x0 = handle
    emitter.instruction("cmp x0, #0");                                          // did the TLS handshake fail?
    emitter.instruction("b.lt __rt_https_open_fail");                           // negative handle means TLS connect failed
    emitter.instruction("str x0, [sp, #0]");                                    // save the TLS session handle

    // -- write the complete HTTP request, preserving partial TLS writes --
    emitter.instruction("str xzr, [sp, #168]");                                 // request write offset starts at zero
    emitter.label("__rt_https_open_write");
    emitter.instruction("ldr x9, [sp, #168]");                                  // reload bytes already sent through TLS
    emitter.instruction("ldr x10, [sp, #16]");                                  // reload the full request length
    emitter.instruction("cmp x9, x10");                                         // has every request byte been written?
    emitter.instruction("b.ge __rt_https_open_read_start");                     // begin reading only after a complete request write
    emitter.instruction("ldr x0, [sp, #0]");                                    // pass the TLS session handle
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the HTTP request pointer
    emitter.instruction("add x1, x1, x9");                                      // advance past request bytes already sent
    emitter.instruction("sub x2, x10, x9");                                     // pass only the unwritten request suffix
    abi::emit_symbol_address(emitter, "x9", "_elephc_tls_write_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load the elephc_tls_write entry pointer
    emitter.emit_published_bridge_call("x9");                                  // send the HTTP request through the published TLS entry
    emitter.instruction("cmp x0, #0");                                          // TLS write must make forward progress
    emitter.instruction("b.le __rt_https_open_tls_failed");                     // errors, WouldBlock, TimedOut, and zero all fail this blocking wrapper
    emitter.instruction("ldr x9, [sp, #168]");                                  // reload the completed write offset
    emitter.instruction("add x9, x9, x0");                                      // account for this partial TLS write
    emitter.instruction("str x9, [sp, #168]");                                  // retain progress for the next write iteration
    emitter.instruction("b __rt_https_open_write");                             // continue until the complete request is sent

    // -- read the whole TLS-decrypted response into _https_resp_buf --
    emitter.label("__rt_https_open_read_start");
    emitter.instruction("str xzr, [sp, #24]");                                  // accumulated response length = 0
    emitter.label("__rt_https_open_read");
    emitter.instruction("ldr x0, [sp, #0]");                                    // TLS handle for the read
    abi::emit_symbol_address(emitter, "x1", "_https_resp_buf");
    emitter.instruction("ldr x9, [sp, #24]");                                   // response bytes already buffered
    emitter.instruction("add x1, x1, x9");                                      // read into the buffer past the buffered bytes
    emitter.instruction(&format!("mov x2, #{}", HTTPS_RESP_BUF_SIZE));          // response buffer capacity
    emitter.instruction("subs x2, x2, x9");                                     // remaining buffer capacity
    emitter.instruction("b.le __rt_https_open_tls_failed");                     // fail instead of returning a truncated response body
    abi::emit_symbol_address(emitter, "x9", "_elephc_tls_read_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load the elephc_tls_read entry pointer
    emitter.emit_published_bridge_call("x9");                                  // read decrypted bytes through the published TLS entry
    emitter.instruction("cmp x0, #0");                                          // distinguish a clean EOF from every TLS error sentinel
    emitter.instruction("b.lt __rt_https_open_tls_failed");                     // terminal, WouldBlock, and TimedOut reads must not become EOF
    emitter.instruction("b.eq __rt_https_open_read_done");                      // a zero-byte TLS read is the sole successful EOF
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the accumulated response length
    emitter.instruction("add x9, x9, x0");                                      // advance by the bytes just read
    emitter.instruction("str x9, [sp, #24]");                                   // store the updated response length
    emitter.instruction("b __rt_https_open_read");                              // continue reading the TLS response
    emitter.label("__rt_https_open_read_done");
    emitter.instruction("str xzr, [sp, #168]");                                 // mark clean TLS completion before closing the session
    emitter.instruction("b __rt_https_open_tls_close");                         // release the TLS handle before materializing the stream
    emitter.label("__rt_https_open_tls_failed");
    emitter.instruction("mov x9, #1");                                          // remember that I/O failed while the handle still needs cleanup
    emitter.instruction("str x9, [sp, #168]");                                  // preserve failure state across the TLS close call

    // -- elephc_tls_close(handle): send close_notify and drop the session --
    emitter.label("__rt_https_open_tls_close");
    emitter.instruction("ldr x0, [sp, #0]");                                    // TLS handle for the close
    abi::emit_symbol_address(emitter, "x9", "_elephc_tls_close_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load the elephc_tls_close entry pointer
    emitter.emit_published_bridge_call("x9");                                  // shut the TLS session down through the published TLS entry
    emitter.instruction("ldr x9, [sp, #168]");                                  // recover the pre-close I/O status
    emitter.instruction("cbnz x9, __rt_https_open_fail");                       // never materialize a partial response after TLS I/O failed

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

    // -- write the complete body into the temporary stream --
    emitter.instruction("str xzr, [sp, #168]");                                 // temporary-file write offset starts at zero
    emitter.label("__rt_https_open_body_write");
    emitter.instruction("ldr x9, [sp, #168]");                                  // reload bytes already copied to the temporary file
    emitter.instruction("ldr x10, [sp, #24]");                                  // reload the full response length
    emitter.instruction("ldr x11, [sp, #32]");                                  // reload the response-body start offset
    emitter.instruction("sub x10, x10, x11");                                   // derive the complete body length
    emitter.instruction("cmp x9, x10");                                         // has the full response body reached the tempfile?
    emitter.instruction("b.ge __rt_https_open_body_written");                   // continue only after every body byte was persisted
    emitter.instruction("ldr x0, [sp, #40]");                                   // pass the anonymous temporary-file descriptor
    abi::emit_symbol_address(emitter, "x1", "_https_resp_buf");
    emitter.instruction("add x1, x1, x11");                                     // point at the beginning of the response body
    emitter.instruction("add x1, x1, x9");                                      // advance past body bytes already copied
    emitter.instruction("sub x2, x10, x9");                                     // pass the remaining body suffix
    emitter.syscall(4);
    if emitter.platform.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: a negative write result means tempfile output failed
    }
    emitter.instruction(&emitter.platform.branch_on_syscall_success("__rt_https_open_body_write_result")); // continue only after a successful tempfile write
    emitter.instruction("b __rt_https_open_temp_fail");                         // discard the tempfile after a write failure
    emitter.label("__rt_https_open_body_write_result");
    emitter.instruction("cbz x0, __rt_https_open_temp_fail");                   // a zero-byte write cannot complete the remaining body
    emitter.instruction("ldr x9, [sp, #168]");                                  // reload the copied-byte count
    emitter.instruction("add x9, x9, x0");                                      // account for this partial tempfile write
    emitter.instruction("str x9, [sp, #168]");                                  // retain progress for the next copy iteration
    emitter.instruction("b __rt_https_open_body_write");                        // continue until the entire body is copied
    emitter.label("__rt_https_open_body_written");

    // -- lseek(temp, 0, SEEK_SET): rewind so the stream reads from the start --
    emitter.instruction("ldr x0, [sp, #40]");                                   // reload the temp-file descriptor
    emitter.instruction("mov x1, #0");                                          // offset = 0
    emitter.instruction("mov x2, #0");                                          // whence = SEEK_SET
    emitter.syscall(199);
    if emitter.platform.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: a negative seek result means the tempfile is unusable
    }
    emitter.instruction(&emitter.platform.branch_on_syscall_success("__rt_https_open_rewound")); // return the tempfile only after a successful rewind
    emitter.instruction("b __rt_https_open_temp_fail");                         // close an unreadable temporary stream before failing
    emitter.label("__rt_https_open_rewound");

    emitter.instruction("ldr x0, [sp, #40]");                                   // return the rewound body descriptor
    emitter.instruction("ldp x29, x30, [sp, #176]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #192");                                    // release the helper frame
    emitter.instruction("ret");                                                 // return the https:// stream descriptor

    emitter.label("__rt_https_open_temp_fail");
    emitter.instruction("ldr x0, [sp, #40]");                                   // close the temporary descriptor before reporting failure
    emitter.syscall(6);
    emitter.instruction("b __rt_https_open_fail");                              // reuse the common failure epilogue after cleanup

    emitter.label("__rt_https_open_fail");
    emitter.instruction("mov x0, #-1");                                         // -1 signals a failed https:// open
    emitter.instruction("ldp x29, x30, [sp, #176]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #192");                                    // release the helper frame
    emitter.instruction("ret");                                                 // return the failure result
}

/// Emits the Linux x86_64 stream runtime helper for https.
fn emit_https_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: https_open ---");
    emitter.label_global("__rt_https_open");

    // Frame (rbp-relative): response bookkeeping at -8..-80, the stable
    // 88-byte ElephcTlsClientOptions at -176..-89, and bool scratch at -184.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 192");                                        // reserve aligned response, TLS options, and scalar scratch storage
    emitter.instruction("mov QWORD PTR [rbp - 16], rdi");                       // save the host pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // save the host length
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // save the port number
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx");                       // save the request pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], r8");                        // save the request length

    // -- initialize ElephcTlsClientOptions v1 (88 bytes at rbp - 176) --
    for offset in (96..=176).step_by(8) {
        emitter.instruction(&format!("mov QWORD PTR [rbp - {offset}], 0"));     // zero one TLS options word
    }
    emitter.instruction("mov DWORD PTR [rbp - 176], 1");                        // store options.abi_version at byte offset 0
    emitter.instruction("mov DWORD PTR [rbp - 172], 3");                        // default policy verifies the peer and its name

    // -- collect all combinable string options directly into the ABI struct --
    for (key, key_len, ptr_offset, len_offset) in [
        ("_ssl_peer_name_key_str", 9, 168, 160),
        ("_ssl_cafile_key_str", 6, 152, 144),
        ("_ssl_capath_key_str", 6, 136, 128),
        ("_ssl_local_cert_key_str", 10, 120, 112),
        ("_ssl_local_pk_key_str", 8, 104, 96),
    ] {
        abi::emit_symbol_address(emitter, "rdi", "_ssl_key_str");               // pass the TLS context wrapper name
        emitter.instruction("mov rsi, 3");                                      // pass strlen("ssl")
        abi::emit_symbol_address(emitter, "rdx", key);
        emitter.instruction(&format!("mov rcx, {key_len}"));                    // pass the TLS option-key length
        emitter.instruction(&format!("lea r8, [rbp - {ptr_offset}]"));          // pass the destination pointer-field address
        emitter.instruction(&format!("lea r9, [rbp - {len_offset}]"));          // pass the destination length-field address
        emitter.instruction("call __rt_get_string_context_option");             // populate this optional string field when present
    }

    // -- resolve verify_peer with PHP scalar truthiness (default true) --
    emitter.instruction("mov QWORD PTR [rbp - 184], 1");                        // default verify_peer to true for TLS clients
    abi::emit_symbol_address(emitter, "rdi", "_ssl_key_str");                   // pass the TLS context wrapper name
    emitter.instruction("mov rsi, 3");                                          // pass strlen("ssl")
    abi::emit_symbol_address(emitter, "rdx", "_ssl_verify_peer_key_str");       // pass the verify_peer option name
    emitter.instruction("mov rcx, 11");                                         // pass strlen("verify_peer")
    emitter.instruction("lea r8, [rbp - 184]");                                 // pass the verify_peer output address
    emitter.instruction("call __rt_get_bool_context_option");                   // accept bool/int/string PHP truthiness
    emitter.instruction("mov r10d, DWORD PTR [rbp - 184]");                     // load the normalized verify_peer value
    emitter.instruction("and r10d, 1");                                         // constrain the policy contribution to one bit
    emitter.instruction("mov eax, DWORD PTR [rbp - 172]");                      // load the current TLS policy bitmask
    emitter.instruction("and eax, -2");                                         // clear the previous verify_peer bit
    emitter.instruction("or eax, r10d");                                        // merge the resolved verify_peer bit
    emitter.instruction("mov DWORD PTR [rbp - 172], eax");                      // persist the updated TLS policy

    // -- resolve verify_peer_name independently (default true) --
    emitter.instruction("mov QWORD PTR [rbp - 184], 1");                        // default verify_peer_name to true for TLS clients
    abi::emit_symbol_address(emitter, "rdi", "_ssl_key_str");                   // pass the TLS context wrapper name
    emitter.instruction("mov rsi, 3");                                          // pass strlen("ssl")
    abi::emit_symbol_address(emitter, "rdx", "_ssl_verify_peer_name_key_str");  // pass the verify_peer_name option name
    emitter.instruction("mov rcx, 16");                                         // pass strlen("verify_peer_name")
    emitter.instruction("lea r8, [rbp - 184]");                                 // pass the verify_peer_name output address
    emitter.instruction("call __rt_get_bool_context_option");                   // accept bool/int/string PHP truthiness
    emitter.instruction("mov r10d, DWORD PTR [rbp - 184]");                     // load the normalized verify_peer_name value
    emitter.instruction("and r10d, 1");                                         // constrain the policy contribution to one bit
    emitter.instruction("shl r10d, 1");                                         // move the value into policy bit one
    emitter.instruction("mov eax, DWORD PTR [rbp - 172]");                      // load the current TLS policy bitmask
    emitter.instruction("and eax, -3");                                         // clear the previous verify_peer_name bit
    emitter.instruction("or eax, r10d");                                        // merge the resolved verify_peer_name bit
    emitter.instruction("mov DWORD PTR [rbp - 172], eax");                      // persist the updated TLS policy

    // -- resolve allow_self_signed independently (default false) --
    emitter.instruction("mov QWORD PTR [rbp - 184], 0");                        // default allow_self_signed to false
    abi::emit_symbol_address(emitter, "rdi", "_ssl_key_str");                   // pass the TLS context wrapper name
    emitter.instruction("mov rsi, 3");                                          // pass strlen("ssl")
    abi::emit_symbol_address(emitter, "rdx", "_ssl_allow_self_signed_key_str"); // pass the allow_self_signed option name
    emitter.instruction("mov rcx, 17");                                         // pass strlen("allow_self_signed")
    emitter.instruction("lea r8, [rbp - 184]");                                 // pass the allow_self_signed output address
    emitter.instruction("call __rt_get_bool_context_option");                   // accept bool/int/string PHP truthiness
    emitter.instruction("mov r10d, DWORD PTR [rbp - 184]");                     // load the normalized allow_self_signed value
    emitter.instruction("and r10d, 1");                                         // constrain the policy contribution to one bit
    emitter.instruction("shl r10d, 2");                                         // move the value into policy bit two
    emitter.instruction("mov eax, DWORD PTR [rbp - 172]");                      // load the current TLS policy bitmask
    emitter.instruction("and eax, -5");                                         // clear the previous allow_self_signed bit
    emitter.instruction("or eax, r10d");                                        // merge the resolved allow_self_signed bit
    emitter.instruction("mov DWORD PTR [rbp - 172], eax");                      // persist the updated TLS policy

    // -- connect_with_options(host, host_len, port, &options) --
    abi::emit_load_symbol_to_reg(emitter, "r9", "_elephc_tls_connect_with_options_fn", 0); // load the policy-aware TLS connect entry
    emitter.instruction("test r9, r9");                                         // missing elephc-tls runtime means HTTPS open fails closed
    emitter.instruction("jz __rt_https_open_fail_x86");                         // return a failed HTTPS stream when no TLS entry is available
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // arg 0 = host pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // arg 1 = host length
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // arg 2 = connection port
    emitter.instruction("lea rcx, [rbp - 176]");                                // arg 3 = stable TLS options struct address
    emitter.emit_published_bridge_call("r9");                                  // call the published TLS adapter, rax = handle
    emitter.instruction("cmp rax, 0");                                          // did the TLS handshake fail?
    emitter.instruction("jl __rt_https_open_fail_x86");                         // negative handle means TLS connect failed
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the TLS session handle

    // -- write the complete HTTP request, preserving partial TLS writes --
    emitter.instruction("mov QWORD PTR [rbp - 80], 0");                         // request write offset starts at zero
    emitter.label("__rt_https_open_write_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 80]");                       // reload bytes already sent through TLS
    emitter.instruction("cmp r10, QWORD PTR [rbp - 48]");                       // has every request byte been written?
    emitter.instruction("jge __rt_https_open_read_start_x86");                  // begin reading only after a complete request write
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // arg 0 = TLS handle for the write
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // arg 1 = request pointer
    emitter.instruction("add rsi, r10");                                        // advance past request bytes already sent
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // reload the full request length
    emitter.instruction("sub rdx, r10");                                        // pass only the unwritten request suffix
    abi::emit_load_symbol_to_reg(emitter, "r9", "_elephc_tls_write_fn", 0);     // load the elephc_tls_write entry pointer
    emitter.emit_published_bridge_call("r9");                                  // send the request through the published TLS adapter
    emitter.instruction("test rax, rax");                                       // TLS write must make forward progress
    emitter.instruction("jle __rt_https_open_tls_failed_x86");                  // errors, WouldBlock, TimedOut, and zero all fail this blocking wrapper
    emitter.instruction("add QWORD PTR [rbp - 80], rax");                       // retain progress for the next write iteration
    emitter.instruction("jmp __rt_https_open_write_x86");                       // continue until the complete request is sent

    // -- read the whole TLS-decrypted response into _https_resp_buf --
    emitter.label("__rt_https_open_read_start_x86");
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // accumulated response length = 0
    emitter.label("__rt_https_open_read_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // TLS handle for the read
    abi::emit_symbol_address(emitter, "rsi", "_https_resp_buf");                // response buffer base
    emitter.instruction("add rsi, QWORD PTR [rbp - 56]");                       // read past the bytes already buffered
    emitter.instruction(&format!("mov rdx, {}", HTTPS_RESP_BUF_SIZE));          // response buffer capacity
    emitter.instruction("sub rdx, QWORD PTR [rbp - 56]");                       // remaining buffer capacity
    emitter.instruction("cmp rdx, 0");                                          // is the response buffer full?
    emitter.instruction("jle __rt_https_open_tls_failed_x86");                  // fail instead of returning a truncated response body
    abi::emit_load_symbol_to_reg(emitter, "r9", "_elephc_tls_read_fn", 0);      // load the elephc_tls_read entry pointer
    emitter.emit_published_bridge_call("r9");                                  // read through the published TLS adapter
    emitter.instruction("test rax, rax");                                       // distinguish a clean EOF from every TLS error sentinel
    emitter.instruction("js __rt_https_open_tls_failed_x86");                   // terminal, WouldBlock, and TimedOut reads must not become EOF
    emitter.instruction("jz __rt_https_open_read_done_x86");                    // a zero-byte TLS read is the sole successful EOF
    emitter.instruction("mov r10, QWORD PTR [rbp - 56]");                       // reload the accumulated response length
    emitter.instruction("add r10, rax");                                        // advance by the bytes just read
    emitter.instruction("mov QWORD PTR [rbp - 56], r10");                       // store the updated response length
    emitter.instruction("jmp __rt_https_open_read_x86");                        // continue reading the TLS response
    emitter.label("__rt_https_open_read_done_x86");
    emitter.instruction("mov QWORD PTR [rbp - 80], 0");                         // mark clean TLS completion before closing the session
    emitter.instruction("jmp __rt_https_open_tls_close_x86");                   // release the TLS handle before materializing the stream
    emitter.label("__rt_https_open_tls_failed_x86");
    emitter.instruction("mov QWORD PTR [rbp - 80], 1");                         // retain failure state across TLS cleanup

    // -- elephc_tls_close(handle): send close_notify and drop the session --
    emitter.label("__rt_https_open_tls_close_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // TLS handle for the close
    abi::emit_load_symbol_to_reg(emitter, "r9", "_elephc_tls_close_fn", 0);     // load the elephc_tls_close entry pointer
    emitter.emit_published_bridge_call("r9");                                  // close through the published TLS adapter
    emitter.instruction("cmp QWORD PTR [rbp - 80], 0");                         // did request/response I/O fail before this cleanup?
    emitter.instruction("jne __rt_https_open_fail_x86");                        // never materialize a partial response after TLS I/O failed

    // -- scan for the CRLFCRLF that separates headers from the body --
    abi::emit_symbol_address(emitter, "r8", "_https_resp_buf");                 // response buffer base
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

    // -- write the complete body into the temporary stream --
    emitter.instruction("mov QWORD PTR [rbp - 80], 0");                         // temporary-file write offset starts at zero
    emitter.label("__rt_https_open_body_write_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 80]");                       // reload bytes already copied to the temporary file
    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");                       // reload the full response length
    emitter.instruction("sub r11, QWORD PTR [rbp - 64]");                       // derive the complete body length
    emitter.instruction("cmp r10, r11");                                        // has the full response body reached the tempfile?
    emitter.instruction("jge __rt_https_open_body_written_x86");                // continue only after every body byte was persisted
    emitter.instruction("mov rdi, QWORD PTR [rbp - 72]");                       // temp-file descriptor for the write
    abi::emit_symbol_address(emitter, "rsi", "_https_resp_buf");                // response buffer base
    emitter.instruction("add rsi, QWORD PTR [rbp - 64]");                       // point at the beginning of the response body
    emitter.instruction("add rsi, r10");                                        // advance past body bytes already copied
    emitter.instruction("mov rdx, r11");                                        // reload the full body length
    emitter.instruction("sub rdx, r10");                                        // pass the remaining body suffix
    emitter.instruction("call write");                                          // copy the body into the temp file
    emitter.instruction("test rax, rax");                                       // tempfile writes must make forward progress
    emitter.instruction("jle __rt_https_open_temp_fail_x86");                   // close the tempfile instead of returning partial bytes
    emitter.instruction("add QWORD PTR [rbp - 80], rax");                       // retain progress for the next copy iteration
    emitter.instruction("jmp __rt_https_open_body_write_x86");                  // continue until the entire body is copied
    emitter.label("__rt_https_open_body_written_x86");

    // -- lseek(temp, 0, SEEK_SET): rewind so the stream reads from the start --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 72]");                       // reload the temp-file descriptor
    emitter.instruction("xor esi, esi");                                        // offset = 0
    emitter.instruction("xor edx, edx");                                        // whence = SEEK_SET
    emitter.instruction("call lseek");                                          // rewind the temp file
    emitter.instruction("test rax, rax");                                       // a negative seek result makes the temporary stream unusable
    emitter.instruction("js __rt_https_open_temp_fail_x86");                    // close the unreadable temporary stream before failing

    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // return the rewound body descriptor
    emitter.instruction("add rsp, 192");                                        // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the https:// stream descriptor

    emitter.label("__rt_https_open_temp_fail_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 72]");                       // close the temporary descriptor before reporting failure
    emitter.instruction("call close");                                          // release the anonymous temporary stream after a failed copy or rewind
    emitter.instruction("jmp __rt_https_open_fail_x86");                        // reuse the common failure epilogue after cleanup

    emitter.label("__rt_https_open_fail_x86");
    emitter.instruction("mov rax, -1");                                         // -1 signals a failed https:// open
    emitter.instruction("add rsp, 192");                                        // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the failure result
}

#[cfg(test)]
mod tests {
    use crate::codegen_support::emit::Emitter;
    use crate::codegen_support::platform::{Arch, Platform, Target};

    use super::*;

    /// Verifies every Windows x86_64 HTTPS call uses the published TLS adapter.
    ///
    /// The pointer slots expose compiler-runtime-ABI adapters, so the four
    /// connect/write/read/close sites must stay as direct calls through `r9`
    /// instead of applying the native C-ABI shim a second time.
    #[test]
    fn test_windows_x86_64_https_native_bridge_calls_use_shim() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_https(&mut emitter);
        let asm = emitter.output();

        assert_eq!(asm.matches("call r9").count(), 4, "all four TLS calls must use the published adapters");
        assert!(!asm.contains("call r11"), "published adapters must not be wrapped as native C callbacks");
        assert!(!asm.contains("mov r11, r9"), "published adapters already expose the runtime ABI");
    }

    /// Verifies Linux HTTPS keeps direct calls through published bridge slots.
    #[test]
    fn test_linux_x86_64_https_calls_stay_bare() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_https(&mut emitter);
        let asm = emitter.output();

        assert!(asm.contains("    call r9\n"), "linux keeps the bare published-slot call");
        assert!(!asm.contains("call r11"), "linux must not emit the windows native-bridge shim");
    }

    /// Pins the policy-aware options layout and combinable context lookups on both backends.
    #[test]
    fn https_builds_policy_options_without_legacy_dispatch() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_https(&mut emitter);
            let asm = emitter.output();

            assert!(asm.contains("_elephc_tls_connect_with_options_fn"));
            assert!(!asm.contains("_elephc_tls_connect_insecure_fn"));
            assert!(!asm.contains("_elephc_tls_connect_cafile_fn"));
            assert!(!asm.contains("_elephc_tls_connect_capath_fn"));
            assert!(!asm.contains("_elephc_tls_connect_peer_name_fn"));
            assert_eq!(asm.matches("__rt_get_bool_context_option").count(), 3);
            assert_eq!(asm.matches("__rt_get_string_context_option").count(), 5);
            for key in [
                "_ssl_peer_name_key_str",
                "_ssl_cafile_key_str",
                "_ssl_capath_key_str",
                "_ssl_local_cert_key_str",
                "_ssl_local_pk_key_str",
            ] {
                assert!(asm.contains(key), "{key} must populate the shared options struct");
            }
        }
    }

    /// Pins every byte offset in the stable 88-byte TLS client options ABI.
    #[test]
    fn https_tls_options_offsets_match_bridge_abi() {
        let mut arm = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_https(&mut arm);
        let arm_asm = arm.output();
        for offset in [48, 52, 56, 64, 72, 80, 88, 96, 104, 112, 120, 128] {
            assert!(arm_asm.contains(&format!("[sp, #{offset}]")));
        }

        let mut x86 = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_https(&mut x86);
        let x86_asm = x86.output();
        for offset in [176, 172, 168, 160, 152, 144, 136, 128, 120, 112, 104, 96] {
            assert!(x86_asm.contains(&format!("[rbp - {offset}]")));
        }
    }

    /// Verifies HTTPS only returns a tempfile after complete request and body I/O.
    #[test]
    fn https_rejects_tls_errors_and_partial_tempfiles() {
        let mut arm = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_https(&mut arm);
        let arm_asm = arm.output();
        assert!(arm_asm.contains("__rt_https_open_write:"));
        assert!(arm_asm.contains("__rt_https_open_tls_failed:"));
        assert!(arm_asm.contains("b.lt __rt_https_open_tls_failed"));
        assert!(arm_asm.contains("__rt_https_open_body_write:"));
        assert!(arm_asm.contains("__rt_https_open_temp_fail:"));
        assert!(!arm_asm.contains("b.le __rt_https_open_read_done"));

        let mut x86 = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_https(&mut x86);
        let x86_asm = x86.output();
        assert!(x86_asm.contains("__rt_https_open_write_x86:"));
        assert!(x86_asm.contains("__rt_https_open_tls_failed_x86:"));
        assert!(x86_asm.contains("js __rt_https_open_tls_failed_x86"));
        assert!(x86_asm.contains("__rt_https_open_body_write_x86:"));
        assert!(x86_asm.contains("__rt_https_open_temp_fail_x86:"));
        assert!(!x86_asm.contains("jle __rt_https_open_read_done_x86"));
    }
}
