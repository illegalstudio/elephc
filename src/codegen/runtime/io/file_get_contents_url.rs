//! Purpose:
//! Emits the runtime dispatcher for non-literal `file_get_contents()` URLs.
//! It recognizes supported URL schemes at run time before falling back to the
//! existing phar/filesystem helper.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::io`.
//! - The `file_get_contents()` builtin for non-literal path expressions.
//!
//! Key details:
//! - Dynamic `http://` / `https://` URLs are parsed into host/path/request
//!   pieces, then routed through the same HTTP helpers used by literal URL
//!   lowering.
//! - Dynamic `ftp://` / `ftps://` URLs share one FTP body; the scheme check
//!   controls the prefix length and `_ftp_use_tls` flag before `__rt_ftp_open`.
//! - Successful URL reads are persisted with `__rt_str_persist` before
//!   returning so the boxed PHP string owns its payload.

use crate::codegen::{abi, emit::Emitter, platform::Arch};

/// Emits `__rt_file_get_contents_maybe_url`.
///
/// Inputs use elephc's string ABI (`x1`/`x2` on AArch64, `rax`/`rdx` on
/// x86_64). The helper returns the same pointer/length ABI, with a null pointer
/// indicating PHP `false`.
pub fn emit_file_get_contents_url(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_file_get_contents_url_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: file_get_contents_maybe_url ---");
    emitter.label_global("__rt_file_get_contents_maybe_url");

    // -- recognize http:// --
    emitter.instruction("cmp x2, #7");                                          // URL long enough for "http://"?
    emitter.instruction("b.lt __rt_fgc_url_check_https");                       // too short for http: try the next URL scheme
    emitter.instruction("ldrb w9, [x1, #0]");                                   // URL byte 0
    emitter.instruction("cmp w9, #0x68");                                       // 'h'
    emitter.instruction("b.ne __rt_fgc_url_check_https");                       // not http://
    emitter.instruction("ldrb w9, [x1, #1]");                                   // URL byte 1
    emitter.instruction("cmp w9, #0x74");                                       // 't'
    emitter.instruction("b.ne __rt_fgc_url_check_https");                       // not http://
    emitter.instruction("ldrb w9, [x1, #2]");                                   // URL byte 2
    emitter.instruction("cmp w9, #0x74");                                       // 't'
    emitter.instruction("b.ne __rt_fgc_url_check_https");                       // not http://
    emitter.instruction("ldrb w9, [x1, #3]");                                   // URL byte 3
    emitter.instruction("cmp w9, #0x70");                                       // 'p'
    emitter.instruction("b.ne __rt_fgc_url_check_https");                       // not http://
    emitter.instruction("ldrb w9, [x1, #4]");                                   // URL byte 4
    emitter.instruction("cmp w9, #0x3a");                                       // ':'
    emitter.instruction("b.ne __rt_fgc_url_check_https");                       // not http://
    emitter.instruction("ldrb w9, [x1, #5]");                                   // URL byte 5
    emitter.instruction("cmp w9, #0x2f");                                       // '/'
    emitter.instruction("b.ne __rt_fgc_url_check_https");                       // not http://
    emitter.instruction("ldrb w9, [x1, #6]");                                   // URL byte 6
    emitter.instruction("cmp w9, #0x2f");                                       // '/'
    emitter.instruction("b.ne __rt_fgc_url_check_https");                       // not http://

    // Frame: [0]=url ptr [8]=url len [16]=host ptr [24]=host len
    //        [32]=path ptr [40]=path len [48]=addr len [56]=fd
    //        [64]=string ptr [72]=string len [80]=x29/x30.
    emitter.instruction("sub sp, sp, #96");                                     // allocate the dynamic-URL helper frame
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish the helper frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save URL pointer
    emitter.instruction("str x2, [sp, #8]");                                    // save URL length

    // -- split authority/path; a missing path defaults to "/" --
    emitter.instruction("mov x11, #7");                                         // scan index starts after "http://"
    emitter.label("__rt_fgc_url_http_slash_scan");
    emitter.instruction("cmp x11, x2");                                         // reached the URL end while scanning for path?
    emitter.instruction("b.ge __rt_fgc_url_http_no_path");                      // no slash: synthesize "/"
    emitter.instruction("ldrb w12, [x1, x11]");                                 // current URL byte
    emitter.instruction("cmp w12, #0x2f");                                      // slash starts the path?
    emitter.instruction("b.eq __rt_fgc_url_http_have_path");                    // path starts at this slash
    emitter.instruction("add x11, x11, #1");                                    // advance the scan index
    emitter.instruction("b __rt_fgc_url_http_slash_scan");                      // keep scanning for path
    emitter.label("__rt_fgc_url_http_no_path");
    abi::emit_symbol_address(emitter, "x12", "_fgc_url_slash");
    emitter.instruction("str x12, [sp, #32]");                                  // path pointer = "/"
    emitter.instruction("mov x12, #1");                                         // path length = 1
    emitter.instruction("str x12, [sp, #40]");                                  // save synthesized path length
    emitter.instruction("ldr x11, [sp, #8]");                                   // authority end = URL length
    emitter.instruction("b __rt_fgc_url_http_after_path");                      // continue with authority parsing
    emitter.label("__rt_fgc_url_http_have_path");
    emitter.instruction("add x12, x1, x11");                                    // path pointer = URL + slash index
    emitter.instruction("str x12, [sp, #32]");                                  // save path pointer
    emitter.instruction("sub x12, x2, x11");                                    // path length = URL length - slash index
    emitter.instruction("str x12, [sp, #40]");                                  // save path length
    emitter.label("__rt_fgc_url_http_after_path");

    // -- drop userinfo by selecting bytes after the last '@' before the path --
    emitter.instruction("mov x13, #7");                                         // host start index
    emitter.instruction("mov x14, #7");                                         // userinfo scan index
    emitter.label("__rt_fgc_url_http_userinfo_scan");
    emitter.instruction("cmp x14, x11");                                        // reached authority end?
    emitter.instruction("b.ge __rt_fgc_url_http_userinfo_done");                // host start is final
    emitter.instruction("ldrb w12, [x1, x14]");                                 // authority byte
    emitter.instruction("cmp w12, #0x40");                                      // '@' userinfo separator?
    emitter.instruction("b.ne __rt_fgc_url_http_userinfo_next");                // not userinfo separator
    emitter.instruction("add x13, x14, #1");                                    // host starts after the separator
    emitter.label("__rt_fgc_url_http_userinfo_next");
    emitter.instruction("add x14, x14, #1");                                    // advance authority scan
    emitter.instruction("b __rt_fgc_url_http_userinfo_scan");                   // keep scanning authority
    emitter.label("__rt_fgc_url_http_userinfo_done");
    emitter.instruction("sub x14, x11, x13");                                   // host/header length
    emitter.instruction("cmp x14, #0");                                         // empty authority after userinfo?
    emitter.instruction("b.le __rt_fgc_url_http_fail");                         // empty host is an invalid URL
    emitter.instruction("add x12, x1, x13");                                    // host pointer inside original URL
    emitter.instruction("str x12, [sp, #16]");                                  // save host/header pointer
    emitter.instruction("str x14, [sp, #24]");                                  // save host/header length

    // -- build tcp://host[:port] in _fgc_url_addr, adding :80 when absent --
    abi::emit_symbol_address(emitter, "x9", "_fgc_url_addr");
    abi::emit_symbol_address(emitter, "x10", "_ftp_tcp_prefix");
    emitter.instruction("mov x15, #0");                                         // address write index
    emitter.label("__rt_fgc_url_http_tcp_prefix");
    emitter.instruction("cmp x15, #6");                                         // copied "tcp://" yet?
    emitter.instruction("b.ge __rt_fgc_url_http_tcp_prefix_done");              // prefix copied
    emitter.instruction("ldrb w12, [x10, x15]");                                // prefix byte
    emitter.instruction("strb w12, [x9, x15]");                                 // store prefix byte
    emitter.instruction("add x15, x15, #1");                                    // advance address write index
    emitter.instruction("b __rt_fgc_url_http_tcp_prefix");                      // copy the next prefix byte
    emitter.label("__rt_fgc_url_http_tcp_prefix_done");
    emitter.instruction("mov x16, #0");                                         // host copy index
    emitter.instruction("mov x17, #0");                                         // has explicit port? 0/1
    emitter.label("__rt_fgc_url_http_host_copy");
    emitter.instruction("cmp x16, x14");                                        // copied all host/header bytes?
    emitter.instruction("b.ge __rt_fgc_url_http_host_done");                    // host copy complete
    emitter.instruction("ldrb w12, [x1, x13]");                                 // next host byte
    emitter.instruction("cmp w12, #0x3a");                                      // ':' means an explicit port is present
    emitter.instruction("b.ne __rt_fgc_url_http_no_port");                      // leave the explicit-port flag unchanged otherwise
    emitter.instruction("mov x17, #1");                                         // remember that the authority already has a port
    emitter.label("__rt_fgc_url_http_no_port");
    emitter.instruction("strb w12, [x9, x15]");                                 // append host byte to the TCP address
    emitter.instruction("add x13, x13, #1");                                    // advance host source index
    emitter.instruction("add x16, x16, #1");                                    // advance host copy index
    emitter.instruction("add x15, x15, #1");                                    // advance address write index
    emitter.instruction("b __rt_fgc_url_http_host_copy");                       // copy the next host byte
    emitter.label("__rt_fgc_url_http_host_done");
    emitter.instruction("cbnz x17, __rt_fgc_url_http_addr_done");               // explicit port: do not append default :80
    emitter.instruction("mov w12, #0x3a");                                      // ':'
    emitter.instruction("strb w12, [x9, x15]");                                 // append ':'
    emitter.instruction("add x15, x15, #1");                                    // advance address write index
    emitter.instruction("mov w12, #0x38");                                      // '8'
    emitter.instruction("strb w12, [x9, x15]");                                 // append '8'
    emitter.instruction("add x15, x15, #1");                                    // advance address write index
    emitter.instruction("mov w12, #0x30");                                      // '0'
    emitter.instruction("strb w12, [x9, x15]");                                 // append '0'
    emitter.instruction("add x15, x15, #1");                                    // advance address write index
    emitter.label("__rt_fgc_url_http_addr_done");
    emitter.instruction("str x15, [sp, #48]");                                  // save TCP address length

    // -- build the HTTP request then open the wrapper fd --
    emitter.instruction("ldr x0, [sp, #16]");                                   // host/header pointer
    emitter.instruction("ldr x1, [sp, #24]");                                   // host/header length
    emitter.instruction("ldr x2, [sp, #32]");                                   // path pointer
    emitter.instruction("ldr x3, [sp, #40]");                                   // path length
    emitter.instruction("bl __rt_http_build_request");                          // build request into _http_req_scratch, returning length
    emitter.instruction("mov x3, x0");                                          // request length for __rt_http_open
    abi::emit_symbol_address(emitter, "x0", "_fgc_url_addr");
    emitter.instruction("ldr x1, [sp, #48]");                                   // TCP address length
    abi::emit_symbol_address(emitter, "x2", "_http_req_scratch");
    emitter.instruction("bl __rt_http_open");                                   // open HTTP response-body fd
    emitter.instruction("cmp x0, #0");                                          // did the URL open fail?
    emitter.instruction("b.lt __rt_fgc_url_http_fail");                         // failed open returns PHP false
    emitter.instruction("str x0, [sp, #56]");                                   // save response-body fd
    emitter.instruction("bl __rt_stream_get_contents");                         // slurp the response fd into concat buffer
    emitter.instruction("stp x1, x2, [sp, #64]");                               // preserve response ptr/len across close
    emitter.instruction("ldr x0, [sp, #56]");                                   // reload response-body fd
    emitter.syscall(6);                                                         // close the temporary response stream
    emitter.instruction("ldp x1, x2, [sp, #64]");                               // restore response ptr/len
    emitter.instruction("bl __rt_str_persist");                                 // persist the response string for file_get_contents ownership
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release helper frame
    emitter.instruction("ret");                                                 // return owned response string

    emitter.label("__rt_fgc_url_http_fail");
    emitter.instruction("mov x1, #0");                                          // null string ptr → PHP false
    emitter.instruction("mov x2, #0");                                          // zero failure length
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release helper frame
    emitter.instruction("ret");                                                 // return failure

    emitter.label("__rt_fgc_url_check_https");
    emitter.instruction("cmp x2, #8");                                          // URL long enough for "https://"?
    emitter.instruction("b.lt __rt_fgc_url_check_ftps");                        // too short for https: try FTPS
    emitter.instruction("ldrb w9, [x1, #0]");                                   // URL byte 0
    emitter.instruction("cmp w9, #0x68");                                       // 'h'
    emitter.instruction("b.ne __rt_fgc_url_check_ftps");                        // not https://
    emitter.instruction("ldrb w9, [x1, #1]");                                   // URL byte 1
    emitter.instruction("cmp w9, #0x74");                                       // 't'
    emitter.instruction("b.ne __rt_fgc_url_check_ftps");                        // not https://
    emitter.instruction("ldrb w9, [x1, #2]");                                   // URL byte 2
    emitter.instruction("cmp w9, #0x74");                                       // 't'
    emitter.instruction("b.ne __rt_fgc_url_check_ftps");                        // not https://
    emitter.instruction("ldrb w9, [x1, #3]");                                   // URL byte 3
    emitter.instruction("cmp w9, #0x70");                                       // 'p'
    emitter.instruction("b.ne __rt_fgc_url_check_ftps");                        // not https://
    emitter.instruction("ldrb w9, [x1, #4]");                                   // URL byte 4
    emitter.instruction("cmp w9, #0x73");                                       // 's'
    emitter.instruction("b.ne __rt_fgc_url_check_ftps");                        // not https://
    emitter.instruction("ldrb w9, [x1, #5]");                                   // URL byte 5
    emitter.instruction("cmp w9, #0x3a");                                       // ':'
    emitter.instruction("b.ne __rt_fgc_url_check_ftps");                        // not https://
    emitter.instruction("ldrb w9, [x1, #6]");                                   // URL byte 6
    emitter.instruction("cmp w9, #0x2f");                                       // '/'
    emitter.instruction("b.ne __rt_fgc_url_check_ftps");                        // not https://
    emitter.instruction("ldrb w9, [x1, #7]");                                   // URL byte 7
    emitter.instruction("cmp w9, #0x2f");                                       // '/'
    emitter.instruction("b.ne __rt_fgc_url_check_ftps");                        // not https://

    // Frame: [0]=url ptr [8]=url len [16]=host header ptr [24]=host header len
    //        [32]=path ptr [40]=path len [48]=connect host ptr
    //        [56]=connect host len [64]=port [72]=fd [80]=str ptr
    //        [88]=str len [96]=x29/x30.
    emitter.instruction("sub sp, sp, #112");                                    // allocate the dynamic-HTTPS helper frame
    emitter.instruction("stp x29, x30, [sp, #96]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #96");                                    // establish the helper frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save URL pointer
    emitter.instruction("str x2, [sp, #8]");                                    // save URL length

    emitter.instruction("mov x11, #8");                                         // scan index starts after "https://"
    emitter.label("__rt_fgc_url_https_slash_scan");
    emitter.instruction("cmp x11, x2");                                         // reached URL end while scanning for path?
    emitter.instruction("b.ge __rt_fgc_url_https_no_path");                     // no slash: synthesize "/"
    emitter.instruction("ldrb w12, [x1, x11]");                                 // current URL byte
    emitter.instruction("cmp w12, #0x2f");                                      // slash starts the path?
    emitter.instruction("b.eq __rt_fgc_url_https_have_path");                   // path starts at this slash
    emitter.instruction("add x11, x11, #1");                                    // advance the scan index
    emitter.instruction("b __rt_fgc_url_https_slash_scan");                     // keep scanning for path
    emitter.label("__rt_fgc_url_https_no_path");
    abi::emit_symbol_address(emitter, "x12", "_fgc_url_slash");
    emitter.instruction("str x12, [sp, #32]");                                  // path pointer = "/"
    emitter.instruction("mov x12, #1");                                         // path length = 1
    emitter.instruction("str x12, [sp, #40]");                                  // save synthesized path length
    emitter.instruction("ldr x11, [sp, #8]");                                   // authority end = URL length
    emitter.instruction("b __rt_fgc_url_https_after_path");                     // continue with authority parsing
    emitter.label("__rt_fgc_url_https_have_path");
    emitter.instruction("add x12, x1, x11");                                    // path pointer = URL + slash index
    emitter.instruction("str x12, [sp, #32]");                                  // save path pointer
    emitter.instruction("sub x12, x2, x11");                                    // path length = URL length - slash index
    emitter.instruction("str x12, [sp, #40]");                                  // save path length
    emitter.label("__rt_fgc_url_https_after_path");

    emitter.instruction("mov x13, #8");                                         // host start index
    emitter.instruction("mov x14, #8");                                         // userinfo scan index
    emitter.label("__rt_fgc_url_https_userinfo_scan");
    emitter.instruction("cmp x14, x11");                                        // reached authority end?
    emitter.instruction("b.ge __rt_fgc_url_https_userinfo_done");               // host start is final
    emitter.instruction("ldrb w12, [x1, x14]");                                 // authority byte
    emitter.instruction("cmp w12, #0x40");                                      // '@' userinfo separator?
    emitter.instruction("b.ne __rt_fgc_url_https_userinfo_next");               // not userinfo separator
    emitter.instruction("add x13, x14, #1");                                    // host starts after the separator
    emitter.label("__rt_fgc_url_https_userinfo_next");
    emitter.instruction("add x14, x14, #1");                                    // advance authority scan
    emitter.instruction("b __rt_fgc_url_https_userinfo_scan");                  // keep scanning authority
    emitter.label("__rt_fgc_url_https_userinfo_done");
    emitter.instruction("sub x14, x11, x13");                                   // host-header length
    emitter.instruction("cmp x14, #0");                                         // empty authority after userinfo?
    emitter.instruction("b.le __rt_fgc_url_https_fail");                        // empty host is an invalid URL
    emitter.instruction("add x12, x1, x13");                                    // host-header pointer inside original URL
    emitter.instruction("str x12, [sp, #16]");                                  // save host-header pointer
    emitter.instruction("str x14, [sp, #24]");                                  // save host-header length
    emitter.instruction("str x12, [sp, #48]");                                  // default connect host pointer = host-header pointer
    emitter.instruction("str x14, [sp, #56]");                                  // default connect host length = host-header length
    emitter.instruction("mov x15, #443");                                       // default HTTPS port
    emitter.instruction("str x15, [sp, #64]");                                  // save default port

    emitter.instruction("mov x15, #0");                                         // authority scan offset
    emitter.label("__rt_fgc_url_https_port_scan");
    emitter.instruction("cmp x15, x14");                                        // scanned every authority byte?
    emitter.instruction("b.ge __rt_fgc_url_https_port_done");                   // no explicit port found
    emitter.instruction("ldrb w12, [x1, x13]");                                 // next authority byte
    emitter.instruction("cmp w12, #0x3a");                                      // ':' starts the explicit port
    emitter.instruction("b.eq __rt_fgc_url_https_port_found");                  // parse the explicit port digits
    emitter.instruction("add x13, x13, #1");                                    // advance source index
    emitter.instruction("add x15, x15, #1");                                    // advance authority scan offset
    emitter.instruction("b __rt_fgc_url_https_port_scan");                      // keep scanning for a port separator
    emitter.label("__rt_fgc_url_https_port_found");
    emitter.instruction("str x15, [sp, #56]");                                  // connect host length excludes ':port'
    emitter.instruction("add x13, x13, #1");                                    // first port digit index
    emitter.instruction("add x15, x15, #1");                                    // first port digit offset
    emitter.instruction("cmp x15, x14");                                        // any port digits present?
    emitter.instruction("b.ge __rt_fgc_url_https_fail");                        // empty port is invalid
    emitter.instruction("mov x16, #0");                                         // parsed port accumulator
    emitter.label("__rt_fgc_url_https_port_digits");
    emitter.instruction("cmp x15, x14");                                        // consumed all port digits?
    emitter.instruction("b.ge __rt_fgc_url_https_store_port");                  // parsed port is ready
    emitter.instruction("ldrb w12, [x1, x13]");                                 // port digit byte
    emitter.instruction("cmp w12, #0x30");                                      // below '0'?
    emitter.instruction("b.lt __rt_fgc_url_https_fail");                        // non-digit port byte
    emitter.instruction("cmp w12, #0x39");                                      // above '9'?
    emitter.instruction("b.gt __rt_fgc_url_https_fail");                        // non-digit port byte
    emitter.instruction("sub w12, w12, #0x30");                                 // digit value
    emitter.instruction("mov x17, #10");                                        // decimal base
    emitter.instruction("mul x16, x16, x17");                                   // port *= 10
    emitter.instruction("add x16, x16, x12");                                   // port += digit
    emitter.instruction("add x13, x13, #1");                                    // advance source index
    emitter.instruction("add x15, x15, #1");                                    // advance digit offset
    emitter.instruction("b __rt_fgc_url_https_port_digits");                    // parse the next digit
    emitter.label("__rt_fgc_url_https_store_port");
    emitter.instruction("str x16, [sp, #64]");                                  // save explicit HTTPS port
    emitter.label("__rt_fgc_url_https_port_done");

    emitter.instruction("ldr x0, [sp, #16]");                                   // Host header pointer
    emitter.instruction("ldr x1, [sp, #24]");                                   // Host header length
    emitter.instruction("ldr x2, [sp, #32]");                                   // path pointer
    emitter.instruction("ldr x3, [sp, #40]");                                   // path length
    emitter.instruction("bl __rt_http_build_request");                          // build request into _http_req_scratch, returning length
    emitter.instruction("mov x4, x0");                                          // request length for __rt_https_open
    emitter.instruction("ldr x0, [sp, #48]");                                   // connect host pointer
    emitter.instruction("ldr x1, [sp, #56]");                                   // connect host length
    emitter.instruction("ldr x2, [sp, #64]");                                   // TCP port
    abi::emit_symbol_address(emitter, "x3", "_http_req_scratch");
    emitter.instruction("bl __rt_https_open");                                  // open HTTPS response-body fd
    emitter.instruction("cmp x0, #0");                                          // did the URL open fail?
    emitter.instruction("b.lt __rt_fgc_url_https_fail");                        // failed open returns PHP false
    emitter.instruction("str x0, [sp, #72]");                                   // save response-body fd
    emitter.instruction("bl __rt_stream_get_contents");                         // slurp the response fd into concat buffer
    emitter.instruction("stp x1, x2, [sp, #80]");                               // preserve response ptr/len across close
    emitter.instruction("ldr x0, [sp, #72]");                                   // reload response-body fd
    emitter.syscall(6);                                                         // close the temporary response stream
    emitter.instruction("ldp x1, x2, [sp, #80]");                               // restore response ptr/len
    emitter.instruction("bl __rt_str_persist");                                 // persist the response string for file_get_contents ownership
    emitter.instruction("ldp x29, x30, [sp, #96]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #112");                                    // release helper frame
    emitter.instruction("ret");                                                 // return owned HTTPS response string

    emitter.label("__rt_fgc_url_https_fail");
    emitter.instruction("mov x1, #0");                                          // null string ptr → PHP false
    emitter.instruction("mov x2, #0");                                          // zero failure length
    emitter.instruction("ldp x29, x30, [sp, #96]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #112");                                    // release helper frame
    emitter.instruction("ret");                                                 // return failure

    emitter.label("__rt_fgc_url_check_ftps");
    emitter.instruction("cmp x2, #7");                                          // URL long enough for "ftps://"?
    emitter.instruction("b.lt __rt_fgc_url_check_ftp");                         // too short for ftps: try FTP
    emitter.instruction("ldrb w9, [x1, #0]");                                   // URL byte 0
    emitter.instruction("cmp w9, #0x66");                                       // 'f'
    emitter.instruction("b.ne __rt_fgc_url_check_ftp");                         // not ftps://
    emitter.instruction("ldrb w9, [x1, #1]");                                   // URL byte 1
    emitter.instruction("cmp w9, #0x74");                                       // 't'
    emitter.instruction("b.ne __rt_fgc_url_check_ftp");                         // not ftps://
    emitter.instruction("ldrb w9, [x1, #2]");                                   // URL byte 2
    emitter.instruction("cmp w9, #0x70");                                       // 'p'
    emitter.instruction("b.ne __rt_fgc_url_check_ftp");                         // not ftps://
    emitter.instruction("ldrb w9, [x1, #3]");                                   // URL byte 3
    emitter.instruction("cmp w9, #0x73");                                       // 's'
    emitter.instruction("b.ne __rt_fgc_url_check_ftp");                         // not ftps://
    emitter.instruction("ldrb w9, [x1, #4]");                                   // URL byte 4
    emitter.instruction("cmp w9, #0x3a");                                       // ':'
    emitter.instruction("b.ne __rt_fgc_url_check_ftp");                         // not ftps://
    emitter.instruction("ldrb w9, [x1, #5]");                                   // URL byte 5
    emitter.instruction("cmp w9, #0x2f");                                       // '/'
    emitter.instruction("b.ne __rt_fgc_url_check_ftp");                         // not ftps://
    emitter.instruction("ldrb w9, [x1, #6]");                                   // URL byte 6
    emitter.instruction("cmp w9, #0x2f");                                       // '/'
    emitter.instruction("b.ne __rt_fgc_url_check_ftp");                         // not ftps://
    abi::emit_symbol_address(emitter, "x9", "_ftp_use_tls");
    emitter.instruction("mov x10, #1");                                         // flag this dynamic FTP open for AUTH TLS
    emitter.instruction("str x10, [x9]");                                       // publish the ftps:// TLS flag
    emitter.instruction("mov x3, #7");                                          // scheme prefix length = strlen("ftps://")
    emitter.instruction("b __rt_fgc_url_ftp_common");                           // parse authority/path with the shared FTP body

    emitter.label("__rt_fgc_url_check_ftp");
    emitter.instruction("cmp x2, #6");                                          // URL long enough for "ftp://"?
    emitter.instruction("b.lt __rt_fgc_url_plain");                             // too short: fall back to phar/filesystem
    emitter.instruction("ldrb w9, [x1, #0]");                                   // URL byte 0
    emitter.instruction("cmp w9, #0x66");                                       // 'f'
    emitter.instruction("b.ne __rt_fgc_url_plain");                             // not ftp://
    emitter.instruction("ldrb w9, [x1, #1]");                                   // URL byte 1
    emitter.instruction("cmp w9, #0x74");                                       // 't'
    emitter.instruction("b.ne __rt_fgc_url_plain");                             // not ftp://
    emitter.instruction("ldrb w9, [x1, #2]");                                   // URL byte 2
    emitter.instruction("cmp w9, #0x70");                                       // 'p'
    emitter.instruction("b.ne __rt_fgc_url_plain");                             // not ftp://
    emitter.instruction("ldrb w9, [x1, #3]");                                   // URL byte 3
    emitter.instruction("cmp w9, #0x3a");                                       // ':'
    emitter.instruction("b.ne __rt_fgc_url_plain");                             // not ftp://
    emitter.instruction("ldrb w9, [x1, #4]");                                   // URL byte 4
    emitter.instruction("cmp w9, #0x2f");                                       // '/'
    emitter.instruction("b.ne __rt_fgc_url_plain");                             // not ftp://
    emitter.instruction("ldrb w9, [x1, #5]");                                   // URL byte 5
    emitter.instruction("cmp w9, #0x2f");                                       // '/'
    emitter.instruction("b.ne __rt_fgc_url_plain");                             // not ftp://

    abi::emit_symbol_address(emitter, "x9", "_ftp_use_tls");
    emitter.instruction("str xzr, [x9]");                                       // plain ftp:// must not inherit an earlier TLS flag
    emitter.instruction("mov x3, #6");                                          // scheme prefix length = strlen("ftp://")
    emitter.label("__rt_fgc_url_ftp_common");
    emitter.instruction("sub sp, sp, #96");                                     // allocate the dynamic-FTP helper frame
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish the helper frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // save URL pointer
    emitter.instruction("str x2, [sp, #8]");                                    // save URL length

    emitter.instruction("mov x11, x3");                                         // scan index starts after the scheme prefix
    emitter.label("__rt_fgc_url_ftp_slash_scan");
    emitter.instruction("cmp x11, x2");                                         // reached URL end without a path?
    emitter.instruction("b.ge __rt_fgc_url_ftp_fail");                          // FTP URLs require a path
    emitter.instruction("ldrb w12, [x1, x11]");                                 // current URL byte
    emitter.instruction("cmp w12, #0x2f");                                      // slash starts the path?
    emitter.instruction("b.eq __rt_fgc_url_ftp_have_path");                     // path starts at this slash
    emitter.instruction("add x11, x11, #1");                                    // advance scan index
    emitter.instruction("b __rt_fgc_url_ftp_slash_scan");                       // keep scanning for path
    emitter.label("__rt_fgc_url_ftp_have_path");
    emitter.instruction("add x12, x1, x11");                                    // path pointer = URL + slash index
    emitter.instruction("str x12, [sp, #32]");                                  // save path pointer
    emitter.instruction("sub x12, x2, x11");                                    // path length = URL length - slash index
    emitter.instruction("cmp x12, #2");                                         // path must include at least one byte after '/'
    emitter.instruction("b.lt __rt_fgc_url_ftp_fail");                          // empty FTP path is invalid
    emitter.instruction("str x12, [sp, #40]");                                  // save path length

    emitter.instruction("mov x13, x3");                                         // host start index after the scheme prefix
    emitter.instruction("mov x14, x3");                                         // userinfo scan index after the scheme prefix
    emitter.label("__rt_fgc_url_ftp_userinfo_scan");
    emitter.instruction("cmp x14, x11");                                        // reached authority end?
    emitter.instruction("b.ge __rt_fgc_url_ftp_userinfo_done");                 // host start is final
    emitter.instruction("ldrb w12, [x1, x14]");                                 // authority byte
    emitter.instruction("cmp w12, #0x40");                                      // '@' userinfo separator?
    emitter.instruction("b.ne __rt_fgc_url_ftp_userinfo_next");                 // not userinfo separator
    emitter.instruction("add x13, x14, #1");                                    // host starts after the separator
    emitter.label("__rt_fgc_url_ftp_userinfo_next");
    emitter.instruction("add x14, x14, #1");                                    // advance authority scan
    emitter.instruction("b __rt_fgc_url_ftp_userinfo_scan");                    // keep scanning authority
    emitter.label("__rt_fgc_url_ftp_userinfo_done");
    emitter.instruction("sub x14, x11, x13");                                   // host/header length
    emitter.instruction("cmp x14, #0");                                         // empty authority after userinfo?
    emitter.instruction("b.le __rt_fgc_url_ftp_fail");                          // empty host is an invalid URL
    emitter.instruction("add x12, x1, x13");                                    // host pointer inside original URL
    emitter.instruction("str x12, [sp, #16]");                                  // save host pointer
    emitter.instruction("str x14, [sp, #24]");                                  // save host length

    abi::emit_symbol_address(emitter, "x9", "_fgc_url_addr");
    abi::emit_symbol_address(emitter, "x10", "_ftp_tcp_prefix");
    emitter.instruction("mov x15, #0");                                         // address write index
    emitter.label("__rt_fgc_url_ftp_tcp_prefix");
    emitter.instruction("cmp x15, #6");                                         // copied "tcp://" yet?
    emitter.instruction("b.ge __rt_fgc_url_ftp_tcp_prefix_done");               // prefix copied
    emitter.instruction("ldrb w12, [x10, x15]");                                // prefix byte
    emitter.instruction("strb w12, [x9, x15]");                                 // store prefix byte
    emitter.instruction("add x15, x15, #1");                                    // advance address write index
    emitter.instruction("b __rt_fgc_url_ftp_tcp_prefix");                       // copy the next prefix byte
    emitter.label("__rt_fgc_url_ftp_tcp_prefix_done");
    emitter.instruction("mov x16, #0");                                         // host copy index
    emitter.instruction("mov x17, #0");                                         // has explicit port? 0/1
    emitter.label("__rt_fgc_url_ftp_host_copy");
    emitter.instruction("cmp x16, x14");                                        // copied all host bytes?
    emitter.instruction("b.ge __rt_fgc_url_ftp_host_done");                     // host copy complete
    emitter.instruction("ldrb w12, [x1, x13]");                                 // next host byte
    emitter.instruction("cmp w12, #0x3a");                                      // ':' means an explicit port is present
    emitter.instruction("b.ne __rt_fgc_url_ftp_no_port");                       // leave the explicit-port flag unchanged otherwise
    emitter.instruction("mov x17, #1");                                         // remember that the authority already has a port
    emitter.label("__rt_fgc_url_ftp_no_port");
    emitter.instruction("strb w12, [x9, x15]");                                 // append host byte to the TCP address
    emitter.instruction("add x13, x13, #1");                                    // advance host source index
    emitter.instruction("add x16, x16, #1");                                    // advance host copy index
    emitter.instruction("add x15, x15, #1");                                    // advance address write index
    emitter.instruction("b __rt_fgc_url_ftp_host_copy");                        // copy the next host byte
    emitter.label("__rt_fgc_url_ftp_host_done");
    emitter.instruction("cbnz x17, __rt_fgc_url_ftp_addr_done");                // explicit port: do not append default :21
    emitter.instruction("mov w12, #0x3a");                                      // ':'
    emitter.instruction("strb w12, [x9, x15]");                                 // append ':'
    emitter.instruction("add x15, x15, #1");                                    // advance address write index
    emitter.instruction("mov w12, #0x32");                                      // '2'
    emitter.instruction("strb w12, [x9, x15]");                                 // append '2'
    emitter.instruction("add x15, x15, #1");                                    // advance address write index
    emitter.instruction("mov w12, #0x31");                                      // '1'
    emitter.instruction("strb w12, [x9, x15]");                                 // append '1'
    emitter.instruction("add x15, x15, #1");                                    // advance address write index
    emitter.label("__rt_fgc_url_ftp_addr_done");
    emitter.instruction("str x15, [sp, #48]");                                  // save TCP address length

    abi::emit_symbol_address(emitter, "x9", "_fgc_url_retr");
    emitter.instruction("mov w12, #0x52");                                      // 'R'
    emitter.instruction("strb w12, [x9, #0]");                                  // RETR byte 0
    emitter.instruction("mov w12, #0x45");                                      // 'E'
    emitter.instruction("strb w12, [x9, #1]");                                  // RETR byte 1
    emitter.instruction("mov w12, #0x54");                                      // 'T'
    emitter.instruction("strb w12, [x9, #2]");                                  // RETR byte 2
    emitter.instruction("mov w12, #0x52");                                      // 'R'
    emitter.instruction("strb w12, [x9, #3]");                                  // RETR byte 3
    emitter.instruction("mov w12, #0x20");                                      // space after RETR
    emitter.instruction("strb w12, [x9, #4]");                                  // RETR byte 4
    emitter.instruction("mov x15, #5");                                         // RETR write index after prefix
    emitter.instruction("mov x16, #0");                                         // path copy index
    emitter.instruction("ldr x10, [sp, #32]");                                  // path pointer
    emitter.instruction("ldr x11, [sp, #40]");                                  // path length
    emitter.label("__rt_fgc_url_ftp_retr_path");
    emitter.instruction("cmp x16, x11");                                        // copied all path bytes?
    emitter.instruction("b.ge __rt_fgc_url_ftp_retr_done_path");                // path copied
    emitter.instruction("ldrb w12, [x10, x16]");                                // next path byte
    emitter.instruction("strb w12, [x9, x15]");                                 // append path byte
    emitter.instruction("add x16, x16, #1");                                    // advance path index
    emitter.instruction("add x15, x15, #1");                                    // advance RETR write index
    emitter.instruction("b __rt_fgc_url_ftp_retr_path");                        // copy the next path byte
    emitter.label("__rt_fgc_url_ftp_retr_done_path");
    emitter.instruction("mov w12, #0x0d");                                      // carriage return
    emitter.instruction("strb w12, [x9, x15]");                                 // append CR
    emitter.instruction("add x15, x15, #1");                                    // advance RETR write index
    emitter.instruction("mov w12, #0x0a");                                      // line feed
    emitter.instruction("strb w12, [x9, x15]");                                 // append LF
    emitter.instruction("add x15, x15, #1");                                    // RETR command length

    abi::emit_symbol_address(emitter, "x0", "_fgc_url_addr");
    emitter.instruction("ldr x1, [sp, #48]");                                   // TCP control address length
    abi::emit_symbol_address(emitter, "x2", "_fgc_url_retr");
    emitter.instruction("mov x3, x15");                                         // RETR command length
    emitter.instruction("bl __rt_ftp_open");                                    // open FTP data fd
    emitter.instruction("cmp x0, #0");                                          // did the FTP open fail?
    emitter.instruction("b.lt __rt_fgc_url_ftp_fail");                          // failed open returns PHP false
    emitter.instruction("str x0, [sp, #56]");                                   // save FTP data fd
    emitter.instruction("bl __rt_stream_get_contents");                         // slurp the FTP data fd into concat buffer
    emitter.instruction("stp x1, x2, [sp, #64]");                               // preserve response ptr/len across close
    emitter.instruction("ldr x0, [sp, #56]");                                   // reload FTP data fd
    abi::emit_symbol_address(emitter, "x9", "_tls_sessions");
    emitter.instruction("ldr x10, [x9, x0, lsl #3]");                           // TLS session attached to this data fd?
    emitter.instruction("cbz x10, __rt_fgc_url_ftp_close_plain");               // plain FTP data fd: close directly
    emitter.instruction("mov x0, x10");                                         // TLS handle as close helper argument
    abi::emit_symbol_address(emitter, "x9", "_elephc_tls_close_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load elephc_tls_close entry pointer
    emitter.instruction("blr x9");                                              // send close_notify and drop the TLS session
    emitter.instruction("ldr x0, [sp, #56]");                                   // reload FTP data fd after TLS close
    abi::emit_symbol_address(emitter, "x9", "_tls_sessions");
    emitter.instruction("str xzr, [x9, x0, lsl #3]");                           // clear the TLS session slot for descriptor reuse
    emitter.label("__rt_fgc_url_ftp_close_plain");
    emitter.syscall(6);                                                         // close the data connection
    emitter.instruction("ldp x1, x2, [sp, #64]");                               // restore response ptr/len
    emitter.instruction("bl __rt_str_persist");                                 // persist the response string for file_get_contents ownership
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release helper frame
    emitter.instruction("ret");                                                 // return owned FTP response string

    emitter.label("__rt_fgc_url_ftp_fail");
    abi::emit_symbol_address(emitter, "x9", "_ftp_use_tls");
    emitter.instruction("str xzr, [x9]");                                       // clear the one-shot AUTH-TLS flag after any FTP failure
    emitter.instruction("mov x1, #0");                                          // null string ptr → PHP false
    emitter.instruction("mov x2, #0");                                          // zero failure length
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release helper frame
    emitter.instruction("ret");                                                 // return failure
    emitter.label("__rt_fgc_url_plain");
    emitter.instruction("b __rt_file_get_contents_maybe_phar");                 // fallback to phar:// runtime reader or filesystem path
}

/// Emits the x86_64 variant of `__rt_file_get_contents_maybe_url`.
fn emit_file_get_contents_url_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: file_get_contents_maybe_url ---");
    emitter.label_global("__rt_file_get_contents_maybe_url");

    emitter.instruction("cmp rdx, 7");                                          // URL long enough for "http://"?
    emitter.instruction("jl __rt_fgc_url_check_https_x86");                     // too short for http: try the next URL scheme
    emitter.instruction("cmp BYTE PTR [rax + 0], 0x68");                        // 'h'
    emitter.instruction("jne __rt_fgc_url_check_https_x86");                    // not http://
    emitter.instruction("cmp BYTE PTR [rax + 1], 0x74");                        // 't'
    emitter.instruction("jne __rt_fgc_url_check_https_x86");                    // not http://
    emitter.instruction("cmp BYTE PTR [rax + 2], 0x74");                        // 't'
    emitter.instruction("jne __rt_fgc_url_check_https_x86");                    // not http://
    emitter.instruction("cmp BYTE PTR [rax + 3], 0x70");                        // 'p'
    emitter.instruction("jne __rt_fgc_url_check_https_x86");                    // not http://
    emitter.instruction("cmp BYTE PTR [rax + 4], 0x3a");                        // ':'
    emitter.instruction("jne __rt_fgc_url_check_https_x86");                    // not http://
    emitter.instruction("cmp BYTE PTR [rax + 5], 0x2f");                        // '/'
    emitter.instruction("jne __rt_fgc_url_check_https_x86");                    // not http://
    emitter.instruction("cmp BYTE PTR [rax + 6], 0x2f");                        // '/'
    emitter.instruction("jne __rt_fgc_url_check_https_x86");                    // not http://

    // Frame: [rbp-8]=url ptr [rbp-16]=url len [rbp-24]=host ptr
    //        [rbp-32]=host len [rbp-40]=path ptr [rbp-48]=path len
    //        [rbp-56]=addr len [rbp-64]=fd [rbp-72]=str ptr
    //        [rbp-80]=str len [rbp-88]=authority end [rbp-96]=has port.
    emitter.instruction("push rbp");                                            // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish helper frame pointer
    emitter.instruction("sub rsp, 112");                                        // reserve aligned dynamic-URL locals
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save URL pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save URL length

    emitter.instruction("mov r8, 7");                                           // scan index starts after "http://"
    emitter.label("__rt_fgc_url_http_slash_scan_x86");
    emitter.instruction("cmp r8, rdx");                                         // reached URL end while scanning for path?
    emitter.instruction("jge __rt_fgc_url_http_no_path_x86");                   // no slash: synthesize "/"
    emitter.instruction("cmp BYTE PTR [rax + r8], 0x2f");                       // slash starts the path?
    emitter.instruction("je __rt_fgc_url_http_have_path_x86");                  // path starts at this slash
    emitter.instruction("inc r8");                                              // advance scan index
    emitter.instruction("jmp __rt_fgc_url_http_slash_scan_x86");                // keep scanning for path
    emitter.label("__rt_fgc_url_http_no_path_x86");
    emitter.instruction("lea r10, [rip + _fgc_url_slash]");                     // synthesized "/" path pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // save path pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], 1");                         // save path length
    emitter.instruction("mov r8, rdx");                                         // authority end = URL length
    emitter.instruction("jmp __rt_fgc_url_http_after_path_x86");                // continue with authority parsing
    emitter.label("__rt_fgc_url_http_have_path_x86");
    emitter.instruction("lea r10, [rax + r8]");                                 // path pointer = URL + slash index
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // save path pointer
    emitter.instruction("mov r10, rdx");                                        // URL length
    emitter.instruction("sub r10, r8");                                         // path length = URL length - slash index
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // save path length
    emitter.label("__rt_fgc_url_http_after_path_x86");
    emitter.instruction("mov QWORD PTR [rbp - 88], r8");                        // save authority end

    emitter.instruction("mov r9, 7");                                           // host start index
    emitter.instruction("mov rcx, 7");                                          // userinfo scan index
    emitter.label("__rt_fgc_url_http_userinfo_scan_x86");
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 88]");                       // reached authority end?
    emitter.instruction("jge __rt_fgc_url_http_userinfo_done_x86");             // host start is final
    emitter.instruction("cmp BYTE PTR [rax + rcx], 0x40");                      // '@' userinfo separator?
    emitter.instruction("jne __rt_fgc_url_http_userinfo_next_x86");             // not userinfo separator
    emitter.instruction("lea r9, [rcx + 1]");                                   // host starts after the separator
    emitter.label("__rt_fgc_url_http_userinfo_next_x86");
    emitter.instruction("inc rcx");                                             // advance authority scan
    emitter.instruction("jmp __rt_fgc_url_http_userinfo_scan_x86");             // keep scanning authority
    emitter.label("__rt_fgc_url_http_userinfo_done_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 88]");                       // authority end
    emitter.instruction("sub r10, r9");                                         // host/header length
    emitter.instruction("cmp r10, 0");                                          // empty authority after userinfo?
    emitter.instruction("jle __rt_fgc_url_http_fail_x86");                      // empty host is an invalid URL
    emitter.instruction("lea r11, [rax + r9]");                                 // host pointer inside original URL
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // save host/header pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // save host/header length

    emitter.instruction("lea r10, [rip + _fgc_url_addr]");                      // TCP address scratch buffer
    emitter.instruction("lea r11, [rip + _ftp_tcp_prefix]");                    // "tcp://" prefix source
    emitter.instruction("xor rcx, rcx");                                        // address write index
    emitter.label("__rt_fgc_url_http_tcp_prefix_x86");
    emitter.instruction("cmp rcx, 6");                                          // copied "tcp://" yet?
    emitter.instruction("jge __rt_fgc_url_http_tcp_prefix_done_x86");           // prefix copied
    emitter.instruction("mov r8b, BYTE PTR [r11 + rcx]");                       // prefix byte
    emitter.instruction("mov BYTE PTR [r10 + rcx], r8b");                       // store prefix byte
    emitter.instruction("inc rcx");                                             // advance address write index
    emitter.instruction("jmp __rt_fgc_url_http_tcp_prefix_x86");                // copy the next prefix byte
    emitter.label("__rt_fgc_url_http_tcp_prefix_done_x86");
    emitter.instruction("mov QWORD PTR [rbp - 96], 0");                         // has explicit port? 0/1
    emitter.instruction("xor r8, r8");                                          // host copy index
    emitter.label("__rt_fgc_url_http_host_copy_x86");
    emitter.instruction("cmp r8, QWORD PTR [rbp - 32]");                        // copied all host/header bytes?
    emitter.instruction("jge __rt_fgc_url_http_host_done_x86");                 // host copy complete
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // host/header pointer
    emitter.instruction("mov r9b, BYTE PTR [r11 + r8]");                        // next host byte
    emitter.instruction("cmp r9b, 0x3a");                                       // ':' means an explicit port is present
    emitter.instruction("jne __rt_fgc_url_http_not_port_x86");                  // not a port separator
    emitter.instruction("mov QWORD PTR [rbp - 96], 1");                         // remember explicit port
    emitter.label("__rt_fgc_url_http_not_port_x86");
    emitter.instruction("mov BYTE PTR [r10 + rcx], r9b");                       // append host byte to TCP address
    emitter.instruction("inc r8");                                              // advance host copy index
    emitter.instruction("inc rcx");                                             // advance address write index
    emitter.instruction("jmp __rt_fgc_url_http_host_copy_x86");                 // copy the next host byte
    emitter.label("__rt_fgc_url_http_host_done_x86");
    emitter.instruction("cmp QWORD PTR [rbp - 96], 0");                         // was a port present?
    emitter.instruction("jne __rt_fgc_url_http_addr_done_x86");                 // explicit port: do not append default :80
    emitter.instruction("mov BYTE PTR [r10 + rcx], 0x3a");                      // append ':'
    emitter.instruction("inc rcx");                                             // advance address write index
    emitter.instruction("mov BYTE PTR [r10 + rcx], 0x38");                      // append '8'
    emitter.instruction("inc rcx");                                             // advance address write index
    emitter.instruction("mov BYTE PTR [r10 + rcx], 0x30");                      // append '0'
    emitter.instruction("inc rcx");                                             // advance address write index
    emitter.label("__rt_fgc_url_http_addr_done_x86");
    emitter.instruction("mov QWORD PTR [rbp - 56], rcx");                       // save TCP address length

    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // host/header pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // host/header length
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // path pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // path length
    emitter.instruction("call __rt_http_build_request");                        // build request into _http_req_scratch, returning length
    emitter.instruction("lea rdi, [rip + _fgc_url_addr]");                      // TCP address pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // TCP address length
    emitter.instruction("lea rdx, [rip + _http_req_scratch]");                  // HTTP request pointer
    emitter.instruction("mov rcx, rax");                                        // HTTP request length
    emitter.instruction("call __rt_http_open");                                 // open HTTP response-body fd
    emitter.instruction("cmp rax, 0");                                          // did the URL open fail?
    emitter.instruction("jl __rt_fgc_url_http_fail_x86");                       // failed open returns PHP false
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // save response-body fd
    emitter.instruction("mov rdi, rax");                                        // fd for stream_get_contents
    emitter.instruction("call __rt_stream_get_contents");                       // slurp the response fd into concat buffer
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // save response ptr across close
    emitter.instruction("mov QWORD PTR [rbp - 80], rdx");                       // save response length across close
    emitter.instruction("mov rdi, QWORD PTR [rbp - 64]");                       // reload response-body fd
    emitter.instruction("call close");                                          // close the temporary response stream
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // restore response ptr
    emitter.instruction("mov rdx, QWORD PTR [rbp - 80]");                       // restore response length
    emitter.instruction("call __rt_str_persist");                               // persist the response string for file_get_contents ownership
    emitter.instruction("add rsp, 112");                                        // release helper locals
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return owned response string

    emitter.label("__rt_fgc_url_http_fail_x86");
    emitter.instruction("xor eax, eax");                                        // null string ptr → PHP false
    emitter.instruction("xor edx, edx");                                        // zero failure length
    emitter.instruction("add rsp, 112");                                        // release helper locals
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return failure

    emitter.label("__rt_fgc_url_check_https_x86");
    emitter.instruction("cmp rdx, 8");                                          // URL long enough for "https://"?
    emitter.instruction("jl __rt_fgc_url_check_ftps_x86");                      // too short for https: try FTPS
    emitter.instruction("cmp BYTE PTR [rax + 0], 0x68");                        // 'h'
    emitter.instruction("jne __rt_fgc_url_check_ftps_x86");                     // not https://
    emitter.instruction("cmp BYTE PTR [rax + 1], 0x74");                        // 't'
    emitter.instruction("jne __rt_fgc_url_check_ftps_x86");                     // not https://
    emitter.instruction("cmp BYTE PTR [rax + 2], 0x74");                        // 't'
    emitter.instruction("jne __rt_fgc_url_check_ftps_x86");                     // not https://
    emitter.instruction("cmp BYTE PTR [rax + 3], 0x70");                        // 'p'
    emitter.instruction("jne __rt_fgc_url_check_ftps_x86");                     // not https://
    emitter.instruction("cmp BYTE PTR [rax + 4], 0x73");                        // 's'
    emitter.instruction("jne __rt_fgc_url_check_ftps_x86");                     // not https://
    emitter.instruction("cmp BYTE PTR [rax + 5], 0x3a");                        // ':'
    emitter.instruction("jne __rt_fgc_url_check_ftps_x86");                     // not https://
    emitter.instruction("cmp BYTE PTR [rax + 6], 0x2f");                        // '/'
    emitter.instruction("jne __rt_fgc_url_check_ftps_x86");                     // not https://
    emitter.instruction("cmp BYTE PTR [rax + 7], 0x2f");                        // '/'
    emitter.instruction("jne __rt_fgc_url_check_ftps_x86");                     // not https://

    // Frame: [rbp-8]=url ptr [rbp-16]=url len [rbp-24]=host-header ptr
    //        [rbp-32]=host-header len [rbp-40]=path ptr [rbp-48]=path len
    //        [rbp-56]=connect host ptr [rbp-64]=connect host len
    //        [rbp-72]=port [rbp-80]=fd [rbp-88]=str ptr [rbp-96]=str len
    //        [rbp-104]=authority end.
    emitter.instruction("push rbp");                                            // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish helper frame pointer
    emitter.instruction("sub rsp, 112");                                        // reserve aligned dynamic-HTTPS locals
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save URL pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save URL length

    emitter.instruction("mov r8, 8");                                           // scan index starts after "https://"
    emitter.label("__rt_fgc_url_https_slash_scan_x86");
    emitter.instruction("cmp r8, rdx");                                         // reached URL end while scanning for path?
    emitter.instruction("jge __rt_fgc_url_https_no_path_x86");                  // no slash: synthesize "/"
    emitter.instruction("cmp BYTE PTR [rax + r8], 0x2f");                       // slash starts the path?
    emitter.instruction("je __rt_fgc_url_https_have_path_x86");                 // path starts at this slash
    emitter.instruction("inc r8");                                              // advance scan index
    emitter.instruction("jmp __rt_fgc_url_https_slash_scan_x86");               // keep scanning for path
    emitter.label("__rt_fgc_url_https_no_path_x86");
    emitter.instruction("lea r10, [rip + _fgc_url_slash]");                     // synthesized "/" path pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // save path pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], 1");                         // save path length
    emitter.instruction("mov r8, rdx");                                         // authority end = URL length
    emitter.instruction("jmp __rt_fgc_url_https_after_path_x86");               // continue with authority parsing
    emitter.label("__rt_fgc_url_https_have_path_x86");
    emitter.instruction("lea r10, [rax + r8]");                                 // path pointer = URL + slash index
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // save path pointer
    emitter.instruction("mov r10, rdx");                                        // URL length
    emitter.instruction("sub r10, r8");                                         // path length = URL length - slash index
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // save path length
    emitter.label("__rt_fgc_url_https_after_path_x86");
    emitter.instruction("mov QWORD PTR [rbp - 104], r8");                       // save authority end

    emitter.instruction("mov r9, 8");                                           // host start index
    emitter.instruction("mov rcx, 8");                                          // userinfo scan index
    emitter.label("__rt_fgc_url_https_userinfo_scan_x86");
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 104]");                      // reached authority end?
    emitter.instruction("jge __rt_fgc_url_https_userinfo_done_x86");            // host start is final
    emitter.instruction("cmp BYTE PTR [rax + rcx], 0x40");                      // '@' userinfo separator?
    emitter.instruction("jne __rt_fgc_url_https_userinfo_next_x86");            // not userinfo separator
    emitter.instruction("lea r9, [rcx + 1]");                                   // host starts after the separator
    emitter.label("__rt_fgc_url_https_userinfo_next_x86");
    emitter.instruction("inc rcx");                                             // advance authority scan
    emitter.instruction("jmp __rt_fgc_url_https_userinfo_scan_x86");            // keep scanning authority
    emitter.label("__rt_fgc_url_https_userinfo_done_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 104]");                      // authority end
    emitter.instruction("sub r10, r9");                                         // host-header length
    emitter.instruction("cmp r10, 0");                                          // empty authority after userinfo?
    emitter.instruction("jle __rt_fgc_url_https_fail_x86");                     // empty host is an invalid URL
    emitter.instruction("lea r11, [rax + r9]");                                 // host-header pointer inside original URL
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // save host-header pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // save host-header length
    emitter.instruction("mov QWORD PTR [rbp - 56], r11");                       // default connect host pointer = host-header pointer
    emitter.instruction("mov QWORD PTR [rbp - 64], r10");                       // default connect host length = host-header length
    emitter.instruction("mov QWORD PTR [rbp - 72], 443");                       // default HTTPS port

    emitter.instruction("xor r8, r8");                                          // authority scan offset
    emitter.label("__rt_fgc_url_https_port_scan_x86");
    emitter.instruction("cmp r8, QWORD PTR [rbp - 32]");                        // scanned every authority byte?
    emitter.instruction("jge __rt_fgc_url_https_port_done_x86");                // no explicit port found
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // host-header pointer
    emitter.instruction("cmp BYTE PTR [r11 + r8], 0x3a");                       // ':' starts the explicit port
    emitter.instruction("je __rt_fgc_url_https_port_found_x86");                // parse the explicit port digits
    emitter.instruction("inc r8");                                              // advance authority scan offset
    emitter.instruction("jmp __rt_fgc_url_https_port_scan_x86");                // keep scanning for a port separator
    emitter.label("__rt_fgc_url_https_port_found_x86");
    emitter.instruction("mov QWORD PTR [rbp - 64], r8");                        // connect host length excludes ':port'
    emitter.instruction("inc r8");                                              // first port digit offset
    emitter.instruction("cmp r8, QWORD PTR [rbp - 32]");                        // any port digits present?
    emitter.instruction("jge __rt_fgc_url_https_fail_x86");                     // empty port is invalid
    emitter.instruction("xor r10, r10");                                        // parsed port accumulator
    emitter.label("__rt_fgc_url_https_port_digits_x86");
    emitter.instruction("cmp r8, QWORD PTR [rbp - 32]");                        // consumed all port digits?
    emitter.instruction("jge __rt_fgc_url_https_store_port_x86");               // parsed port is ready
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // host-header pointer
    emitter.instruction("movzx ecx, BYTE PTR [r11 + r8]");                      // port digit byte
    emitter.instruction("cmp ecx, 0x30");                                       // below '0'?
    emitter.instruction("jl __rt_fgc_url_https_fail_x86");                      // non-digit port byte
    emitter.instruction("cmp ecx, 0x39");                                       // above '9'?
    emitter.instruction("jg __rt_fgc_url_https_fail_x86");                      // non-digit port byte
    emitter.instruction("sub ecx, 0x30");                                       // digit value
    emitter.instruction("imul r10, r10, 10");                                   // port *= 10
    emitter.instruction("add r10, rcx");                                        // port += digit
    emitter.instruction("inc r8");                                              // advance digit offset
    emitter.instruction("jmp __rt_fgc_url_https_port_digits_x86");              // parse the next digit
    emitter.label("__rt_fgc_url_https_store_port_x86");
    emitter.instruction("mov QWORD PTR [rbp - 72], r10");                       // save explicit HTTPS port
    emitter.label("__rt_fgc_url_https_port_done_x86");

    emitter.instruction("mov rdi, QWORD PTR [rbp - 24]");                       // Host header pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // Host header length
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // path pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // path length
    emitter.instruction("call __rt_http_build_request");                        // build request into _http_req_scratch, returning length
    emitter.instruction("mov r8, rax");                                         // request length for __rt_https_open
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // connect host pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 64]");                       // connect host length
    emitter.instruction("mov rdx, QWORD PTR [rbp - 72]");                       // TCP port
    emitter.instruction("lea rcx, [rip + _http_req_scratch]");                  // HTTP request pointer
    emitter.instruction("call __rt_https_open");                                // open HTTPS response-body fd
    emitter.instruction("cmp rax, 0");                                          // did the URL open fail?
    emitter.instruction("jl __rt_fgc_url_https_fail_x86");                      // failed open returns PHP false
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // save response-body fd
    emitter.instruction("mov rdi, rax");                                        // fd for stream_get_contents
    emitter.instruction("call __rt_stream_get_contents");                       // slurp the response fd into concat buffer
    emitter.instruction("mov QWORD PTR [rbp - 88], rax");                       // save response ptr across close
    emitter.instruction("mov QWORD PTR [rbp - 96], rdx");                       // save response length across close
    emitter.instruction("mov rdi, QWORD PTR [rbp - 80]");                       // reload response-body fd
    emitter.instruction("call close");                                          // close the temporary response stream
    emitter.instruction("mov rax, QWORD PTR [rbp - 88]");                       // restore response ptr
    emitter.instruction("mov rdx, QWORD PTR [rbp - 96]");                       // restore response length
    emitter.instruction("call __rt_str_persist");                               // persist the response string for file_get_contents ownership
    emitter.instruction("add rsp, 112");                                        // release helper locals
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return owned HTTPS response string

    emitter.label("__rt_fgc_url_https_fail_x86");
    emitter.instruction("xor eax, eax");                                        // null string ptr → PHP false
    emitter.instruction("xor edx, edx");                                        // zero failure length
    emitter.instruction("add rsp, 112");                                        // release helper locals
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return failure

    emitter.label("__rt_fgc_url_check_ftps_x86");
    emitter.instruction("cmp rdx, 7");                                          // URL long enough for "ftps://"?
    emitter.instruction("jl __rt_fgc_url_check_ftp_x86");                       // too short for ftps: try FTP
    emitter.instruction("cmp BYTE PTR [rax + 0], 0x66");                        // 'f'
    emitter.instruction("jne __rt_fgc_url_check_ftp_x86");                      // not ftps://
    emitter.instruction("cmp BYTE PTR [rax + 1], 0x74");                        // 't'
    emitter.instruction("jne __rt_fgc_url_check_ftp_x86");                      // not ftps://
    emitter.instruction("cmp BYTE PTR [rax + 2], 0x70");                        // 'p'
    emitter.instruction("jne __rt_fgc_url_check_ftp_x86");                      // not ftps://
    emitter.instruction("cmp BYTE PTR [rax + 3], 0x73");                        // 's'
    emitter.instruction("jne __rt_fgc_url_check_ftp_x86");                      // not ftps://
    emitter.instruction("cmp BYTE PTR [rax + 4], 0x3a");                        // ':'
    emitter.instruction("jne __rt_fgc_url_check_ftp_x86");                      // not ftps://
    emitter.instruction("cmp BYTE PTR [rax + 5], 0x2f");                        // '/'
    emitter.instruction("jne __rt_fgc_url_check_ftp_x86");                      // not ftps://
    emitter.instruction("cmp BYTE PTR [rax + 6], 0x2f");                        // '/'
    emitter.instruction("jne __rt_fgc_url_check_ftp_x86");                      // not ftps://
    emitter.instruction("mov QWORD PTR [rip + _ftp_use_tls], 1");               // flag this dynamic FTP open for AUTH TLS
    emitter.instruction("mov r11, 7");                                          // scheme prefix length = strlen("ftps://")
    emitter.instruction("jmp __rt_fgc_url_ftp_common_x86");                     // parse authority/path with the shared FTP body

    emitter.label("__rt_fgc_url_check_ftp_x86");
    emitter.instruction("cmp rdx, 6");                                          // URL long enough for "ftp://"?
    emitter.instruction("jl __rt_fgc_url_plain_x86");                           // too short: fall back to phar/filesystem
    emitter.instruction("cmp BYTE PTR [rax + 0], 0x66");                        // 'f'
    emitter.instruction("jne __rt_fgc_url_plain_x86");                          // not ftp://
    emitter.instruction("cmp BYTE PTR [rax + 1], 0x74");                        // 't'
    emitter.instruction("jne __rt_fgc_url_plain_x86");                          // not ftp://
    emitter.instruction("cmp BYTE PTR [rax + 2], 0x70");                        // 'p'
    emitter.instruction("jne __rt_fgc_url_plain_x86");                          // not ftp://
    emitter.instruction("cmp BYTE PTR [rax + 3], 0x3a");                        // ':'
    emitter.instruction("jne __rt_fgc_url_plain_x86");                          // not ftp://
    emitter.instruction("cmp BYTE PTR [rax + 4], 0x2f");                        // '/'
    emitter.instruction("jne __rt_fgc_url_plain_x86");                          // not ftp://
    emitter.instruction("cmp BYTE PTR [rax + 5], 0x2f");                        // '/'
    emitter.instruction("jne __rt_fgc_url_plain_x86");                          // not ftp://

    emitter.instruction("mov QWORD PTR [rip + _ftp_use_tls], 0");               // plain ftp:// must not inherit an earlier TLS flag
    emitter.instruction("mov r11, 6");                                          // scheme prefix length = strlen("ftp://")
    emitter.label("__rt_fgc_url_ftp_common_x86");
    emitter.instruction("push rbp");                                            // preserve caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish helper frame pointer
    emitter.instruction("sub rsp, 112");                                        // reserve aligned dynamic-FTP locals
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save URL pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save URL length

    emitter.instruction("mov r8, r11");                                         // scan index starts after the scheme prefix
    emitter.label("__rt_fgc_url_ftp_slash_scan_x86");
    emitter.instruction("cmp r8, rdx");                                         // reached URL end without a path?
    emitter.instruction("jge __rt_fgc_url_ftp_fail_x86");                       // FTP URLs require a path
    emitter.instruction("cmp BYTE PTR [rax + r8], 0x2f");                       // slash starts the path?
    emitter.instruction("je __rt_fgc_url_ftp_have_path_x86");                   // path starts at this slash
    emitter.instruction("inc r8");                                              // advance scan index
    emitter.instruction("jmp __rt_fgc_url_ftp_slash_scan_x86");                 // keep scanning for path
    emitter.label("__rt_fgc_url_ftp_have_path_x86");
    emitter.instruction("lea r10, [rax + r8]");                                 // path pointer = URL + slash index
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // save path pointer
    emitter.instruction("mov r10, rdx");                                        // URL length
    emitter.instruction("sub r10, r8");                                         // path length = URL length - slash index
    emitter.instruction("cmp r10, 2");                                          // path must include at least one byte after '/'
    emitter.instruction("jl __rt_fgc_url_ftp_fail_x86");                        // empty FTP path is invalid
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // save path length
    emitter.instruction("mov QWORD PTR [rbp - 88], r8");                        // save authority end

    emitter.instruction("mov r9, r11");                                         // host start index after the scheme prefix
    emitter.instruction("mov rcx, r11");                                        // userinfo scan index after the scheme prefix
    emitter.label("__rt_fgc_url_ftp_userinfo_scan_x86");
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 88]");                       // reached authority end?
    emitter.instruction("jge __rt_fgc_url_ftp_userinfo_done_x86");              // host start is final
    emitter.instruction("cmp BYTE PTR [rax + rcx], 0x40");                      // '@' userinfo separator?
    emitter.instruction("jne __rt_fgc_url_ftp_userinfo_next_x86");              // not userinfo separator
    emitter.instruction("lea r9, [rcx + 1]");                                   // host starts after the separator
    emitter.label("__rt_fgc_url_ftp_userinfo_next_x86");
    emitter.instruction("inc rcx");                                             // advance authority scan
    emitter.instruction("jmp __rt_fgc_url_ftp_userinfo_scan_x86");              // keep scanning authority
    emitter.label("__rt_fgc_url_ftp_userinfo_done_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 88]");                       // authority end
    emitter.instruction("sub r10, r9");                                         // host length
    emitter.instruction("cmp r10, 0");                                          // empty authority after userinfo?
    emitter.instruction("jle __rt_fgc_url_ftp_fail_x86");                       // empty host is an invalid URL
    emitter.instruction("lea r11, [rax + r9]");                                 // host pointer inside original URL
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // save host pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // save host length

    emitter.instruction("lea r10, [rip + _fgc_url_addr]");                      // TCP address scratch buffer
    emitter.instruction("lea r11, [rip + _ftp_tcp_prefix]");                    // "tcp://" prefix source
    emitter.instruction("xor rcx, rcx");                                        // address write index
    emitter.label("__rt_fgc_url_ftp_tcp_prefix_x86");
    emitter.instruction("cmp rcx, 6");                                          // copied "tcp://" yet?
    emitter.instruction("jge __rt_fgc_url_ftp_tcp_prefix_done_x86");            // prefix copied
    emitter.instruction("mov r8b, BYTE PTR [r11 + rcx]");                       // prefix byte
    emitter.instruction("mov BYTE PTR [r10 + rcx], r8b");                       // store prefix byte
    emitter.instruction("inc rcx");                                             // advance address write index
    emitter.instruction("jmp __rt_fgc_url_ftp_tcp_prefix_x86");                 // copy the next prefix byte
    emitter.label("__rt_fgc_url_ftp_tcp_prefix_done_x86");
    emitter.instruction("mov QWORD PTR [rbp - 96], 0");                         // has explicit port? 0/1
    emitter.instruction("xor r8, r8");                                          // host copy index
    emitter.label("__rt_fgc_url_ftp_host_copy_x86");
    emitter.instruction("cmp r8, QWORD PTR [rbp - 32]");                        // copied all host bytes?
    emitter.instruction("jge __rt_fgc_url_ftp_host_done_x86");                  // host copy complete
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // host pointer
    emitter.instruction("mov r9b, BYTE PTR [r11 + r8]");                        // next host byte
    emitter.instruction("cmp r9b, 0x3a");                                       // ':' means an explicit port is present
    emitter.instruction("jne __rt_fgc_url_ftp_not_port_x86");                   // not a port separator
    emitter.instruction("mov QWORD PTR [rbp - 96], 1");                         // remember explicit port
    emitter.label("__rt_fgc_url_ftp_not_port_x86");
    emitter.instruction("mov BYTE PTR [r10 + rcx], r9b");                       // append host byte to TCP address
    emitter.instruction("inc r8");                                              // advance host copy index
    emitter.instruction("inc rcx");                                             // advance address write index
    emitter.instruction("jmp __rt_fgc_url_ftp_host_copy_x86");                  // copy the next host byte
    emitter.label("__rt_fgc_url_ftp_host_done_x86");
    emitter.instruction("cmp QWORD PTR [rbp - 96], 0");                         // was a port present?
    emitter.instruction("jne __rt_fgc_url_ftp_addr_done_x86");                  // explicit port: do not append default :21
    emitter.instruction("mov BYTE PTR [r10 + rcx], 0x3a");                      // append ':'
    emitter.instruction("inc rcx");                                             // advance address write index
    emitter.instruction("mov BYTE PTR [r10 + rcx], 0x32");                      // append '2'
    emitter.instruction("inc rcx");                                             // advance address write index
    emitter.instruction("mov BYTE PTR [r10 + rcx], 0x31");                      // append '1'
    emitter.instruction("inc rcx");                                             // advance address write index
    emitter.label("__rt_fgc_url_ftp_addr_done_x86");
    emitter.instruction("mov QWORD PTR [rbp - 56], rcx");                       // save TCP address length

    emitter.instruction("lea r10, [rip + _fgc_url_retr]");                      // RETR command scratch buffer
    emitter.instruction("mov BYTE PTR [r10 + 0], 0x52");                        // 'R'
    emitter.instruction("mov BYTE PTR [r10 + 1], 0x45");                        // 'E'
    emitter.instruction("mov BYTE PTR [r10 + 2], 0x54");                        // 'T'
    emitter.instruction("mov BYTE PTR [r10 + 3], 0x52");                        // 'R'
    emitter.instruction("mov BYTE PTR [r10 + 4], 0x20");                        // space after RETR
    emitter.instruction("mov rcx, 5");                                          // RETR write index after prefix
    emitter.instruction("xor r8, r8");                                          // path copy index
    emitter.instruction("mov r11, QWORD PTR [rbp - 40]");                       // path pointer
    emitter.label("__rt_fgc_url_ftp_retr_path_x86");
    emitter.instruction("cmp r8, QWORD PTR [rbp - 48]");                        // copied all path bytes?
    emitter.instruction("jge __rt_fgc_url_ftp_retr_done_path_x86");             // path copied
    emitter.instruction("mov r9b, BYTE PTR [r11 + r8]");                        // next path byte
    emitter.instruction("mov BYTE PTR [r10 + rcx], r9b");                       // append path byte
    emitter.instruction("inc r8");                                              // advance path index
    emitter.instruction("inc rcx");                                             // advance RETR write index
    emitter.instruction("jmp __rt_fgc_url_ftp_retr_path_x86");                  // copy the next path byte
    emitter.label("__rt_fgc_url_ftp_retr_done_path_x86");
    emitter.instruction("mov BYTE PTR [r10 + rcx], 0x0d");                      // append CR
    emitter.instruction("inc rcx");                                             // advance RETR write index
    emitter.instruction("mov BYTE PTR [r10 + rcx], 0x0a");                      // append LF
    emitter.instruction("inc rcx");                                             // RETR command length

    emitter.instruction("lea rdi, [rip + _fgc_url_addr]");                      // TCP control address pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // TCP control address length
    emitter.instruction("lea rdx, [rip + _fgc_url_retr]");                      // RETR command pointer
    emitter.instruction("call __rt_ftp_open");                                  // open FTP data fd (rcx already holds command length)
    emitter.instruction("cmp rax, 0");                                          // did the FTP open fail?
    emitter.instruction("jl __rt_fgc_url_ftp_fail_x86");                        // failed open returns PHP false
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // save FTP data fd
    emitter.instruction("mov rdi, rax");                                        // fd for stream_get_contents
    emitter.instruction("call __rt_stream_get_contents");                       // slurp the FTP data fd into concat buffer
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // save response ptr across close
    emitter.instruction("mov QWORD PTR [rbp - 80], rdx");                       // save response length across close
    emitter.instruction("mov rdi, QWORD PTR [rbp - 64]");                       // reload FTP data fd
    emitter.instruction("lea r9, [rip + _tls_sessions]");                       // TLS session handle table
    emitter.instruction("mov r10, QWORD PTR [r9 + rdi * 8]");                   // TLS session attached to this data fd?
    emitter.instruction("test r10, r10");                                       // is the data fd plain?
    emitter.instruction("je __rt_fgc_url_ftp_close_plain_x86");                 // plain FTP data fd: close directly
    emitter.instruction("mov rdi, r10");                                        // TLS handle as close helper argument
    emitter.instruction("mov r9, QWORD PTR [rip + _elephc_tls_close_fn]");      // elephc_tls_close entry pointer
    emitter.instruction("call r9");                                             // send close_notify and drop the TLS session
    emitter.instruction("mov rdi, QWORD PTR [rbp - 64]");                       // reload FTP data fd after TLS close
    emitter.instruction("lea r9, [rip + _tls_sessions]");                       // TLS session handle table
    emitter.instruction("mov QWORD PTR [r9 + rdi * 8], 0");                     // clear the TLS session slot for descriptor reuse
    emitter.label("__rt_fgc_url_ftp_close_plain_x86");
    emitter.instruction("call close");                                          // close the data connection
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // restore response ptr
    emitter.instruction("mov rdx, QWORD PTR [rbp - 80]");                       // restore response length
    emitter.instruction("call __rt_str_persist");                               // persist the response string for file_get_contents ownership
    emitter.instruction("add rsp, 112");                                        // release helper locals
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return owned FTP response string

    emitter.label("__rt_fgc_url_ftp_fail_x86");
    emitter.instruction("mov QWORD PTR [rip + _ftp_use_tls], 0");               // clear the one-shot AUTH-TLS flag after any FTP failure
    emitter.instruction("xor eax, eax");                                        // null string ptr → PHP false
    emitter.instruction("xor edx, edx");                                        // zero failure length
    emitter.instruction("add rsp, 112");                                        // release helper locals
    emitter.instruction("pop rbp");                                             // restore caller frame pointer
    emitter.instruction("ret");                                                 // return failure
    emitter.label("__rt_fgc_url_plain_x86");
    emitter.instruction("jmp __rt_file_get_contents_maybe_phar");               // fallback to phar:// runtime reader or filesystem path
}
