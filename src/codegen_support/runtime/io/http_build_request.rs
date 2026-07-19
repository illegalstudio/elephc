//! Purpose:
//! Emits the `__rt_http_build_request` runtime helper. Builds an
//! HTTP/1.0 request into `_http_req_scratch`, picking up the method
//! from `_stream_context_options["http"]["method"]` when set, falling
//! back to `"GET"` otherwise. Header and body support are deferred.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::io`.
//! - The http:// `fopen` lowering (`http_stream.rs`) — replaces the
//!   compile-time-built static request with this runtime build so the
//!   stream context can override the request method.
//!
//! Key details:
//! - Output layout in `_http_req_scratch`:
//!     `<method> <path> HTTP/1.0\r\nHost: <host>\r\nConnection: close\r\n\r\n`
//! - Method defaults to `"GET"` (3 bytes) when no context override is
//!   set. Override length is bounded only by the scratch buffer (8 KB).
//! - Returns the total byte count written; the caller fetches the
//!   buffer base via `_http_req_scratch`.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// `__rt_http_build_request`:
/// Input:  AArch64 x0/x1 = host ptr/len, x2/x3 = path ptr/len.
///         x86_64  rdi/rsi = host ptr/len, rdx/rcx = path ptr/len.
/// Output: x0/rax = total bytes written to `_http_req_scratch`.
pub fn emit_http_build_request(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_http_build_request_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: http_build_request ---");
    emitter.label_global("__rt_http_build_request");

    // Frame (256 bytes — bumped to hold all Phase B option slots):
    //   [sp,   0] host_ptr / len (16)
    //   [sp,  16] path_ptr / len (16)
    //   [sp,  32] method_ptr / len (16)
    //   [sp,  48] running write pointer (8)
    //   [sp,  56] header_ptr / len (16)
    //   [sp,  72] header_found flag (8)
    //   [sp,  80] content_ptr / len (16)
    //   [sp,  96] content_found flag (8)
    //   [sp, 104] user_agent_ptr / len (16)
    //   [sp, 120] proto_version_ptr / len (16)
    //   [sp, 136] request_fulluri_ptr / len (16)
    //   [sp, 152] timeout_ptr / len (16)
    //   [sp, 168] ignore_errors_ptr / len (16)
    //   [sp, 184] proxy_ptr / len (16)
    //   [sp, 200] follow_location_ptr / len (16)
    //   [sp, 216] max_redirects_ptr / len (16)
    //   [sp, 240] saved x29
    //   [sp, 248] saved x30
    emitter.instruction("sub sp, sp, #256");                                    // allocate runtime stack frame
    emitter.instruction("stp x29, x30, [sp, #240]");                            // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish runtime frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // host_ptr
    emitter.instruction("str x1, [sp, #8]");                                    // host_len
    emitter.instruction("str x2, [sp, #16]");                                   // path_ptr
    emitter.instruction("str x3, [sp, #24]");                                   // path_len

    // -- expose host to __rt_http_open's follow_location loop. --
    abi::emit_symbol_address(emitter, "x9", "_http_active_host_ptr");
    emitter.instruction("str x0, [x9]");                                        // store runtime value
    abi::emit_symbol_address(emitter, "x9", "_http_active_host_len");
    emitter.instruction("str x1, [x9]");                                        // store runtime value

    // -- preload the default method ("GET", 3) into the method slots.
    //    The lookup overwrites those when it hits. --
    abi::emit_symbol_address(emitter, "x9", "_http_default_method");
    emitter.instruction("str x9, [sp, #32]");                                   // method_ptr = default
    emitter.instruction("mov x9, #3");                                          // move runtime value between registers
    emitter.instruction("str x9, [sp, #40]");                                   // method_len = 3

    // -- look up _stream_context_options["http"]["method"] --
    abi::emit_symbol_address(emitter, "x0", "_http_key_str");                   // wrapper_ptr
    emitter.instruction("mov x1, #4");                                          // strlen("http") = 4
    abi::emit_symbol_address(emitter, "x2", "_http_method_key_str");            // opt_ptr
    emitter.instruction("mov x3, #6");                                          // strlen("method") = 6
    emitter.instruction("add x4, sp, #32");                                     // out_ptr_addr → method_ptr slot
    emitter.instruction("add x5, sp, #40");                                     // out_len_addr → method_len slot
    emitter.instruction("bl __rt_get_string_context_option");                   // 1 hit / 0 miss; slots already pre-filled

    // -- look up _stream_context_options["http"]["header"] --
    emitter.instruction("str xzr, [sp, #56]");                                  // header_ptr = 0
    emitter.instruction("str xzr, [sp, #64]");                                  // header_len = 0
    abi::emit_symbol_address(emitter, "x0", "_http_key_str");
    emitter.instruction("mov x1, #4");                                          // prepare AArch64 call argument
    abi::emit_symbol_address(emitter, "x2", "_http_header_key_str");
    emitter.instruction("mov x3, #6");                                          // strlen("header") = 6
    emitter.instruction("add x4, sp, #56");                                     // header_ptr slot
    emitter.instruction("add x5, sp, #64");                                     // header_len slot
    emitter.instruction("bl __rt_get_string_context_option");                   // call runtime helper
    emitter.instruction("str x0, [sp, #72]");                                   // header_found flag (1 hit / 0 miss)

    // -- look up _stream_context_options["http"]["content"] --
    emitter.instruction("str xzr, [sp, #80]");                                  // content_ptr = 0
    emitter.instruction("str xzr, [sp, #88]");                                  // content_len = 0
    abi::emit_symbol_address(emitter, "x0", "_http_key_str");
    emitter.instruction("mov x1, #4");                                          // prepare AArch64 call argument
    abi::emit_symbol_address(emitter, "x2", "_http_content_key_str");
    emitter.instruction("mov x3, #7");                                          // strlen("content") = 7
    emitter.instruction("add x4, sp, #80");                                     // content_ptr slot
    emitter.instruction("add x5, sp, #88");                                     // content_len slot
    emitter.instruction("bl __rt_get_string_context_option");                   // call runtime helper
    emitter.instruction("str x0, [sp, #96]");                                   // content_found flag

    // -- look up _stream_context_options["http"]["user_agent"] --
    emitter.instruction("str xzr, [sp, #104]");                                 // user_agent_ptr = 0
    emitter.instruction("str xzr, [sp, #112]");                                 // user_agent_len = 0
    abi::emit_symbol_address(emitter, "x0", "_http_key_str");
    emitter.instruction("mov x1, #4");                                          // prepare AArch64 call argument
    abi::emit_symbol_address(emitter, "x2", "_http_user_agent_key_str");
    emitter.instruction("mov x3, #10");                                         // strlen("user_agent")
    emitter.instruction("add x4, sp, #104");                                    // advance runtime pointer or counter
    emitter.instruction("add x5, sp, #112");                                    // advance runtime pointer or counter
    emitter.instruction("bl __rt_get_string_context_option");                   // call runtime helper

    // -- look up _stream_context_options["http"]["protocol_version"] --
    emitter.instruction("str xzr, [sp, #120]");                                 // proto_version_ptr = 0
    emitter.instruction("str xzr, [sp, #128]");                                 // proto_version_len = 0
    abi::emit_symbol_address(emitter, "x0", "_http_key_str");
    emitter.instruction("mov x1, #4");                                          // prepare AArch64 call argument
    abi::emit_symbol_address(emitter, "x2", "_http_protocol_version_key_str");
    emitter.instruction("mov x3, #16");                                         // strlen("protocol_version")
    emitter.instruction("add x4, sp, #120");                                    // advance runtime pointer or counter
    emitter.instruction("add x5, sp, #128");                                    // advance runtime pointer or counter
    emitter.instruction("bl __rt_get_string_context_option");                   // call runtime helper

    // -- batch lookups for the remaining six Phase B HTTP context options.
    //    All six are now read into globals here AND enforced by __rt_http_open:
    //      - request_fulluri: rewrites the request-line target to an absolute
    //        URI (also auto-enabled when proxy is set).
    //      - timeout: setsockopt SO_RCVTIMEO on the connected socket.
    //      - ignore_errors: tolerate 4xx/5xx responses instead of returning
    //        a failed fopen.
    //      - proxy: rewrite the connect target to the proxy address.
    //      - follow_location: enable the redirect loop in __rt_http_open
    //        (relative and same-host absolute URLs are followed; cross-host
    //        redirects are not, by design).
    //      - max_redirects: counter consumed by the redirect loop.
    let lookup_str = |emitter: &mut Emitter, sym: &str, len: i64, ptr_off: i64| {
        emitter.instruction(&format!("str xzr, [sp, #{}]", ptr_off));           // store runtime value
        emitter.instruction(&format!("str xzr, [sp, #{}]", ptr_off + 8));       // store runtime value
        abi::emit_symbol_address(emitter, "x0", "_http_key_str");
        emitter.instruction("mov x1, #4");                                      // prepare AArch64 call argument
        abi::emit_symbol_address(emitter, "x2", sym);
        emitter.instruction(&format!("mov x3, #{}", len));                      // prepare AArch64 call argument
        emitter.instruction(&format!("add x4, sp, #{}", ptr_off));              // advance runtime pointer or counter
        emitter.instruction(&format!("add x5, sp, #{}", ptr_off + 8));          // advance runtime pointer or counter
        emitter.instruction("bl __rt_get_string_context_option");               // call runtime helper
    };
    lookup_str(emitter, "_http_request_fulluri_key_str", 15, 136);              // strlen("request_fulluri") = 15
    lookup_str(emitter, "_http_timeout_key_str", 7, 152);                       // strlen("timeout") = 7
    lookup_str(emitter, "_http_ignore_errors_key_str", 13, 168);                // strlen("ignore_errors") = 13
    lookup_str(emitter, "_http_proxy_key_str", 5, 184);                         // strlen("proxy") = 5
    lookup_str(emitter, "_http_follow_location_key_str", 15, 200);              // strlen("follow_location") = 15
    lookup_str(emitter, "_http_max_redirects_key_str", 13, 216);                // strlen("max_redirects") = 13

    // -- propagate enforcement-relevant options to globals that
    //    __rt_http_open reads later in the same fopen call. --
    //
    // ignore_errors: truthy when ptr non-zero AND first byte != '0'.
    abi::emit_symbol_address(emitter, "x10", "_http_active_ignore_errors");
    emitter.instruction("str xzr, [x10]");                                      // default = 0 (fail-open on 4xx/5xx)
    emitter.instruction("ldr x11, [sp, #176]");                                 // ignore_errors_len
    emitter.instruction("cbz x11, __rt_hbr_skip_ie_aarch64");                   // branch when the checked value is zero or equal
    emitter.instruction("ldr x12, [sp, #168]");                                 // ignore_errors_ptr
    emitter.instruction("ldrb w13, [x12]");                                     // load runtime value
    emitter.instruction("cmp w13, #48");                                        // '0' (falsy)
    emitter.instruction("b.eq __rt_hbr_skip_ie_aarch64");                       // branch when the checked value is zero or equal
    emitter.instruction("mov x14, #1");                                         // move runtime value between registers
    emitter.instruction("str x14, [x10]");                                      // truthy → ignore_errors = 1
    emitter.label("__rt_hbr_skip_ie_aarch64");
    // max_redirects: parse the string as a base-10 unsigned int. If
    // follow_location is unset OR max_redirects is missing, default to 0
    // (which disables redirect-following). 0 also implicitly means
    // "follow_location was off" — http_open just doesn't loop.
    abi::emit_symbol_address(emitter, "x10", "_http_active_max_redirects");
    emitter.instruction("str xzr, [x10]");                                      // default = 0 (no redirects)
    // First check follow_location is truthy; if not, leave max_redirects at 0.
    emitter.instruction("ldr x11, [sp, #208]");                                 // follow_location_len
    emitter.instruction("cbz x11, __rt_hbr_skip_mr_aarch64");                   // branch when the checked value is zero or equal
    emitter.instruction("ldr x12, [sp, #200]");                                 // follow_location_ptr
    emitter.instruction("ldrb w13, [x12]");                                     // load runtime value
    emitter.instruction("cmp w13, #48");                                        // '0' = falsy
    emitter.instruction("b.eq __rt_hbr_skip_mr_aarch64");                       // branch when the checked value is zero or equal
    // follow_location is truthy; now parse max_redirects (default 20 per PHP if
    // follow_location is set but max_redirects is absent).
    emitter.instruction("mov x15, #20");                                        // PHP default cap
    emitter.instruction("ldr x11, [sp, #224]");                                 // max_redirects_len
    emitter.instruction("cbz x11, __rt_hbr_mr_store_aarch64");                  // branch when the checked value is zero or equal
    emitter.instruction("ldr x12, [sp, #216]");                                 // max_redirects_ptr
    // base-10 ascii to int: accumulator x15.
    emitter.instruction("mov x15, #0");                                         // move runtime value between registers
    emitter.instruction("mov x16, #0");                                         // move runtime value between registers
    emitter.label("__rt_hbr_mr_loop_aarch64");
    emitter.instruction("cmp x16, x11");                                        // compare runtime values for the next branch
    emitter.instruction("b.ge __rt_hbr_mr_store_aarch64");                      // branch when comparison is at least target
    emitter.instruction("ldrb w13, [x12, x16]");                                // load runtime value
    emitter.instruction("sub w13, w13, #48");                                   // '0'..'9' → 0..9
    emitter.instruction("cmp w13, #9");                                         // compare runtime values for the next branch
    emitter.instruction("b.hi __rt_hbr_mr_store_aarch64");                      // non-digit terminates
    emitter.instruction("mov x17, #10");                                        // move runtime value between registers
    emitter.instruction("mul x15, x15, x17");                                   // compute scaled runtime value
    emitter.instruction("add x15, x15, x13");                                   // x13's high 32 bits are zero (w-reg sub cleared them); plain 64-bit add
    emitter.instruction("add x16, x16, #1");                                    // advance runtime pointer or counter
    emitter.instruction("b __rt_hbr_mr_loop_aarch64");                          // continue at target label
    emitter.label("__rt_hbr_mr_store_aarch64");
    emitter.instruction("str x15, [x10]");                                      // store runtime value
    emitter.label("__rt_hbr_skip_mr_aarch64");
    // timeout: parse the seconds value as base-10 int. 0 disables.
    abi::emit_symbol_address(emitter, "x10", "_http_active_timeout_seconds");
    emitter.instruction("str xzr, [x10]");                                      // store runtime value
    emitter.instruction("ldr x11, [sp, #160]");                                 // timeout_len
    emitter.instruction("cbz x11, __rt_hbr_skip_to_aarch64");                   // branch when the checked value is zero or equal
    emitter.instruction("ldr x12, [sp, #152]");                                 // timeout_ptr
    emitter.instruction("mov x15, #0");                                         // move runtime value between registers
    emitter.instruction("mov x16, #0");                                         // move runtime value between registers
    emitter.label("__rt_hbr_to_loop_aarch64");
    emitter.instruction("cmp x16, x11");                                        // compare runtime values for the next branch
    emitter.instruction("b.ge __rt_hbr_to_store_aarch64");                      // branch when comparison is at least target
    emitter.instruction("ldrb w13, [x12, x16]");                                // load runtime value
    emitter.instruction("sub w13, w13, #48");                                   // reduce runtime pointer or counter
    emitter.instruction("cmp w13, #9");                                         // compare runtime values for the next branch
    emitter.instruction("b.hi __rt_hbr_to_store_aarch64");                      // stop timeout parsing on the first non-digit byte
    emitter.instruction("mov x17, #10");                                        // move runtime value between registers
    emitter.instruction("mul x15, x15, x17");                                   // compute scaled runtime value
    emitter.instruction("add x15, x15, x13");                                   // advance runtime pointer or counter
    emitter.instruction("add x16, x16, #1");                                    // advance runtime pointer or counter
    emitter.instruction("b __rt_hbr_to_loop_aarch64");                          // continue at target label
    emitter.label("__rt_hbr_to_store_aarch64");
    emitter.instruction("str x15, [x10]");                                      // store runtime value
    emitter.label("__rt_hbr_skip_to_aarch64");
    // proxy: capture ptr/len pair into globals so __rt_http_open can override
    // the connect target with the proxy address.
    abi::emit_symbol_address(emitter, "x10", "_http_active_proxy_ptr");
    emitter.instruction("ldr x12, [sp, #184]");                                 // proxy_ptr
    emitter.instruction("str x12, [x10]");                                      // store runtime value
    abi::emit_symbol_address(emitter, "x10", "_http_active_proxy_len");
    emitter.instruction("ldr x13, [sp, #192]");                                 // proxy_len
    emitter.instruction("str x13, [x10]");                                      // store runtime value

    // -- write offset = 0 --
    emitter.instruction("str xzr, [sp, #48]");                                  // store runtime value

    // -- copy method bytes into _http_req_scratch[0..method_len] --
    abi::emit_symbol_address(emitter, "x9", "_http_req_scratch");
    emitter.instruction("ldr x10, [sp, #32]");                                  // method_ptr
    emitter.instruction("ldr x11, [sp, #40]");                                  // method_len
    emitter.instruction("bl __rt_http_build_copy_aarch64");                     // x9 += x11; uses x10/x11; preserves them
    emitter.instruction("str x9, [sp, #48]");                                   // running write ptr = end of method

    // -- write ' ' separator --
    emitter.instruction("ldr x9, [sp, #48]");                                   // load runtime value
    emitter.instruction("mov w10, #32");                                        // ASCII space
    emitter.instruction("strb w10, [x9]");                                      // store runtime value
    emitter.instruction("add x9, x9, #1");                                      // advance runtime pointer or counter
    emitter.instruction("str x9, [sp, #48]");                                   // store runtime value

    // -- if [http][request_fulluri] is truthy OR [http][proxy] is set,
    //    emit "http://" + host BEFORE the path so the request line carries
    //    an absolute URI (RFC 7230 request-target = absolute-form). PHP
    //    auto-enables full-URI form whenever a proxy is configured because
    //    proxies require the absolute form to forward the request. --
    emitter.instruction("ldr x13, [sp, #144]");                                 // request_fulluri_len
    emitter.instruction("cbnz x13, __rt_hbr_check_fulluri_byte");               // fulluri set → check truthiness
    emitter.instruction("ldr x13, [sp, #192]");                                 // proxy_len
    emitter.instruction("cbz x13, __rt_hbr_no_fulluri_aarch64");                // neither set → skip
    emitter.instruction("b __rt_hbr_emit_fulluri");                             // proxy set (truthy by presence) → emit
    emitter.label("__rt_hbr_check_fulluri_byte");
    emitter.instruction("ldr x14, [sp, #136]");                                 // request_fulluri_ptr
    emitter.instruction("ldrb w15, [x14]");                                     // load runtime value
    emitter.instruction("cmp w15, #48");                                        // '0' (falsy)
    emitter.instruction("b.eq __rt_hbr_no_fulluri_aarch64");                    // branch when the checked value is zero or equal
    emitter.label("__rt_hbr_emit_fulluri");
    abi::emit_symbol_address(emitter, "x10", "_http_scheme_prefix");
    emitter.instruction("mov x11, #7");                                         // strlen("http://")
    emitter.instruction("bl __rt_http_build_copy_aarch64");                     // call runtime helper
    emitter.instruction("ldr x10, [sp, #0]");                                   // host_ptr
    emitter.instruction("ldr x11, [sp, #8]");                                   // host_len
    emitter.instruction("bl __rt_http_build_copy_aarch64");                     // call runtime helper
    emitter.instruction("str x9, [sp, #48]");                                   // store runtime value
    emitter.label("__rt_hbr_no_fulluri_aarch64");

    // -- copy path --
    emitter.instruction("ldr x10, [sp, #16]");                                  // load runtime value
    emitter.instruction("ldr x11, [sp, #24]");                                  // load runtime value
    emitter.instruction("bl __rt_http_build_copy_aarch64");                     // call runtime helper
    emitter.instruction("str x9, [sp, #48]");                                   // store runtime value

    // -- copy " HTTP/1.x\r\nHost: " (17 bytes) — select 1.0 vs 1.1 based on
    //    [http][protocol_version]. Default is 1.0; if the option's value is
    //    exactly the 3-byte string "1.1" we swap in the 1.1 prefix instead.
    abi::emit_symbol_address(emitter, "x10", "_http_version_host");             // default = HTTP/1.0
    emitter.instruction("ldr x13, [sp, #128]");                                 // proto_version_len
    emitter.instruction("cmp x13, #3");                                         // compare runtime values for the next branch
    emitter.instruction("b.ne __rt_hbr_proto_default_aarch64");                 // branch when the checked value is nonzero or different
    emitter.instruction("ldr x14, [sp, #120]");                                 // proto_version_ptr
    emitter.instruction("ldrb w15, [x14]");                                     // load runtime value
    emitter.instruction("cmp w15, #49");                                        // '1'
    emitter.instruction("b.ne __rt_hbr_proto_default_aarch64");                 // branch when the checked value is nonzero or different
    emitter.instruction("ldrb w15, [x14, #1]");                                 // load runtime value
    emitter.instruction("cmp w15, #46");                                        // '.'
    emitter.instruction("b.ne __rt_hbr_proto_default_aarch64");                 // branch when the checked value is nonzero or different
    emitter.instruction("ldrb w15, [x14, #2]");                                 // load runtime value
    emitter.instruction("cmp w15, #49");                                        // '1'
    emitter.instruction("b.ne __rt_hbr_proto_default_aarch64");                 // branch when the checked value is nonzero or different
    abi::emit_symbol_address(emitter, "x10", "_http_version_host_11");
    emitter.label("__rt_hbr_proto_default_aarch64");
    emitter.instruction("mov x11, #17");                                        // move runtime value between registers
    emitter.instruction("bl __rt_http_build_copy_aarch64");                     // call runtime helper
    emitter.instruction("str x9, [sp, #48]");                                   // store runtime value

    // -- copy host --
    emitter.instruction("ldr x10, [sp, #0]");                                   // load runtime value
    emitter.instruction("ldr x11, [sp, #8]");                                   // load runtime value
    emitter.instruction("bl __rt_http_build_copy_aarch64");                     // call runtime helper
    emitter.instruction("str x9, [sp, #48]");                                   // store runtime value

    // -- copy "\r\n" after the Host: value --
    abi::emit_symbol_address(emitter, "x10", "_http_crlf");
    emitter.instruction("mov x11, #2");                                         // move runtime value between registers
    emitter.instruction("bl __rt_http_build_copy_aarch64");                     // call runtime helper
    emitter.instruction("str x9, [sp, #48]");                                   // store runtime value

    // -- if [http][user_agent] was set, write "User-Agent: <val>\r\n" --
    emitter.instruction("ldr x13, [sp, #112]");                                 // user_agent_len
    emitter.instruction("cbz x13, __rt_hbr_no_ua_aarch64");                     // branch when the checked value is zero or equal
    abi::emit_symbol_address(emitter, "x10", "_http_user_agent_prefix");
    emitter.instruction("mov x11, #12");                                        // strlen("User-Agent: ")
    emitter.instruction("bl __rt_http_build_copy_aarch64");                     // call runtime helper
    emitter.instruction("ldr x10, [sp, #104]");                                 // user_agent_ptr
    emitter.instruction("ldr x11, [sp, #112]");                                 // user_agent_len
    emitter.instruction("bl __rt_http_build_copy_aarch64");                     // call runtime helper
    abi::emit_symbol_address(emitter, "x10", "_http_crlf");
    emitter.instruction("mov x11, #2");                                         // move runtime value between registers
    emitter.instruction("bl __rt_http_build_copy_aarch64");                     // call runtime helper
    emitter.instruction("str x9, [sp, #48]");                                   // store runtime value
    emitter.label("__rt_hbr_no_ua_aarch64");

    // -- if [http][header] was found, copy header bytes + "\r\n" --
    emitter.instruction("ldr x12, [sp, #72]");                                  // header_found flag
    emitter.instruction("cbz x12, __rt_hbr_no_header_aarch64");                 // branch when the checked value is zero or equal
    emitter.instruction("ldr x10, [sp, #56]");                                  // header_ptr
    emitter.instruction("ldr x11, [sp, #64]");                                  // header_len
    emitter.instruction("bl __rt_http_build_copy_aarch64");                     // call runtime helper
    emitter.instruction("str x9, [sp, #48]");                                   // store runtime value
    abi::emit_symbol_address(emitter, "x10", "_http_crlf");
    emitter.instruction("mov x11, #2");                                         // move runtime value between registers
    emitter.instruction("bl __rt_http_build_copy_aarch64");                     // call runtime helper
    emitter.instruction("str x9, [sp, #48]");                                   // store runtime value
    emitter.label("__rt_hbr_no_header_aarch64");

    // -- if [http][content] was found, write "Content-Length: <N>\r\n" --
    emitter.instruction("ldr x12, [sp, #96]");                                  // content_found flag
    emitter.instruction("cbz x12, __rt_hbr_no_clen_aarch64");                   // branch when the checked value is zero or equal
    emitter.instruction("ldr x9, [sp, #48]");                                   // running write ptr
    abi::emit_symbol_address(emitter, "x10", "_http_content_length_prefix");
    emitter.instruction("mov x11, #16");                                        // strlen("Content-Length: ")
    emitter.instruction("bl __rt_http_build_copy_aarch64");                     // x9 += 16
    emitter.instruction("str x9, [sp, #48]");                                   // save write ptr across __rt_itoa
    emitter.instruction("ldr x0, [sp, #88]");                                   // content_len
    emitter.instruction("bl __rt_itoa");                                        // x1=digit_ptr, x2=digit_count
    emitter.instruction("mov x10, x1");                                         // digit_ptr
    emitter.instruction("mov x11, x2");                                         // digit_count
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload write ptr
    emitter.instruction("bl __rt_http_build_copy_aarch64");                     // copy digits
    abi::emit_symbol_address(emitter, "x10", "_http_crlf");
    emitter.instruction("mov x11, #2");                                         // move runtime value between registers
    emitter.instruction("bl __rt_http_build_copy_aarch64");                     // copy "\r\n"
    emitter.instruction("str x9, [sp, #48]");                                   // commit updated write ptr
    emitter.label("__rt_hbr_no_clen_aarch64");

    // -- copy "Connection: close\r\n\r\n" (21 bytes) --
    abi::emit_symbol_address(emitter, "x10", "_http_trailer");
    emitter.instruction("mov x11, #21");                                        // move runtime value between registers
    emitter.instruction("bl __rt_http_build_copy_aarch64");                     // call runtime helper
    emitter.instruction("str x9, [sp, #48]");                                   // store runtime value

    // -- if content was found, append the body bytes after the blank line --
    emitter.instruction("ldr x12, [sp, #96]");                                  // load runtime value
    emitter.instruction("cbz x12, __rt_hbr_no_body_aarch64");                   // branch when the checked value is zero or equal
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload running write pointer (paranoia — x9 might be clobbered between trailer write and here)
    emitter.instruction("ldr x10, [sp, #80]");                                  // content_ptr
    emitter.instruction("ldr x11, [sp, #88]");                                  // content_len
    emitter.instruction("bl __rt_http_build_copy_aarch64");                     // call runtime helper
    emitter.instruction("str x9, [sp, #48]");                                   // store runtime value
    emitter.label("__rt_hbr_no_body_aarch64");

    // -- compute total length: write_ptr - scratch_base --
    abi::emit_symbol_address(emitter, "x10", "_http_req_scratch");
    emitter.instruction("ldr x11, [sp, #48]");                                  // load runtime value
    emitter.instruction("sub x0, x11, x10");                                    // return length

    emitter.instruction("ldp x29, x30, [sp, #240]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #256");                                    // release runtime stack frame
    emitter.instruction("ret");                                                 // return to caller

    // -- inline byte-copy helper:
    //    Input: x9 = dest, x10 = src, x11 = len. Output: x9 = dest + len.
    //    Trashes x12, x13. Preserves x10, x11. --
    emitter.blank();
    emitter.label_global("__rt_http_build_copy_aarch64");
    emitter.instruction("mov x12, xzr");                                        // i = 0
    emitter.label("__rt_http_build_copy_loop_aarch64");
    emitter.instruction("cmp x12, x11");                                        // compare runtime values for the next branch
    emitter.instruction("b.ge __rt_http_build_copy_done_aarch64");              // branch when comparison is at least target
    emitter.instruction("ldrb w13, [x10, x12]");                                // src[i]
    emitter.instruction("strb w13, [x9, x12]");                                 // dest[i] = src[i]
    emitter.instruction("add x12, x12, #1");                                    // advance runtime pointer or counter
    emitter.instruction("b __rt_http_build_copy_loop_aarch64");                 // continue at target label
    emitter.label("__rt_http_build_copy_done_aarch64");
    emitter.instruction("add x9, x9, x11");                                     // advance dest past the copied range
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 stream runtime helper for http build request.
fn emit_http_build_request_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: http_build_request ---");
    emitter.label_global("__rt_http_build_request");

    // rbp-relative frame (240 bytes — accommodates all Phase B HTTP options):
    //   [rbp -   8..104] existing slots (host, path, method, write, header, content)
    //   [rbp - 112..136] user_agent + proto_version (16 bytes each, ptr+len)
    //   [rbp - 144..152] request_fulluri (ptr+len)
    //   [rbp - 160..168] timeout (ptr+len)
    //   [rbp - 176..184] ignore_errors (ptr+len)
    //   [rbp - 192..200] proxy (ptr+len)
    //   [rbp - 208..216] follow_location (ptr+len)
    //   [rbp - 224..232] max_redirects (ptr+len)
    emitter.instruction("push rbp");                                            // save caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish runtime frame pointer
    emitter.instruction("sub rsp, 240");                                        // matches the epilogue's add rsp, 240 — 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // host_ptr
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // host_len
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // path_ptr
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // path_len

    // Expose host to __rt_http_open's follow_location loop.
    abi::emit_store_reg_to_symbol(emitter, "rdi", "_http_active_host_ptr", 0);  // store runtime value
    abi::emit_store_reg_to_symbol(emitter, "rsi", "_http_active_host_len", 0);  // store runtime value

    // Preload method = default ("GET", 3)
    abi::emit_symbol_address(emitter, "r9", "_http_default_method");            // load runtime data address
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // store runtime value
    emitter.instruction("mov r9, 3");                                           // prepare SysV call argument
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // store runtime value

    // Context lookup: __rt_get_string_context_option(http, http_len, method, method_len, &method_ptr_slot, &method_len_slot)
    abi::emit_symbol_address(emitter, "rdi", "_http_key_str");                  // load runtime data address
    emitter.instruction("mov rsi, 4");                                          // strlen("http")
    abi::emit_symbol_address(emitter, "rdx", "_http_method_key_str");           // load runtime data address
    emitter.instruction("mov rcx, 6");                                          // strlen("method")
    emitter.instruction("lea r8, [rbp - 40]");                                  // out_ptr_addr
    emitter.instruction("lea r9, [rbp - 48]");                                  // out_len_addr
    emitter.instruction("call __rt_get_string_context_option");                 // call runtime helper

    // Context lookup: [http][header]
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // header_ptr = 0
    emitter.instruction("mov QWORD PTR [rbp - 72], 0");                         // header_len = 0
    abi::emit_symbol_address(emitter, "rdi", "_http_key_str");                  // load runtime data address
    emitter.instruction("mov rsi, 4");                                          // prepare SysV call argument
    abi::emit_symbol_address(emitter, "rdx", "_http_header_key_str");           // load runtime data address
    emitter.instruction("mov rcx, 6");                                          // strlen("header")
    emitter.instruction("lea r8, [rbp - 64]");                                  // header_ptr slot
    emitter.instruction("lea r9, [rbp - 72]");                                  // header_len slot
    emitter.instruction("call __rt_get_string_context_option");                 // call runtime helper
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // header_found flag

    // Context lookup: [http][content]
    emitter.instruction("mov QWORD PTR [rbp - 88], 0");                         // store runtime value
    emitter.instruction("mov QWORD PTR [rbp - 96], 0");                         // store runtime value
    abi::emit_symbol_address(emitter, "rdi", "_http_key_str");                  // load runtime data address
    emitter.instruction("mov rsi, 4");                                          // prepare SysV call argument
    abi::emit_symbol_address(emitter, "rdx", "_http_content_key_str");          // load runtime data address
    emitter.instruction("mov rcx, 7");                                          // strlen("content")
    emitter.instruction("lea r8, [rbp - 88]");                                  // load runtime data address
    emitter.instruction("lea r9, [rbp - 96]");                                  // load runtime data address
    emitter.instruction("call __rt_get_string_context_option");                 // call runtime helper
    emitter.instruction("mov QWORD PTR [rbp - 104], rax");                      // content_found flag

    // Context lookup: [http][user_agent]
    emitter.instruction("mov QWORD PTR [rbp - 112], 0");                        // store runtime value
    emitter.instruction("mov QWORD PTR [rbp - 120], 0");                        // store runtime value
    abi::emit_symbol_address(emitter, "rdi", "_http_key_str");                  // load runtime data address
    emitter.instruction("mov rsi, 4");                                          // prepare SysV call argument
    abi::emit_symbol_address(emitter, "rdx", "_http_user_agent_key_str");       // load runtime data address
    emitter.instruction("mov rcx, 10");                                         // strlen("user_agent")
    emitter.instruction("lea r8, [rbp - 112]");                                 // load runtime data address
    emitter.instruction("lea r9, [rbp - 120]");                                 // load runtime data address
    emitter.instruction("call __rt_get_string_context_option");                 // call runtime helper

    // Context lookup: [http][protocol_version]
    emitter.instruction("mov QWORD PTR [rbp - 128], 0");                        // store runtime value
    emitter.instruction("mov QWORD PTR [rbp - 136], 0");                        // store runtime value
    abi::emit_symbol_address(emitter, "rdi", "_http_key_str");                  // load runtime data address
    emitter.instruction("mov rsi, 4");                                          // prepare SysV call argument
    abi::emit_symbol_address(emitter, "rdx", "_http_protocol_version_key_str"); // load runtime data address
    emitter.instruction("mov rcx, 16");                                         // strlen("protocol_version")
    emitter.instruction("lea r8, [rbp - 128]");                                 // load runtime data address
    emitter.instruction("lea r9, [rbp - 136]");                                 // load runtime data address
    emitter.instruction("call __rt_get_string_context_option");                 // call runtime helper

    // Batch lookups for the remaining six Phase B HTTP options (request_fulluri,
    // timeout, ignore_errors, proxy, follow_location, max_redirects). Only
    // request_fulluri changes emitted bytes here; the others are stored for
    // future enforcement passes (see the ARM64 comment block for details).
    let lookup_str_x = |emitter: &mut Emitter, sym: &str, len: i64, ptr_off: i64| {
        emitter.instruction(&format!("mov QWORD PTR [rbp - {}], 0", ptr_off));  // store runtime value
        emitter.instruction(&format!("mov QWORD PTR [rbp - {}], 0", ptr_off + 8)); // store runtime value
        abi::emit_symbol_address(emitter, "rdi", "_http_key_str");              // load runtime data address
        emitter.instruction("mov rsi, 4");                                      // prepare SysV call argument
        abi::emit_symbol_address(emitter, "rdx", sym);                          // load runtime data address
        emitter.instruction(&format!("mov rcx, {}", len));                      // prepare SysV call argument
        emitter.instruction(&format!("lea r8, [rbp - {}]", ptr_off));           // load runtime data address
        emitter.instruction(&format!("lea r9, [rbp - {}]", ptr_off + 8));       // load runtime data address
        emitter.instruction("call __rt_get_string_context_option");             // call runtime helper
    };
    lookup_str_x(emitter, "_http_request_fulluri_key_str", 15, 144);            // strlen("request_fulluri")
    lookup_str_x(emitter, "_http_timeout_key_str", 7, 160);                     // strlen("timeout")
    lookup_str_x(emitter, "_http_ignore_errors_key_str", 13, 176);              // strlen("ignore_errors")
    lookup_str_x(emitter, "_http_proxy_key_str", 5, 192);                       // strlen("proxy")
    lookup_str_x(emitter, "_http_follow_location_key_str", 15, 208);            // strlen("follow_location")
    lookup_str_x(emitter, "_http_max_redirects_key_str", 13, 224);              // strlen("max_redirects")

    // -- propagate enforcement-relevant options to globals for __rt_http_open.
    // ignore_errors: truthy when ptr non-zero AND first byte != '0'.
    abi::emit_symbol_address(emitter, "r10", "_http_active_ignore_errors");     // load runtime data address
    emitter.instruction("mov QWORD PTR [r10], 0");                              // store runtime value
    emitter.instruction("mov r11, QWORD PTR [rbp - 184]");                      // ignore_errors_len
    emitter.instruction("test r11, r11");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_hbr_skip_ie_x");                               // branch when the checked value is zero or equal
    emitter.instruction("mov r12, QWORD PTR [rbp - 176]");                      // ignore_errors_ptr
    emitter.instruction("movzx eax, BYTE PTR [r12]");                           // load runtime value
    emitter.instruction("cmp al, 48");                                          // '0' falsy
    emitter.instruction("je __rt_hbr_skip_ie_x");                               // branch when the checked value is zero or equal
    emitter.instruction("mov QWORD PTR [r10], 1");                              // store runtime value
    emitter.label("__rt_hbr_skip_ie_x");
    // max_redirects: only honored when follow_location is truthy. Parse the
    // string value (digits), or default to PHP's 20.
    abi::emit_symbol_address(emitter, "r10", "_http_active_max_redirects");     // load runtime data address
    emitter.instruction("mov QWORD PTR [r10], 0");                              // store runtime value
    emitter.instruction("mov r11, QWORD PTR [rbp - 216]");                      // follow_location_len
    emitter.instruction("test r11, r11");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_hbr_skip_mr_x");                               // branch when the checked value is zero or equal
    emitter.instruction("mov r12, QWORD PTR [rbp - 208]");                      // follow_location_ptr
    emitter.instruction("movzx eax, BYTE PTR [r12]");                           // load runtime value
    emitter.instruction("cmp al, 48");                                          // '0' falsy
    emitter.instruction("je __rt_hbr_skip_mr_x");                               // branch when the checked value is zero or equal
    // follow_location truthy. Parse max_redirects, default 20.
    emitter.instruction("mov r15, 20");                                         // move runtime value between registers
    emitter.instruction("mov r11, QWORD PTR [rbp - 232]");                      // max_redirects_len
    emitter.instruction("test r11, r11");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_hbr_mr_store_x");                              // branch when the checked value is zero or equal
    emitter.instruction("mov r12, QWORD PTR [rbp - 224]");                      // max_redirects_ptr
    emitter.instruction("xor r15, r15");                                        // accumulator
    emitter.instruction("xor rcx, rcx");                                        // loop index
    emitter.label("__rt_hbr_mr_loop_x");
    emitter.instruction("cmp rcx, r11");                                        // compare runtime values for the next branch
    emitter.instruction("jge __rt_hbr_mr_store_x");                             // branch when comparison is at least target
    emitter.instruction("movzx eax, BYTE PTR [r12 + rcx]");                     // load runtime value
    emitter.instruction("sub al, 48");                                          // '0'..'9' → 0..9
    emitter.instruction("cmp al, 9");                                           // compare runtime values for the next branch
    emitter.instruction("ja __rt_hbr_mr_store_x");                              // non-digit terminates
    emitter.instruction("imul r15, r15, 10");                                   // compute scaled runtime value
    emitter.instruction("movzx rax, al");                                       // load runtime value
    emitter.instruction("add r15, rax");                                        // advance runtime pointer or counter
    emitter.instruction("inc rcx");                                             // advance runtime pointer or counter
    emitter.instruction("jmp __rt_hbr_mr_loop_x");                              // continue at target label
    emitter.label("__rt_hbr_mr_store_x");
    emitter.instruction("mov QWORD PTR [r10], r15");                            // store runtime value
    emitter.label("__rt_hbr_skip_mr_x");
    // timeout: parse seconds as base-10 int.
    abi::emit_symbol_address(emitter, "r10", "_http_active_timeout_seconds");   // load runtime data address
    emitter.instruction("mov QWORD PTR [r10], 0");                              // store runtime value
    emitter.instruction("mov r11, QWORD PTR [rbp - 168]");                      // timeout_len
    emitter.instruction("test r11, r11");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_hbr_skip_to_x");                               // branch when the checked value is zero or equal
    emitter.instruction("mov r12, QWORD PTR [rbp - 160]");                      // timeout_ptr
    emitter.instruction("xor r15, r15");                                        // clear register value
    emitter.instruction("xor rcx, rcx");                                        // clear register value
    emitter.label("__rt_hbr_to_loop_x");
    emitter.instruction("cmp rcx, r11");                                        // compare runtime values for the next branch
    emitter.instruction("jge __rt_hbr_to_store_x");                             // branch when comparison is at least target
    emitter.instruction("movzx eax, BYTE PTR [r12 + rcx]");                     // load runtime value
    emitter.instruction("sub al, 48");                                          // reduce runtime pointer or counter
    emitter.instruction("cmp al, 9");                                           // compare runtime values for the next branch
    emitter.instruction("ja __rt_hbr_to_store_x");                              // branch when comparison is above target
    emitter.instruction("imul r15, r15, 10");                                   // compute scaled runtime value
    emitter.instruction("movzx rax, al");                                       // load runtime value
    emitter.instruction("add r15, rax");                                        // advance runtime pointer or counter
    emitter.instruction("inc rcx");                                             // advance runtime pointer or counter
    emitter.instruction("jmp __rt_hbr_to_loop_x");                              // continue at target label
    emitter.label("__rt_hbr_to_store_x");
    emitter.instruction("mov QWORD PTR [r10], r15");                            // store runtime value
    emitter.label("__rt_hbr_skip_to_x");
    // proxy: capture ptr/len globals.
    emitter.instruction("mov r11, QWORD PTR [rbp - 192]");                      // proxy_ptr
    abi::emit_store_reg_to_symbol(emitter, "r11", "_http_active_proxy_ptr", 0); // store runtime value
    emitter.instruction("mov r11, QWORD PTR [rbp - 200]");                      // proxy_len
    abi::emit_store_reg_to_symbol(emitter, "r11", "_http_active_proxy_len", 0); // store runtime value

    // running write ptr = _http_req_scratch
    abi::emit_symbol_address(emitter, "r9", "_http_req_scratch");               // load runtime data address
    emitter.instruction("mov QWORD PTR [rbp - 56], r9");                        // store runtime value

    // copy method
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // prepare SysV call argument
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // prepare SysV call argument
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // prepare SysV call argument
    emitter.instruction("call __rt_http_build_copy_x86");                       // call runtime helper
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // store runtime value

    // write ' ' separator
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // prepare SysV call argument
    emitter.instruction("mov BYTE PTR [rdi], 32");                              // store runtime value
    emitter.instruction("add rdi, 1");                                          // advance runtime pointer or counter
    emitter.instruction("mov QWORD PTR [rbp - 56], rdi");                       // store runtime value

    // if [http][request_fulluri] is truthy OR [http][proxy] is set, emit
    // "http://" + host before the path (RFC 7230 absolute-form
    // request-target). PHP auto-enables this when proxy is configured.
    emitter.instruction("mov r10, QWORD PTR [rbp - 152]");                      // request_fulluri_len
    emitter.instruction("test r10, r10");                                       // check whether the runtime value is zero
    emitter.instruction("jnz __rt_hbr_check_fulluri_byte_x");                   // fulluri set → check truthy
    emitter.instruction("mov r10, QWORD PTR [rbp - 200]");                      // proxy_len
    emitter.instruction("test r10, r10");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_hbr_no_fulluri_x");                            // neither set → skip
    emitter.instruction("jmp __rt_hbr_emit_fulluri_x");                         // proxy → emit
    emitter.label("__rt_hbr_check_fulluri_byte_x");
    emitter.instruction("mov r11, QWORD PTR [rbp - 144]");                      // request_fulluri_ptr
    emitter.instruction("movzx eax, BYTE PTR [r11]");                           // load runtime value
    emitter.instruction("cmp al, 48");                                          // '0' (falsy)
    emitter.instruction("je __rt_hbr_no_fulluri_x");                            // branch when the checked value is zero or equal
    emitter.label("__rt_hbr_emit_fulluri_x");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // prepare SysV call argument
    abi::emit_symbol_address(emitter, "rsi", "_http_scheme_prefix");            // load runtime data address
    emitter.instruction("mov rdx, 7");                                          // strlen("http://")
    emitter.instruction("call __rt_http_build_copy_x86");                       // call runtime helper
    emitter.instruction("mov rdi, rax");                                        // prepare SysV call argument
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // host_ptr
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // host_len
    emitter.instruction("call __rt_http_build_copy_x86");                       // call runtime helper
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // store runtime value
    emitter.label("__rt_hbr_no_fulluri_x");

    // copy path
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // prepare SysV call argument
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // prepare SysV call argument
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // prepare SysV call argument
    emitter.instruction("call __rt_http_build_copy_x86");                       // call runtime helper
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // store runtime value

    // copy " HTTP/1.x\r\nHost: " — pick 1.0 vs 1.1 based on
    // [http][protocol_version] (default = 1.0, "1.1" → 1.1).
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // prepare SysV call argument
    abi::emit_symbol_address(emitter, "rsi", "_http_version_host");             // default = HTTP/1.0
    emitter.instruction("mov r10, QWORD PTR [rbp - 136]");                      // proto_version_len
    emitter.instruction("cmp r10, 3");                                          // compare runtime values for the next branch
    emitter.instruction("jne __rt_hbr_proto_default_x");                        // branch when the checked value is nonzero or different
    emitter.instruction("mov r11, QWORD PTR [rbp - 128]");                      // proto_version_ptr
    emitter.instruction("movzx eax, BYTE PTR [r11]");                           // load runtime value
    emitter.instruction("cmp al, 49");                                          // '1'
    emitter.instruction("jne __rt_hbr_proto_default_x");                        // branch when the checked value is nonzero or different
    emitter.instruction("movzx eax, BYTE PTR [r11 + 1]");                       // load runtime value
    emitter.instruction("cmp al, 46");                                          // '.'
    emitter.instruction("jne __rt_hbr_proto_default_x");                        // branch when the checked value is nonzero or different
    emitter.instruction("movzx eax, BYTE PTR [r11 + 2]");                       // load runtime value
    emitter.instruction("cmp al, 49");                                          // '1'
    emitter.instruction("jne __rt_hbr_proto_default_x");                        // branch when the checked value is nonzero or different
    abi::emit_symbol_address(emitter, "rsi", "_http_version_host_11");          // load runtime data address
    emitter.label("__rt_hbr_proto_default_x");
    emitter.instruction("mov rdx, 17");                                         // prepare SysV call argument
    emitter.instruction("call __rt_http_build_copy_x86");                       // call runtime helper
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // store runtime value

    // copy host
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // prepare SysV call argument
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // prepare SysV call argument
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // prepare SysV call argument
    emitter.instruction("call __rt_http_build_copy_x86");                       // call runtime helper
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // store runtime value

    // copy "\r\n" after the Host: value
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // prepare SysV call argument
    abi::emit_symbol_address(emitter, "rsi", "_http_crlf");                     // load runtime data address
    emitter.instruction("mov rdx, 2");                                          // prepare SysV call argument
    emitter.instruction("call __rt_http_build_copy_x86");                       // call runtime helper
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // store runtime value

    // if [http][user_agent] is set, emit "User-Agent: <val>\r\n"
    emitter.instruction("mov r10, QWORD PTR [rbp - 120]");                      // user_agent_len
    emitter.instruction("test r10, r10");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_hbr_no_ua_x");                                 // branch when the checked value is zero or equal
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // prepare SysV call argument
    abi::emit_symbol_address(emitter, "rsi", "_http_user_agent_prefix");        // load runtime data address
    emitter.instruction("mov rdx, 12");                                         // strlen("User-Agent: ")
    emitter.instruction("call __rt_http_build_copy_x86");                       // call runtime helper
    emitter.instruction("mov rdi, rax");                                        // prepare SysV call argument
    emitter.instruction("mov rsi, QWORD PTR [rbp - 112]");                      // user_agent_ptr
    emitter.instruction("mov rdx, QWORD PTR [rbp - 120]");                      // user_agent_len
    emitter.instruction("call __rt_http_build_copy_x86");                       // call runtime helper
    emitter.instruction("mov rdi, rax");                                        // prepare SysV call argument
    abi::emit_symbol_address(emitter, "rsi", "_http_crlf");                     // load runtime data address
    emitter.instruction("mov rdx, 2");                                          // prepare SysV call argument
    emitter.instruction("call __rt_http_build_copy_x86");                       // call runtime helper
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // store runtime value
    emitter.label("__rt_hbr_no_ua_x");

    // if [http][header] was found, copy header bytes + "\r\n"
    emitter.instruction("mov r10, QWORD PTR [rbp - 80]");                       // header_found flag
    emitter.instruction("test r10, r10");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_hbr_no_header_x86");                           // branch when the checked value is zero or equal
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // prepare SysV call argument
    emitter.instruction("mov rsi, QWORD PTR [rbp - 64]");                       // prepare SysV call argument
    emitter.instruction("mov rdx, QWORD PTR [rbp - 72]");                       // prepare SysV call argument
    emitter.instruction("call __rt_http_build_copy_x86");                       // call runtime helper
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // store runtime value
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // prepare SysV call argument
    abi::emit_symbol_address(emitter, "rsi", "_http_crlf");                     // load runtime data address
    emitter.instruction("mov rdx, 2");                                          // prepare SysV call argument
    emitter.instruction("call __rt_http_build_copy_x86");                       // call runtime helper
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // store runtime value
    emitter.label("__rt_hbr_no_header_x86");

    // if [http][content] was found, write "Content-Length: <N>\r\n"
    emitter.instruction("mov r10, QWORD PTR [rbp - 104]");                      // content_found flag
    emitter.instruction("test r10, r10");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_hbr_no_clen_x86");                             // branch when the checked value is zero or equal
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // prepare SysV call argument
    abi::emit_symbol_address(emitter, "rsi", "_http_content_length_prefix");    // load runtime data address
    emitter.instruction("mov rdx, 16");                                         // strlen("Content-Length: ")
    emitter.instruction("call __rt_http_build_copy_x86");                       // call runtime helper
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // store runtime value
    emitter.instruction("mov rax, QWORD PTR [rbp - 96]");                       // content_len → itoa input (rax)
    emitter.instruction("call __rt_itoa");                                      // rax=ptr, rdx=len
    emitter.instruction("mov rsi, rax");                                        // itoa ptr → 2nd arg
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // dest
    emitter.instruction("call __rt_http_build_copy_x86");                       // call runtime helper
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // store runtime value
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // prepare SysV call argument
    abi::emit_symbol_address(emitter, "rsi", "_http_crlf");                     // load runtime data address
    emitter.instruction("mov rdx, 2");                                          // prepare SysV call argument
    emitter.instruction("call __rt_http_build_copy_x86");                       // call runtime helper
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // store runtime value
    emitter.label("__rt_hbr_no_clen_x86");

    // copy "Connection: close\r\n\r\n"
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // prepare SysV call argument
    abi::emit_symbol_address(emitter, "rsi", "_http_trailer");                  // load runtime data address
    emitter.instruction("mov rdx, 21");                                         // prepare SysV call argument
    emitter.instruction("call __rt_http_build_copy_x86");                       // call runtime helper
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // store runtime value

    // if content was found, append body bytes after the blank line
    emitter.instruction("mov r10, QWORD PTR [rbp - 104]");                      // move runtime value between registers
    emitter.instruction("test r10, r10");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_hbr_no_body_x86");                             // branch when the checked value is zero or equal
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // prepare SysV call argument
    emitter.instruction("mov rsi, QWORD PTR [rbp - 88]");                       // prepare SysV call argument
    emitter.instruction("mov rdx, QWORD PTR [rbp - 96]");                       // prepare SysV call argument
    emitter.instruction("call __rt_http_build_copy_x86");                       // call runtime helper
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // store runtime value
    emitter.label("__rt_hbr_no_body_x86");

    // total length = write_ptr - scratch_base
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // prepare runtime result value
    abi::emit_symbol_address(emitter, "r9", "_http_req_scratch");               // load runtime data address
    emitter.instruction("sub rax, r9");                                         // reduce runtime pointer or counter

    emitter.instruction("add rsp, 240");                                        // must match the prologue's sub rsp, 240
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return to caller

    // inline byte-copy helper:
    // Input: rdi = dest, rsi = src, rdx = len. Output: rax = dest + len.
    emitter.blank();
    emitter.label_global("__rt_http_build_copy_x86");
    emitter.instruction("xor rcx, rcx");                                        // i = 0
    emitter.label("__rt_http_build_copy_loop_x86");
    emitter.instruction("cmp rcx, rdx");                                        // compare runtime values for the next branch
    emitter.instruction("jge __rt_http_build_copy_done_x86");                   // branch when comparison is at least target
    emitter.instruction("mov r10b, BYTE PTR [rsi + rcx]");                      // move runtime value between registers
    emitter.instruction("mov BYTE PTR [rdi + rcx], r10b");                      // store runtime value
    emitter.instruction("add rcx, 1");                                          // advance runtime pointer or counter
    emitter.instruction("jmp __rt_http_build_copy_loop_x86");                   // continue at target label
    emitter.label("__rt_http_build_copy_done_x86");
    emitter.instruction("mov rax, rdi");                                        // prepare runtime result value
    emitter.instruction("add rax, rdx");                                        // advance runtime pointer or counter
    emitter.instruction("ret");                                                 // return to caller
}
