//! Purpose:
//! Emits the `http://` wrapper runtime helper `__rt_http_open`.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::io`.
//! - `__rt_http_open` is the entry point invoked by the `http://` `fopen` lowering.
//!
//! Key details:
//! - v1 issues a single HTTP/1.0 `GET` over a plain TCP connection: connect,
//!   send the compile-time-built request, read the whole response until the
//!   server closes, locate the `CRLFCRLF` header/body separator, and copy the
//!   body into an anonymous temp file whose descriptor is the readable stream.
//! - The response is buffered in the 1 MiB `_http_resp_buf`; a larger response
//!   is truncated. HTTP/1.0 + `Connection: close` keeps the body close-framed,
//!   so no chunked-transfer decoding is needed.
//! - Reuses `__rt_stream_socket_client` to connect and `__rt_tmpfile` to back
//!   the returned descriptor.

use crate::codegen::{abi, emit::Emitter, platform::Arch};

/// Capacity of the `_http_resp_buf` response buffer, in bytes.
const HTTP_RESP_BUF_SIZE: i64 = 1048576;

/// Emits the `__rt_http_open` runtime helper.
/// Input:  AArch64 x0/x1 = TCP address, x2/x3 = HTTP request text.
///         x86_64  rdi/rsi = TCP address, rdx/rcx = HTTP request text.
/// Output: a readable descriptor for the response body, or -1 on failure.
pub fn emit_http(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_http_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: http_open ---");
    emitter.label_global("__rt_http_open");

    // Frame (80 bytes): [0]=socket fd [8]=request ptr [16]=request len
    //                   [24]=response len [32]=body start [40]=temp fd
    //                   [48]=saved addr_ptr [56]=saved addr_len.
    // The addr is saved so the follow_location loop below can re-connect
    // on each iteration without losing the original target address.
    emitter.instruction("sub sp, sp, #80");                                     // allocate the helper frame
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #48]");                                   // save the addr pointer (for redirect re-connects)
    emitter.instruction("str x1, [sp, #56]");                                   // save the addr length
    emitter.instruction("str x2, [sp, #8]");                                    // save the HTTP request pointer
    emitter.instruction("str x3, [sp, #16]");                                   // save the HTTP request length

    // -- Top of the follow_location loop. Each iteration re-runs the
    //    connect/send/read sequence with the current request bytes (which
    //    can be rewritten by the redirect-handling block below). --
    emitter.label("__rt_http_open_loop_top_aarch64");

    // -- if [http][proxy] was set, override the connect target with the
    //    proxy address (e.g. "tcp://proxy:8080"). The request line was
    //    already built with the target URL in absolute form when
    //    request_fulluri (or proxy itself) was truthy, so the proxy
    //    will see the full URI and forward it. --
    emitter.instruction("ldr x0, [sp, #48]");                                   // reload original addr ptr
    emitter.instruction("ldr x1, [sp, #56]");                                   // reload original addr len
    abi::emit_symbol_address(emitter, "x9", "_http_active_proxy_len");
    emitter.instruction("ldr x10, [x9]");                                       // load runtime value
    emitter.instruction("cbz x10, __rt_http_open_no_proxy_aarch64");            // branch when the checked value is zero or equal
    abi::emit_symbol_address(emitter, "x9", "_http_active_proxy_ptr");
    emitter.instruction("ldr x0, [x9]");                                        // proxy addr ptr
    emitter.instruction("mov x1, x10");                                         // proxy addr len
    emitter.label("__rt_http_open_no_proxy_aarch64");
    // -- connect the TCP socket (x0/x1 hold the address — possibly overridden by proxy above) --
    emitter.instruction("bl __rt_stream_socket_client");                        // connect to the HTTP server, x0 = fd
    emitter.instruction("cmp x0, #0");                                          // did the connection fail?
    emitter.instruction("b.lt __rt_http_open_fail");                            // propagate the failure
    emitter.instruction("str x0, [sp, #0]");                                    // save the connected socket descriptor

    // -- fire STREAM_NOTIFY_CONNECT (code 2) for the context notification --
    emitter.instruction("mov x0, #2");                                          // notification code 2 = STREAM_NOTIFY_CONNECT
    emitter.instruction("mov x1, #0");                                          // severity 0 = STREAM_NOTIFY_SEVERITY_INFO
    emitter.instruction("mov x2, #0");                                          // no message string for a connect event
    emitter.instruction("mov x3, #0");                                          // message length 0
    emitter.instruction("mov x4, #0");                                          // bytes_transferred 0 at connect time
    emitter.instruction("mov x5, #0");                                          // bytes_max unknown (0)
    emitter.instruction("bl __rt_http_fire_notification");                      // invoke the registered notification callback (no-op if none)
    emitter.instruction("ldr x0, [sp, #0]");                                    // restore fd into x0 (the shim clobbered it; the send relies on it)

    // -- if [http][timeout] was set (seconds > 0), apply SO_RCVTIMEO so
    //    slow servers don't hang the read loop forever. --
    abi::emit_symbol_address(emitter, "x9", "_http_active_timeout_seconds");
    emitter.instruction("ldr x10, [x9]");                                       // load runtime value
    emitter.instruction("cbz x10, __rt_http_open_skip_timeout_aarch64");        // branch when the checked value is zero or equal
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd
    emitter.instruction("mov x1, x10");                                         // tv_sec
    emitter.instruction("mov x2, #0");                                          // tv_usec
    emitter.instruction("bl __rt_stream_set_timeout");                          // call runtime helper
    emitter.label("__rt_http_open_skip_timeout_aarch64");

    // -- send the HTTP request --
    emitter.instruction("ldr x1, [sp, #8]");                                    // request pointer for the write
    emitter.instruction("ldr x2, [sp, #16]");                                   // request length for the write
    emitter.syscall(4);

    // -- read the whole response into _http_resp_buf --
    emitter.instruction("str xzr, [sp, #24]");                                  // accumulated response length = 0
    emitter.label("__rt_http_open_read");
    emitter.instruction("ldr x0, [sp, #0]");                                    // socket descriptor for the read
    abi::emit_symbol_address(emitter, "x1", "_http_resp_buf");
    emitter.instruction("ldr x9, [sp, #24]");                                   // response bytes already buffered
    emitter.instruction("add x1, x1, x9");                                      // read into the buffer past the buffered bytes
    emitter.instruction(&format!("mov x2, #{}", HTTP_RESP_BUF_SIZE));           // response buffer capacity
    emitter.instruction("subs x2, x2, x9");                                     // remaining buffer capacity
    emitter.instruction("b.le __rt_http_open_read_done");                       // stop when the response buffer is full
    emitter.syscall(3);
    emitter.instruction("cmp x0, #0");                                          // did the read hit EOF or fail?
    emitter.instruction("b.le __rt_http_open_read_done");                       // the server closed the connection
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the accumulated response length
    emitter.instruction("add x9, x9, x0");                                      // advance by the bytes just read
    emitter.instruction("str x9, [sp, #24]");                                   // store the updated response length
    emitter.instruction("b __rt_http_open_read");                               // continue reading the response
    emitter.label("__rt_http_open_read_done");

    // -- close the socket; the whole response is buffered --
    emitter.instruction("ldr x0, [sp, #0]");                                    // the connected socket descriptor
    emitter.syscall(6);

    // -- parse the HTTP status line ("HTTP/1.x SSS …\r\n"). --
    emitter.instruction("ldr x5, [sp, #24]");                                   // response length
    emitter.instruction("cmp x5, #12");                                         // need at least 12 bytes
    emitter.instruction("b.lt __rt_http_open_status_ok_aarch64");               // branch when comparison is below target
    abi::emit_symbol_address(emitter, "x4", "_http_resp_buf");
    emitter.instruction("ldrb w6, [x4, #9]");                                   // status hundreds
    emitter.instruction("sub w6, w6, #48");                                     // reduce runtime pointer or counter
    emitter.instruction("ldrb w7, [x4, #10]");                                  // tens
    emitter.instruction("sub w7, w7, #48");                                     // reduce runtime pointer or counter
    emitter.instruction("ldrb w8, [x4, #11]");                                  // units
    emitter.instruction("sub w8, w8, #48");                                     // reduce runtime pointer or counter
    emitter.instruction("mov w9, #100");                                        // move runtime value between registers
    emitter.instruction("mul w6, w6, w9");                                      // compute scaled runtime value
    emitter.instruction("mov w9, #10");                                         // move runtime value between registers
    emitter.instruction("madd w6, w7, w9, w6");                                 // compute scaled runtime value
    emitter.instruction("add w6, w6, w8");                                      // status code in w6

    // -- follow_location: 3xx with max_redirects > 0 → find Location,
    //    rebuild request with original host + new path, loop back. --
    emitter.instruction("cmp w6, #300");                                        // compare runtime values for the next branch
    emitter.instruction("b.lt __rt_http_open_check_err_aarch64");               // branch when comparison is below target
    emitter.instruction("cmp w6, #400");                                        // compare runtime values for the next branch
    emitter.instruction("b.ge __rt_http_open_check_err_aarch64");               // branch when comparison is at least target
    abi::emit_symbol_address(emitter, "x9", "_http_active_max_redirects");
    emitter.instruction("ldr x10, [x9]");                                       // load runtime value
    emitter.instruction("cbz x10, __rt_http_open_check_err_aarch64");           // no remaining hops → no redirect
    // Scan for "Location:" / "location:" via case-folded byte compare.
    // Loop: for i in 0 .. resp_len-9: if bytes[i..i+9] match "location:" (lower),
    // skip optional ws, copy until '\r' to _http_redirect_path_buf.
    emitter.instruction("mov x11, #0");                                         // i
    emitter.label("__rt_http_open_loc_scan_aarch64");
    emitter.instruction("add x12, x11, #9");                                    // advance runtime pointer or counter
    emitter.instruction("cmp x12, x5");                                         // compare runtime values for the next branch
    emitter.instruction("b.gt __rt_http_open_check_err_aarch64");               // ran past end → no Location found
    // Compare bytes[i] | 0x20 with 'l', etc. (case-fold uppercase to lowercase).
    let cmp_lc = |emitter: &mut Emitter, ofs: u32, ch: u32| {
        emitter.instruction(&format!("ldrb w13, [x4, x11]"));                   // load base byte
        emitter.instruction(&format!("add x14, x11, #{}", ofs));                // i + ofs
        emitter.instruction(&format!("ldrb w13, [x4, x14]"));                   // load runtime value
        emitter.instruction("orr w13, w13, #0x20");                             // to lowercase
        emitter.instruction(&format!("cmp w13, #{}", ch));                      // compare runtime values for the next branch
        emitter.instruction("b.ne __rt_http_open_loc_next_aarch64");            // branch when the checked value is nonzero or different
    };
    cmp_lc(emitter, 0, 0x6c);                                                   // 'l'
    cmp_lc(emitter, 1, 0x6f);                                                   // 'o'
    cmp_lc(emitter, 2, 0x63);                                                   // 'c'
    cmp_lc(emitter, 3, 0x61);                                                   // 'a'
    cmp_lc(emitter, 4, 0x74);                                                   // 't'
    cmp_lc(emitter, 5, 0x69);                                                   // 'i'
    cmp_lc(emitter, 6, 0x6f);                                                   // 'o'
    cmp_lc(emitter, 7, 0x6e);                                                   // 'n'
    // 8th char must be ':' (no case-fold).
    emitter.instruction("add x14, x11, #8");                                    // advance runtime pointer or counter
    emitter.instruction("ldrb w13, [x4, x14]");                                 // load runtime value
    emitter.instruction("cmp w13, #58");                                        // ':'
    emitter.instruction("b.ne __rt_http_open_loc_next_aarch64");                // branch when the checked value is nonzero or different
    // Match: skip "Location:" + any leading ws. value starts at i+9 (skip ws).
    emitter.instruction("add x12, x11, #9");                                    // advance runtime pointer or counter
    emitter.label("__rt_http_open_loc_skip_ws_aarch64");
    emitter.instruction("cmp x12, x5");                                         // compare runtime values for the next branch
    emitter.instruction("b.ge __rt_http_open_check_err_aarch64");               // ran past end → bail
    emitter.instruction("ldrb w13, [x4, x12]");                                 // load runtime value
    emitter.instruction("cmp w13, #32");                                        // space
    emitter.instruction("b.eq __rt_http_open_loc_advance_ws_aarch64");          // branch when the checked value is zero or equal
    emitter.instruction("cmp w13, #9");                                         // tab
    emitter.instruction("b.ne __rt_http_open_loc_copy_aarch64");                // branch when the checked value is nonzero or different
    emitter.label("__rt_http_open_loc_advance_ws_aarch64");
    emitter.instruction("add x12, x12, #1");                                    // advance runtime pointer or counter
    emitter.instruction("b __rt_http_open_loc_skip_ws_aarch64");                // continue at target label
    emitter.label("__rt_http_open_loc_copy_aarch64");
    // Copy bytes[x12..] until '\r' or '\n' into _http_redirect_path_buf,
    // capped at 2047 bytes.
    abi::emit_symbol_address(emitter, "x15", "_http_redirect_path_buf");
    emitter.instruction("mov x16, #0");                                         // write index
    emitter.label("__rt_http_open_loc_copy_loop_aarch64");
    emitter.instruction("cmp x12, x5");                                         // compare runtime values for the next branch
    emitter.instruction("b.ge __rt_http_open_loc_copy_done_aarch64");           // branch when comparison is at least target
    emitter.instruction("cmp x16, #2047");                                      // compare runtime values for the next branch
    emitter.instruction("b.ge __rt_http_open_loc_copy_done_aarch64");           // branch when comparison is at least target
    emitter.instruction("ldrb w13, [x4, x12]");                                 // load runtime value
    emitter.instruction("cmp w13, #13");                                        // '\r'
    emitter.instruction("b.eq __rt_http_open_loc_copy_done_aarch64");           // branch when the checked value is zero or equal
    emitter.instruction("cmp w13, #10");                                        // '\n'
    emitter.instruction("b.eq __rt_http_open_loc_copy_done_aarch64");           // branch when the checked value is zero or equal
    emitter.instruction("strb w13, [x15, x16]");                                // store runtime value
    emitter.instruction("add x12, x12, #1");                                    // advance runtime pointer or counter
    emitter.instruction("add x16, x16, #1");                                    // advance runtime pointer or counter
    emitter.instruction("b __rt_http_open_loc_copy_loop_aarch64");              // continue at target label
    emitter.label("__rt_http_open_loc_copy_done_aarch64");
    // Store length.
    abi::emit_symbol_address(emitter, "x9", "_http_redirect_path_len");
    emitter.instruction("str x16, [x9]");                                       // store runtime value
    emitter.instruction("cbz x16, __rt_http_open_check_err_aarch64");           // empty → bail
    // Two redirect shapes are accepted: a relative path starting with '/' (use
    // as-is) or an absolute "http://host[:port]/path" URL whose host:port
    // matches the active host (rewrite buffer to just the path, then use as a
    // relative redirect). Cross-host or scheme-changing redirects are still
    // bailed: re-resolving DNS / re-opening the TCP socket / promoting to TLS
    // is out of scope for v1.
    emitter.instruction("ldrb w13, [x15]");                                     // first byte of redirect target
    emitter.instruction("cmp w13, #47");                                        // '/'
    emitter.instruction("b.eq __rt_http_open_loc_abs_done_aarch64");            // relative path → already canonical
    // Try "http://" prefix (case-insensitive ASCII).
    emitter.instruction("cmp x16, #7");                                         // buffer must hold at least the 7-byte prefix
    emitter.instruction("b.lt __rt_http_open_check_err_aarch64");               // branch when comparison is below target
    emitter.instruction("orr w13, w13, #0x20");                                 // fold 'H' → 'h'
    emitter.instruction("cmp w13, #0x68");                                      // 'h'
    emitter.instruction("b.ne __rt_http_open_check_err_aarch64");               // branch when the checked value is nonzero or different
    emitter.instruction("ldrb w13, [x15, #1]");                                 // load runtime value
    emitter.instruction("orr w13, w13, #0x20");                                 // combine runtime bit flags
    emitter.instruction("cmp w13, #0x74");                                      // 't'
    emitter.instruction("b.ne __rt_http_open_check_err_aarch64");               // branch when the checked value is nonzero or different
    emitter.instruction("ldrb w13, [x15, #2]");                                 // load runtime value
    emitter.instruction("orr w13, w13, #0x20");                                 // combine runtime bit flags
    emitter.instruction("cmp w13, #0x74");                                      // 't'
    emitter.instruction("b.ne __rt_http_open_check_err_aarch64");               // branch when the checked value is nonzero or different
    emitter.instruction("ldrb w13, [x15, #3]");                                 // load runtime value
    emitter.instruction("orr w13, w13, #0x20");                                 // combine runtime bit flags
    emitter.instruction("cmp w13, #0x70");                                      // 'p'
    emitter.instruction("b.ne __rt_http_open_check_err_aarch64");               // branch when the checked value is nonzero or different
    emitter.instruction("ldrb w13, [x15, #4]");                                 // load runtime value
    emitter.instruction("cmp w13, #58");                                        // ':'
    emitter.instruction("b.ne __rt_http_open_check_err_aarch64");               // branch when the checked value is nonzero or different
    emitter.instruction("ldrb w13, [x15, #5]");                                 // load runtime value
    emitter.instruction("cmp w13, #47");                                        // '/'
    emitter.instruction("b.ne __rt_http_open_check_err_aarch64");               // branch when the checked value is nonzero or different
    emitter.instruction("ldrb w13, [x15, #6]");                                 // load runtime value
    emitter.instruction("cmp w13, #47");                                        // '/'
    emitter.instruction("b.ne __rt_http_open_check_err_aarch64");               // branch when the checked value is nonzero or different
    // "http://" matched. Compare host bytes against the active host:port.
    abi::emit_symbol_address(emitter, "x9", "_http_active_host_len");
    emitter.instruction("ldr x11, [x9]");                                       // x11 = active host length (with port if any)
    emitter.instruction("add x12, x11, #7");                                    // 7 + host_len = minimum required buffer length
    emitter.instruction("cmp x16, x12");                                        // compare runtime values for the next branch
    emitter.instruction("b.lt __rt_http_open_check_err_aarch64");               // buffer too short to contain a same-host URL
    abi::emit_symbol_address(emitter, "x9", "_http_active_host_ptr");
    emitter.instruction("ldr x14, [x9]");                                       // x14 = active host ptr
    emitter.instruction("mov x12, #0");                                         // host compare index
    emitter.label("__rt_http_open_loc_host_cmp_aarch64");
    emitter.instruction("cmp x12, x11");                                        // compare runtime values for the next branch
    emitter.instruction("b.ge __rt_http_open_loc_host_ok_aarch64");             // branch when comparison is at least target
    emitter.instruction("add x9, x12, #7");                                     // buffer offset = 7 + i
    emitter.instruction("ldrb w13, [x15, x9]");                                 // redirect buf byte
    emitter.instruction("ldrb w8, [x14, x12]");                                 // active host byte
    emitter.instruction("cmp w13, w8");                                         // compare runtime values for the next branch
    emitter.instruction("b.ne __rt_http_open_check_err_aarch64");               // host differs → don't follow cross-host
    emitter.instruction("add x12, x12, #1");                                    // advance runtime pointer or counter
    emitter.instruction("b __rt_http_open_loc_host_cmp_aarch64");               // continue at target label
    emitter.label("__rt_http_open_loc_host_ok_aarch64");
    // The active host stored by http_build_request omits the URL port, so the
    // byte after the matched host bytes may be ':' (port) or '/' (path). Skip
    // an optional ":NNN" port literal — any non-digit, non-'/' delimiter here
    // is a host mismatch (e.g. host suffix like .example.com).
    emitter.instruction("add x12, x11, #7");                                    // index of byte right after the matched host
    emitter.instruction("cmp x12, x16");                                        // compare runtime values for the next branch
    emitter.instruction("b.eq __rt_http_open_loc_no_path_aarch64");             // bare host with no port nor path
    emitter.instruction("ldrb w13, [x15, x12]");                                // load runtime value
    emitter.instruction("cmp w13, #47");                                        // '/'
    emitter.instruction("b.eq __rt_http_open_loc_have_path_aarch64");           // branch when the checked value is zero or equal
    emitter.instruction("cmp w13, #58");                                        // ':'
    emitter.instruction("b.ne __rt_http_open_check_err_aarch64");               // not ':' or '/' → host suffix mismatch
    emitter.instruction("add x12, x12, #1");                                    // skip past ':'
    emitter.label("__rt_http_open_loc_skip_port_aarch64");
    emitter.instruction("cmp x12, x16");                                        // compare runtime values for the next branch
    emitter.instruction("b.ge __rt_http_open_loc_no_path_aarch64");             // port with no path → use "/"
    emitter.instruction("ldrb w13, [x15, x12]");                                // load runtime value
    emitter.instruction("cmp w13, #47");                                        // '/'
    emitter.instruction("b.eq __rt_http_open_loc_have_path_aarch64");           // branch when the checked value is zero or equal
    emitter.instruction("cmp w13, #48");                                        // '0'
    emitter.instruction("b.lt __rt_http_open_check_err_aarch64");               // non-digit in port → reject
    emitter.instruction("cmp w13, #57");                                        // '9'
    emitter.instruction("b.gt __rt_http_open_check_err_aarch64");               // branch when comparison is above target
    emitter.instruction("add x12, x12, #1");                                    // advance runtime pointer or counter
    emitter.instruction("b __rt_http_open_loc_skip_port_aarch64");              // continue at target label
    emitter.label("__rt_http_open_loc_have_path_aarch64");
    // Memmove buf[path_start..len] left to buf[0..].
    emitter.instruction("sub x9, x16, x12");                                    // new length = old length - path_start
    emitter.instruction("mov x10, #0");                                         // write index
    emitter.label("__rt_http_open_loc_shift_aarch64");
    emitter.instruction("cmp x10, x9");                                         // compare runtime values for the next branch
    emitter.instruction("b.ge __rt_http_open_loc_shift_done_aarch64");          // branch when comparison is at least target
    emitter.instruction("add x8, x12, x10");                                    // advance runtime pointer or counter
    emitter.instruction("ldrb w13, [x15, x8]");                                 // src byte at path_start + i
    emitter.instruction("strb w13, [x15, x10]");                                // dst byte at i
    emitter.instruction("add x10, x10, #1");                                    // advance runtime pointer or counter
    emitter.instruction("b __rt_http_open_loc_shift_aarch64");                  // continue at target label
    emitter.label("__rt_http_open_loc_shift_done_aarch64");
    emitter.instruction("mov x16, x9");                                         // commit new buffer length
    emitter.instruction("b __rt_http_open_loc_abs_done_aarch64");               // continue at target label
    emitter.label("__rt_http_open_loc_no_path_aarch64");
    // Absolute URL with no explicit path → treat as "/".
    emitter.instruction("mov w13, #47");                                        // '/'
    emitter.instruction("strb w13, [x15]");                                     // store runtime value
    emitter.instruction("mov x16, #1");                                         // move runtime value between registers
    emitter.label("__rt_http_open_loc_abs_done_aarch64");
    // Persist the (possibly-rewritten) redirect path length so the rebuild
    // request below sees the path-only length, not the absolute-URL length.
    abi::emit_symbol_address(emitter, "x9", "_http_redirect_path_len");
    emitter.instruction("str x16, [x9]");                                       // store runtime value
    // Decrement max_redirects.
    abi::emit_symbol_address(emitter, "x9", "_http_active_max_redirects");
    emitter.instruction("ldr x10, [x9]");                                       // load runtime value
    emitter.instruction("sub x10, x10, #1");                                    // reduce runtime pointer or counter
    emitter.instruction("str x10, [x9]");                                       // store runtime value
    // Rebuild request with original host + new path.
    abi::emit_symbol_address(emitter, "x9", "_http_active_host_ptr");
    emitter.instruction("ldr x0, [x9]");                                        // load runtime value
    abi::emit_symbol_address(emitter, "x9", "_http_active_host_len");
    emitter.instruction("ldr x1, [x9]");                                        // load runtime value
    abi::emit_symbol_address(emitter, "x2", "_http_redirect_path_buf");
    abi::emit_symbol_address(emitter, "x9", "_http_redirect_path_len");
    emitter.instruction("ldr x3, [x9]");                                        // load runtime value
    emitter.instruction("bl __rt_http_build_request");                          // returns new req_len in x0
    abi::emit_symbol_address(emitter, "x9", "_http_req_scratch");
    emitter.instruction("str x9, [sp, #8]");                                    // request_ptr = _http_req_scratch
    emitter.instruction("str x0, [sp, #16]");                                   // request_len = new length
    emitter.instruction("b __rt_http_open_loop_top_aarch64");                   // loop back: reconnect, send, read
    emitter.label("__rt_http_open_loc_next_aarch64");
    emitter.instruction("add x11, x11, #1");                                    // advance runtime pointer or counter
    emitter.instruction("b __rt_http_open_loc_scan_aarch64");                   // continue at target label

    // -- check status >= 400 for ignore_errors handling. --
    emitter.label("__rt_http_open_check_err_aarch64");
    emitter.instruction("cmp w6, #400");                                        // compare runtime values for the next branch
    emitter.instruction("b.lt __rt_http_open_status_ok_aarch64");               // branch when comparison is below target
    abi::emit_symbol_address(emitter, "x9", "_http_active_ignore_errors");
    emitter.instruction("ldr x10, [x9]");                                       // load runtime value
    emitter.instruction("cbnz x10, __rt_http_open_status_ok_aarch64");          // ignore_errors=1 → keep going
    emitter.instruction("b __rt_http_open_fail");                               // strict mode: report fopen failure
    emitter.label("__rt_http_open_status_ok_aarch64");

    // -- scan for the CRLFCRLF that separates headers from the body --
    abi::emit_symbol_address(emitter, "x4", "_http_resp_buf");
    emitter.instruction("ldr x5, [sp, #24]");                                   // response length
    emitter.instruction("str xzr, [sp, #32]");                                  // body start = 0 when no separator is found
    emitter.instruction("mov x6, #0");                                          // response scan index
    emitter.label("__rt_http_open_scan");
    emitter.instruction("add x7, x6, #4");                                      // index just past a 4-byte separator
    emitter.instruction("cmp x7, x5");                                          // is there room for CRLFCRLF at this index?
    emitter.instruction("b.gt __rt_http_open_body");                            // no separator found: treat all bytes as body
    emitter.instruction("ldrb w8, [x4, x6]");                                   // separator byte 0
    emitter.instruction("cmp w8, #13");                                         // is it carriage return?
    emitter.instruction("b.ne __rt_http_open_scan_next");                       // not a separator start
    emitter.instruction("add x9, x6, #1");                                      // index of separator byte 1
    emitter.instruction("ldrb w8, [x4, x9]");                                   // separator byte 1
    emitter.instruction("cmp w8, #10");                                         // is it line feed?
    emitter.instruction("b.ne __rt_http_open_scan_next");                       // not the separator
    emitter.instruction("add x9, x6, #2");                                      // index of separator byte 2
    emitter.instruction("ldrb w8, [x4, x9]");                                   // separator byte 2
    emitter.instruction("cmp w8, #13");                                         // is it carriage return?
    emitter.instruction("b.ne __rt_http_open_scan_next");                       // not the separator
    emitter.instruction("add x9, x6, #3");                                      // index of separator byte 3
    emitter.instruction("ldrb w8, [x4, x9]");                                   // separator byte 3
    emitter.instruction("cmp w8, #10");                                         // is it line feed?
    emitter.instruction("b.ne __rt_http_open_scan_next");                       // not the separator
    emitter.instruction("add x6, x6, #4");                                      // the body begins just past CRLFCRLF
    emitter.instruction("str x6, [sp, #32]");                                   // save the body start offset
    emitter.instruction("b __rt_http_open_body");                               // headers are stripped
    emitter.label("__rt_http_open_scan_next");
    emitter.instruction("add x6, x6, #1");                                      // advance the scan index
    emitter.instruction("b __rt_http_open_scan");                               // keep scanning for the separator
    emitter.label("__rt_http_open_body");

    // -- back the body with an anonymous temp file --
    emitter.instruction("bl __rt_tmpfile");                                     // create an unlinked temp file, x0 = fd
    emitter.instruction("cmp x0, #0");                                          // did tmpfile fail?
    emitter.instruction("b.lt __rt_http_open_fail");                            // propagate the failure
    emitter.instruction("str x0, [sp, #40]");                                   // save the temp-file descriptor

    // -- write(temp, body, body length) --
    abi::emit_symbol_address(emitter, "x1", "_http_resp_buf");
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

    // -- fire STREAM_NOTIFY_COMPLETED (code 8) before returning the body fd --
    emitter.instruction("ldr x9, [sp, #24]");                                   // accumulated response length
    emitter.instruction("ldr x10, [sp, #32]");                                  // body start offset within the response
    emitter.instruction("sub x4, x9, x10");                                     // bytes_transferred = body length
    emitter.instruction("mov x0, #8");                                          // notification code 8 = STREAM_NOTIFY_COMPLETED
    emitter.instruction("mov x1, #0");                                          // severity 0 = STREAM_NOTIFY_SEVERITY_INFO
    emitter.instruction("mov x2, #0");                                          // no message string for completion
    emitter.instruction("mov x3, #0");                                          // message length 0
    emitter.instruction("mov x5, #0");                                          // bytes_max unknown for a close-framed body (0)
    emitter.instruction("bl __rt_http_fire_notification");                      // invoke the registered notification callback (no-op if none)

    emitter.instruction("ldr x0, [sp, #40]");                                   // return the rewound body descriptor
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the http:// stream descriptor

    emitter.label("__rt_http_open_fail");
    // -- fire STREAM_NOTIFY_FAILURE (code 9, ERR) before returning -1 --
    emitter.instruction("mov x0, #9");                                          // notification code 9 = STREAM_NOTIFY_FAILURE
    emitter.instruction("mov x1, #2");                                          // severity 2 = STREAM_NOTIFY_SEVERITY_ERR
    emitter.instruction("mov x2, #0");                                          // no message string for the failure event
    emitter.instruction("mov x3, #0");                                          // message length 0
    emitter.instruction("mov x4, #0");                                          // bytes_transferred 0 on failure
    emitter.instruction("mov x5, #0");                                          // bytes_max 0
    emitter.instruction("bl __rt_http_fire_notification");                      // invoke the registered notification callback (no-op if none)
    emitter.instruction("mov x0, #-1");                                         // -1 signals a failed http:// open
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the failure result
}

/// Emits the Linux x86_64 stream runtime helper for http.
fn emit_http_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: http_open ---");
    emitter.label_global("__rt_http_open");

    // Frame (rbp-relative, 64 bytes used + 16 padding = 80):
    //   [rbp -  8] socket fd
    //   [rbp - 16] request ptr (updated by follow_location loop)
    //   [rbp - 24] request len (updated by follow_location loop)
    //   [rbp - 32] response len
    //   [rbp - 40] body start
    //   [rbp - 48] temp fd
    //   [rbp - 56] saved addr ptr (for follow_location re-connect)
    //   [rbp - 64] saved addr len
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 80");                                         // reserve the helper spill slots
    emitter.instruction("mov QWORD PTR [rbp - 56], rdi");                       // save addr ptr for redirect re-connect
    emitter.instruction("mov QWORD PTR [rbp - 64], rsi");                       // save addr len
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the HTTP request pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // save the HTTP request length

    // -- top of follow_location loop: each iteration re-runs connect/send/read --
    emitter.label("__rt_http_open_loop_top_x");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // reload addr ptr
    emitter.instruction("mov rsi, QWORD PTR [rbp - 64]");                       // reload addr len

    // -- if [http][proxy] is set, override the connect target with proxy --
    abi::emit_load_symbol_to_reg(emitter, "r10", "_http_active_proxy_len", 0);  // move runtime value between registers
    emitter.instruction("test r10, r10");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_http_open_no_proxy_x");                        // branch when the checked value is zero or equal
    abi::emit_load_symbol_to_reg(emitter, "rdi", "_http_active_proxy_ptr", 0);  // prepare SysV call argument
    emitter.instruction("mov rsi, r10");                                        // prepare SysV call argument
    emitter.label("__rt_http_open_no_proxy_x");
    // -- connect the TCP socket (rdi/rsi hold the address — possibly proxy-overridden) --
    emitter.instruction("call __rt_stream_socket_client");                      // connect to the HTTP server, rax = fd
    emitter.instruction("cmp rax, 0");                                          // did the connection fail?
    emitter.instruction("jl __rt_http_open_fail_x86");                          // propagate the failure
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the connected socket descriptor

    // -- fire STREAM_NOTIFY_CONNECT (code 2) for the context notification --
    emitter.instruction("mov edi, 2");                                          // notification code 2 = STREAM_NOTIFY_CONNECT
    emitter.instruction("xor esi, esi");                                        // severity 0 = STREAM_NOTIFY_SEVERITY_INFO
    emitter.instruction("xor edx, edx");                                        // no message string for a connect event
    emitter.instruction("xor ecx, ecx");                                        // message length 0
    emitter.instruction("xor r8d, r8d");                                        // bytes_transferred 0 at connect time
    emitter.instruction("xor r9d, r9d");                                        // bytes_max unknown (0)
    emitter.instruction("call __rt_http_fire_notification");                    // invoke the registered notification callback (no-op if none)
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // restore fd into rax (the shim clobbered it; the send relies on it)

    // -- if [http][timeout] was set (seconds > 0), apply SO_RCVTIMEO --
    abi::emit_load_symbol_to_reg(emitter, "r9", "_http_active_timeout_seconds", 0); // prepare SysV call argument
    emitter.instruction("test r9, r9");                                         // check whether the runtime value is zero
    emitter.instruction("jz __rt_http_open_skip_timeout_x");                    // branch when the checked value is zero or equal
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // prepare SysV call argument
    emitter.instruction("mov rsi, r9");                                         // tv_sec
    emitter.instruction("xor edx, edx");                                        // tv_usec = 0
    emitter.instruction("call __rt_stream_set_timeout");                        // call runtime helper
    emitter.label("__rt_http_open_skip_timeout_x");

    // -- send the HTTP request --
    emitter.instruction("mov rdi, rax");                                        // socket descriptor for the write
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // request pointer for the write
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // request length for the write
    emitter.instruction("call write");                                          // send the HTTP request through libc write()

    // -- read the whole response into _http_resp_buf --
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // accumulated response length = 0
    emitter.label("__rt_http_open_read_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // socket descriptor for the read
    abi::emit_symbol_address(emitter, "rsi", "_http_resp_buf");                 // response buffer base
    emitter.instruction("add rsi, QWORD PTR [rbp - 32]");                       // read past the bytes already buffered
    emitter.instruction(&format!("mov rdx, {}", HTTP_RESP_BUF_SIZE));           // response buffer capacity
    emitter.instruction("sub rdx, QWORD PTR [rbp - 32]");                       // remaining buffer capacity
    emitter.instruction("cmp rdx, 0");                                          // is the response buffer full?
    emitter.instruction("jle __rt_http_open_read_done_x86");                    // stop when no capacity remains
    emitter.instruction("call read");                                           // read more of the response through libc read()
    emitter.instruction("cmp rax, 0");                                          // did the read hit EOF or fail?
    emitter.instruction("jle __rt_http_open_read_done_x86");                    // the server closed the connection
    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");                        // reload the accumulated response length
    emitter.instruction("add r9, rax");                                         // advance by the bytes just read
    emitter.instruction("mov QWORD PTR [rbp - 32], r9");                        // store the updated response length
    emitter.instruction("jmp __rt_http_open_read_x86");                         // continue reading the response
    emitter.label("__rt_http_open_read_done_x86");

    // -- close the socket; the whole response is buffered --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the connected socket descriptor
    emitter.instruction("call close");                                          // close the HTTP connection

    // -- HTTP status parse. --
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // response length
    emitter.instruction("cmp r10, 12");                                         // compare runtime values for the next branch
    emitter.instruction("jl __rt_http_open_status_ok_x");                       // branch when comparison is below target
    abi::emit_symbol_address(emitter, "r8", "_http_resp_buf");                  // load runtime data address
    emitter.instruction("movzx eax, BYTE PTR [r8 + 9]");                        // hundreds
    emitter.instruction("sub eax, 48");                                         // reduce runtime pointer or counter
    emitter.instruction("movzx r9d, BYTE PTR [r8 + 10]");                       // tens
    emitter.instruction("sub r9d, 48");                                         // reduce runtime pointer or counter
    emitter.instruction("movzx r11d, BYTE PTR [r8 + 11]");                      // units
    emitter.instruction("sub r11d, 48");                                        // reduce runtime pointer or counter
    emitter.instruction("imul eax, eax, 100");                                  // compute scaled runtime value
    emitter.instruction("imul r9d, r9d, 10");                                   // compute scaled runtime value
    emitter.instruction("add eax, r9d");                                        // advance runtime pointer or counter
    emitter.instruction("add eax, r11d");                                       // status in eax

    // -- follow_location: 3xx + max_redirects > 0 → rebuild, reconnect --
    emitter.instruction("cmp eax, 300");                                        // compare runtime values for the next branch
    emitter.instruction("jl __rt_http_open_check_err_x");                       // branch when comparison is below target
    emitter.instruction("cmp eax, 400");                                        // compare runtime values for the next branch
    emitter.instruction("jge __rt_http_open_check_err_x");                      // branch when comparison is at least target
    abi::emit_load_symbol_to_reg(emitter, "r9", "_http_active_max_redirects", 0); // prepare SysV call argument
    emitter.instruction("test r9, r9");                                         // check whether the runtime value is zero
    emitter.instruction("jz __rt_http_open_check_err_x");                       // branch when the checked value is zero or equal
    // Scan for case-folded "location:" header — same pattern as ARM64.
    emitter.instruction("xor r11, r11");                                        // i
    emitter.label("__rt_http_open_loc_scan_x");
    emitter.instruction("lea rcx, [r11 + 9]");                                  // load runtime data address
    emitter.instruction("cmp rcx, r10");                                        // compare runtime values for the next branch
    emitter.instruction("jg __rt_http_open_check_err_x");                       // branch when comparison is above target
    let cmp_lc_x = |emitter: &mut Emitter, ofs: u32, ch: u32| {
        emitter.instruction(&format!("lea rcx, [r11 + {}]", ofs));              // load runtime data address
        emitter.instruction("movzx edx, BYTE PTR [r8 + rcx]");                  // load runtime value
        emitter.instruction("or dl, 0x20");                                     // combine runtime bit flags
        emitter.instruction(&format!("cmp dl, {}", ch));                        // compare runtime values for the next branch
        emitter.instruction("jne __rt_http_open_loc_next_x");                   // branch when the checked value is nonzero or different
    };
    cmp_lc_x(emitter, 0, 0x6c);                                                 // 'l'
    cmp_lc_x(emitter, 1, 0x6f);                                                 // 'o'
    cmp_lc_x(emitter, 2, 0x63);                                                 // 'c'
    cmp_lc_x(emitter, 3, 0x61);                                                 // 'a'
    cmp_lc_x(emitter, 4, 0x74);                                                 // 't'
    cmp_lc_x(emitter, 5, 0x69);                                                 // 'i'
    cmp_lc_x(emitter, 6, 0x6f);                                                 // 'o'
    cmp_lc_x(emitter, 7, 0x6e);                                                 // 'n'
    emitter.instruction("lea rcx, [r11 + 8]");                                  // load runtime data address
    emitter.instruction("movzx edx, BYTE PTR [r8 + rcx]");                      // load runtime value
    emitter.instruction("cmp dl, 58");                                          // ':'
    emitter.instruction("jne __rt_http_open_loc_next_x");                       // branch when the checked value is nonzero or different
    // Skip optional ws after the colon.
    emitter.instruction("lea rcx, [r11 + 9]");                                  // load runtime data address
    emitter.label("__rt_http_open_loc_skip_ws_x");
    emitter.instruction("cmp rcx, r10");                                        // compare runtime values for the next branch
    emitter.instruction("jge __rt_http_open_check_err_x");                      // branch when comparison is at least target
    emitter.instruction("movzx edx, BYTE PTR [r8 + rcx]");                      // load runtime value
    emitter.instruction("cmp dl, 32");                                          // compare runtime values for the next branch
    emitter.instruction("je __rt_http_open_loc_adv_ws_x");                      // branch when the checked value is zero or equal
    emitter.instruction("cmp dl, 9");                                           // compare runtime values for the next branch
    emitter.instruction("jne __rt_http_open_loc_copy_x");                       // branch when the checked value is nonzero or different
    emitter.label("__rt_http_open_loc_adv_ws_x");
    emitter.instruction("inc rcx");                                             // advance runtime pointer or counter
    emitter.instruction("jmp __rt_http_open_loc_skip_ws_x");                    // continue at target label
    emitter.label("__rt_http_open_loc_copy_x");
    abi::emit_symbol_address(emitter, "r12", "_http_redirect_path_buf");        // load runtime data address
    emitter.instruction("xor r13, r13");                                        // write index
    emitter.label("__rt_http_open_loc_copy_loop_x");
    emitter.instruction("cmp rcx, r10");                                        // compare runtime values for the next branch
    emitter.instruction("jge __rt_http_open_loc_copy_done_x");                  // branch when comparison is at least target
    emitter.instruction("cmp r13, 2047");                                       // compare runtime values for the next branch
    emitter.instruction("jge __rt_http_open_loc_copy_done_x");                  // branch when comparison is at least target
    emitter.instruction("movzx edx, BYTE PTR [r8 + rcx]");                      // load runtime value
    emitter.instruction("cmp dl, 13");                                          // compare runtime values for the next branch
    emitter.instruction("je __rt_http_open_loc_copy_done_x");                   // branch when the checked value is zero or equal
    emitter.instruction("cmp dl, 10");                                          // compare runtime values for the next branch
    emitter.instruction("je __rt_http_open_loc_copy_done_x");                   // branch when the checked value is zero or equal
    emitter.instruction("mov BYTE PTR [r12 + r13], dl");                        // store runtime value
    emitter.instruction("inc rcx");                                             // advance runtime pointer or counter
    emitter.instruction("inc r13");                                             // advance runtime pointer or counter
    emitter.instruction("jmp __rt_http_open_loc_copy_loop_x");                  // continue at target label
    emitter.label("__rt_http_open_loc_copy_done_x");
    abi::emit_store_reg_to_symbol(emitter, "r13", "_http_redirect_path_len", 0); // store runtime value
    emitter.instruction("test r13, r13");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_http_open_check_err_x");                       // branch when the checked value is zero or equal
    // Two redirect shapes are accepted: a relative path starting with '/' (use
    // as-is) or an absolute "http://host[:port]/path" URL whose host:port
    // matches the active host (rewrite buffer to just the path). Cross-host
    // and scheme-changing redirects fall through to the bail branch.
    emitter.instruction("movzx edx, BYTE PTR [r12]");                           // first byte of redirect target
    emitter.instruction("cmp dl, 47");                                          // '/'
    emitter.instruction("je __rt_http_open_loc_abs_done_x");                    // relative → already canonical
    emitter.instruction("cmp r13, 7");                                          // need at least "http://"
    emitter.instruction("jl __rt_http_open_check_err_x");                       // branch when comparison is below target
    emitter.instruction("or dl, 0x20");                                         // fold 'H' → 'h'
    emitter.instruction("cmp dl, 0x68");                                        // 'h'
    emitter.instruction("jne __rt_http_open_check_err_x");                      // branch when the checked value is nonzero or different
    emitter.instruction("movzx edx, BYTE PTR [r12 + 1]");                       // load runtime value
    emitter.instruction("or dl, 0x20");                                         // combine runtime bit flags
    emitter.instruction("cmp dl, 0x74");                                        // 't'
    emitter.instruction("jne __rt_http_open_check_err_x");                      // branch when the checked value is nonzero or different
    emitter.instruction("movzx edx, BYTE PTR [r12 + 2]");                       // load runtime value
    emitter.instruction("or dl, 0x20");                                         // combine runtime bit flags
    emitter.instruction("cmp dl, 0x74");                                        // 't'
    emitter.instruction("jne __rt_http_open_check_err_x");                      // branch when the checked value is nonzero or different
    emitter.instruction("movzx edx, BYTE PTR [r12 + 3]");                       // load runtime value
    emitter.instruction("or dl, 0x20");                                         // combine runtime bit flags
    emitter.instruction("cmp dl, 0x70");                                        // 'p'
    emitter.instruction("jne __rt_http_open_check_err_x");                      // branch when the checked value is nonzero or different
    emitter.instruction("movzx edx, BYTE PTR [r12 + 4]");                       // load runtime value
    emitter.instruction("cmp dl, 58");                                          // ':'
    emitter.instruction("jne __rt_http_open_check_err_x");                      // branch when the checked value is nonzero or different
    emitter.instruction("movzx edx, BYTE PTR [r12 + 5]");                       // load runtime value
    emitter.instruction("cmp dl, 47");                                          // '/'
    emitter.instruction("jne __rt_http_open_check_err_x");                      // branch when the checked value is nonzero or different
    emitter.instruction("movzx edx, BYTE PTR [r12 + 6]");                       // load runtime value
    emitter.instruction("cmp dl, 47");                                          // '/'
    emitter.instruction("jne __rt_http_open_check_err_x");                      // branch when the checked value is nonzero or different
    // "http://" matched. Compare host bytes against the active host:port.
    abi::emit_load_symbol_to_reg(emitter, "r9", "_http_active_host_len", 0);    // r9 = active host length
    emitter.instruction("mov rax, r9");                                         // prepare runtime result value
    emitter.instruction("add rax, 7");                                          // 7 + host_len = required min length
    emitter.instruction("cmp r13, rax");                                        // compare runtime values for the next branch
    emitter.instruction("jl __rt_http_open_check_err_x");                       // buffer too short for same-host URL
    abi::emit_load_symbol_to_reg(emitter, "r14", "_http_active_host_ptr", 0);   // r14 = active host ptr
    emitter.instruction("xor rcx, rcx");                                        // host compare index
    emitter.label("__rt_http_open_loc_host_cmp_x");
    emitter.instruction("cmp rcx, r9");                                         // compare runtime values for the next branch
    emitter.instruction("jge __rt_http_open_loc_host_ok_x");                    // branch when comparison is at least target
    emitter.instruction("lea rax, [rcx + 7]");                                  // buf offset = 7 + i
    emitter.instruction("movzx edx, BYTE PTR [r12 + rax]");                     // redirect buf byte
    emitter.instruction("movzx eax, BYTE PTR [r14 + rcx]");                     // active host byte
    emitter.instruction("cmp dl, al");                                          // compare runtime values for the next branch
    emitter.instruction("jne __rt_http_open_check_err_x");                      // host differs → don't follow cross-host
    emitter.instruction("inc rcx");                                             // advance runtime pointer or counter
    emitter.instruction("jmp __rt_http_open_loc_host_cmp_x");                   // continue at target label
    emitter.label("__rt_http_open_loc_host_ok_x");
    // The active host stored by http_build_request omits the URL port, so the
    // byte after the matched host bytes may be ':' (port) or '/' (path). Skip
    // an optional ":NNN" port literal — any non-digit, non-'/' delimiter here
    // is a host mismatch.
    emitter.instruction("mov rax, r9");                                         // prepare runtime result value
    emitter.instruction("add rax, 7");                                          // index right after the matched host
    emitter.instruction("cmp rax, r13");                                        // compare runtime values for the next branch
    emitter.instruction("je __rt_http_open_loc_no_path_x");                     // bare host with no port nor path
    emitter.instruction("movzx edx, BYTE PTR [r12 + rax]");                     // load runtime value
    emitter.instruction("cmp dl, 47");                                          // '/'
    emitter.instruction("je __rt_http_open_loc_have_path_x");                   // branch when the checked value is zero or equal
    emitter.instruction("cmp dl, 58");                                          // ':'
    emitter.instruction("jne __rt_http_open_check_err_x");                      // host suffix mismatch
    emitter.instruction("inc rax");                                             // skip past ':'
    emitter.label("__rt_http_open_loc_skip_port_x");
    emitter.instruction("cmp rax, r13");                                        // compare runtime values for the next branch
    emitter.instruction("jge __rt_http_open_loc_no_path_x");                    // branch when comparison is at least target
    emitter.instruction("movzx edx, BYTE PTR [r12 + rax]");                     // load runtime value
    emitter.instruction("cmp dl, 47");                                          // '/'
    emitter.instruction("je __rt_http_open_loc_have_path_x");                   // branch when the checked value is zero or equal
    emitter.instruction("cmp dl, 48");                                          // '0'
    emitter.instruction("jl __rt_http_open_check_err_x");                       // branch when comparison is below target
    emitter.instruction("cmp dl, 57");                                          // '9'
    emitter.instruction("jg __rt_http_open_check_err_x");                       // branch when comparison is above target
    emitter.instruction("inc rax");                                             // advance runtime pointer or counter
    emitter.instruction("jmp __rt_http_open_loc_skip_port_x");                  // continue at target label
    emitter.label("__rt_http_open_loc_have_path_x");
    // Memmove buf[path_start..len] left to buf[0..].
    emitter.instruction("mov r9, r13");                                         // prepare SysV call argument
    emitter.instruction("sub r9, rax");                                         // new length = old - path_start
    emitter.instruction("xor rcx, rcx");                                        // write index
    emitter.label("__rt_http_open_loc_shift_x");
    emitter.instruction("cmp rcx, r9");                                         // compare runtime values for the next branch
    emitter.instruction("jge __rt_http_open_loc_shift_done_x");                 // branch when comparison is at least target
    emitter.instruction("lea r8, [rax + rcx]");                                 // load runtime data address
    emitter.instruction("movzx edx, BYTE PTR [r12 + r8]");                      // src
    emitter.instruction("mov BYTE PTR [r12 + rcx], dl");                        // dst
    emitter.instruction("inc rcx");                                             // advance runtime pointer or counter
    emitter.instruction("jmp __rt_http_open_loc_shift_x");                      // continue at target label
    emitter.label("__rt_http_open_loc_shift_done_x");
    emitter.instruction("mov r13, r9");                                         // commit new length
    emitter.instruction("jmp __rt_http_open_loc_abs_done_x");                   // continue at target label
    emitter.label("__rt_http_open_loc_no_path_x");
    emitter.instruction("mov BYTE PTR [r12], 47");                              // '/'
    emitter.instruction("mov r13, 1");                                          // move runtime value between registers
    emitter.label("__rt_http_open_loc_abs_done_x");
    abi::emit_store_reg_to_symbol(emitter, "r13", "_http_redirect_path_len", 0); // persist rewritten length
    // Decrement max_redirects.
    abi::emit_dec_symbol(emitter, "_http_active_max_redirects");                // reduce runtime pointer or counter
    // Rebuild request: __rt_http_build_request(host, host_len, redirect_buf, redirect_len).
    abi::emit_load_symbol_to_reg(emitter, "rdi", "_http_active_host_ptr", 0);   // prepare SysV call argument
    abi::emit_load_symbol_to_reg(emitter, "rsi", "_http_active_host_len", 0);   // prepare SysV call argument
    abi::emit_symbol_address(emitter, "rdx", "_http_redirect_path_buf");        // load runtime data address
    abi::emit_load_symbol_to_reg(emitter, "rcx", "_http_redirect_path_len", 0); // prepare SysV call argument
    emitter.instruction("call __rt_http_build_request");                        // returns new req len in rax
    abi::emit_symbol_address(emitter, "r9", "_http_req_scratch");               // load runtime data address
    emitter.instruction("mov QWORD PTR [rbp - 16], r9");                        // store runtime value
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // store runtime value
    emitter.instruction("jmp __rt_http_open_loop_top_x");                       // continue at target label
    emitter.label("__rt_http_open_loc_next_x");
    emitter.instruction("inc r11");                                             // advance runtime pointer or counter
    emitter.instruction("jmp __rt_http_open_loc_scan_x");                       // continue at target label

    // -- check status >= 400 for ignore_errors --
    emitter.label("__rt_http_open_check_err_x");
    emitter.instruction("cmp eax, 400");                                        // compare runtime values for the next branch
    emitter.instruction("jl __rt_http_open_status_ok_x");                       // branch when comparison is below target
    abi::emit_load_symbol_to_reg(emitter, "r9", "_http_active_ignore_errors", 0); // prepare SysV call argument
    emitter.instruction("test r9, r9");                                         // check whether the runtime value is zero
    emitter.instruction("jnz __rt_http_open_status_ok_x");                      // branch when the checked value is nonzero or different
    emitter.instruction("jmp __rt_http_open_fail_x86");                         // continue at target label
    emitter.label("__rt_http_open_status_ok_x");

    // -- scan for the CRLFCRLF that separates headers from the body --
    abi::emit_symbol_address(emitter, "r8", "_http_resp_buf");                  // response buffer base
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // response length
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // body start = 0 when no separator is found
    emitter.instruction("xor rcx, rcx");                                        // response scan index
    emitter.label("__rt_http_open_scan_x86");
    emitter.instruction("lea rax, [rcx + 4]");                                  // index just past a 4-byte separator
    emitter.instruction("cmp rax, r10");                                        // is there room for CRLFCRLF at this index?
    emitter.instruction("jg __rt_http_open_body_x86");                          // no separator found: treat all bytes as body
    emitter.instruction("movzx eax, BYTE PTR [r8 + rcx]");                      // separator byte 0
    emitter.instruction("cmp al, 13");                                          // is it carriage return?
    emitter.instruction("jne __rt_http_open_scan_next_x86");                    // not a separator start
    emitter.instruction("lea rax, [rcx + 1]");                                  // index of separator byte 1
    emitter.instruction("movzx eax, BYTE PTR [r8 + rax]");                      // separator byte 1
    emitter.instruction("cmp al, 10");                                          // is it line feed?
    emitter.instruction("jne __rt_http_open_scan_next_x86");                    // not the separator
    emitter.instruction("lea rax, [rcx + 2]");                                  // index of separator byte 2
    emitter.instruction("movzx eax, BYTE PTR [r8 + rax]");                      // separator byte 2
    emitter.instruction("cmp al, 13");                                          // is it carriage return?
    emitter.instruction("jne __rt_http_open_scan_next_x86");                    // not the separator
    emitter.instruction("lea rax, [rcx + 3]");                                  // index of separator byte 3
    emitter.instruction("movzx eax, BYTE PTR [r8 + rax]");                      // separator byte 3
    emitter.instruction("cmp al, 10");                                          // is it line feed?
    emitter.instruction("jne __rt_http_open_scan_next_x86");                    // not the separator
    emitter.instruction("lea rax, [rcx + 4]");                                  // the body begins just past CRLFCRLF
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // save the body start offset
    emitter.instruction("jmp __rt_http_open_body_x86");                         // headers are stripped
    emitter.label("__rt_http_open_scan_next_x86");
    emitter.instruction("inc rcx");                                             // advance the scan index
    emitter.instruction("jmp __rt_http_open_scan_x86");                         // keep scanning for the separator
    emitter.label("__rt_http_open_body_x86");

    // -- back the body with an anonymous temp file --
    emitter.instruction("call __rt_tmpfile");                                   // create an unlinked temp file, rax = fd
    emitter.instruction("cmp rax, 0");                                          // did tmpfile fail?
    emitter.instruction("jl __rt_http_open_fail_x86");                          // propagate the failure
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the temp-file descriptor

    // -- write(temp, body, body length) --
    emitter.instruction("mov rdi, rax");                                        // temp-file descriptor for the write
    abi::emit_symbol_address(emitter, "rsi", "_http_resp_buf");                 // response buffer base
    emitter.instruction("add rsi, QWORD PTR [rbp - 40]");                       // body pointer = buffer + body start
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // response length
    emitter.instruction("sub rdx, QWORD PTR [rbp - 40]");                       // body length = response length - body start
    emitter.instruction("call write");                                          // copy the body into the temp file

    // -- lseek(temp, 0, SEEK_SET): rewind so the stream reads from the start --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // reload the temp-file descriptor
    emitter.instruction("xor esi, esi");                                        // offset = 0
    emitter.instruction("xor edx, edx");                                        // whence = SEEK_SET
    emitter.instruction("call lseek");                                          // rewind the temp file

    // -- fire STREAM_NOTIFY_COMPLETED (code 8) before returning the body fd --
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");                        // accumulated response length
    emitter.instruction("sub r8, QWORD PTR [rbp - 40]");                        // bytes_transferred = body length (response - body start)
    emitter.instruction("mov edi, 8");                                          // notification code 8 = STREAM_NOTIFY_COMPLETED
    emitter.instruction("xor esi, esi");                                        // severity 0 = STREAM_NOTIFY_SEVERITY_INFO
    emitter.instruction("xor edx, edx");                                        // no message string for completion
    emitter.instruction("xor ecx, ecx");                                        // message length 0
    emitter.instruction("xor r9d, r9d");                                        // bytes_max unknown for a close-framed body (0)
    emitter.instruction("call __rt_http_fire_notification");                    // invoke the registered notification callback (no-op if none)

    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // return the rewound body descriptor
    emitter.instruction("add rsp, 80");                                         // release the helper frame (matches the prologue's sub rsp, 80)
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the http:// stream descriptor

    emitter.label("__rt_http_open_fail_x86");
    // -- fire STREAM_NOTIFY_FAILURE (code 9, ERR) before returning -1 --
    emitter.instruction("mov edi, 9");                                          // notification code 9 = STREAM_NOTIFY_FAILURE
    emitter.instruction("mov esi, 2");                                          // severity 2 = STREAM_NOTIFY_SEVERITY_ERR
    emitter.instruction("xor edx, edx");                                        // no message string for the failure event
    emitter.instruction("xor ecx, ecx");                                        // message length 0
    emitter.instruction("xor r8d, r8d");                                        // bytes_transferred 0 on failure
    emitter.instruction("xor r9d, r9d");                                        // bytes_max 0
    emitter.instruction("call __rt_http_fire_notification");                    // invoke the registered notification callback (no-op if none)
    emitter.instruction("mov rax, -1");                                         // -1 signals a failed http:// open
    emitter.instruction("add rsp, 80");                                         // release the helper frame (matches the prologue's sub rsp, 80)
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the failure result
}
