//! Purpose:
//! Emits the `ftp://` wrapper runtime: `__rt_ftp_send_recv`,
//! `__rt_ftp_parse_pasv` and `__rt_ftp_open`.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::io`.
//! - `__rt_ftp_open` is the entry point invoked by the `ftp://` `fopen` lowering.
//!
//! Key details:
//! - v1 performs an anonymous, binary (`TYPE I`), passive-mode read: connect the
//!   control socket, drain the greeting, send `USER`/`PASS`/`TYPE`/`PASV`, parse
//!   the `227 (...)` reply for the data address, connect the data socket and
//!   send `RETR`. The data descriptor is returned as the readable stream.
//! - The control connection is left open for the lifetime of the process; it is
//!   reclaimed at exit. FTP responses are read with a single `read` per step.
//! - Reuses `__rt_stream_socket_client` for both connections and `__rt_itoa` for
//!   the passive-mode port.

use crate::codegen::{abi, emit::Emitter, platform::Arch};

const USER_CMD_LEN: i64 = 16; // "USER anonymous\r\n"
const PASS_CMD_LEN: i64 = 17; // "PASS anonymous@\r\n"
const TYPE_CMD_LEN: i64 = 8; //  "TYPE I\r\n"
const PASV_CMD_LEN: i64 = 6; //  "PASV\r\n"

/// Emits the three FTP runtime helpers.
pub fn emit_ftp(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_ftp_linux_x86_64(emitter);
        return;
    }

    // ================================================================
    // __rt_ftp_send_recv: send a command and read the server reply.
    // Input: x0 = fd, x1 = command pointer, x2 = command length.
    // Dispatches both the write and the read through elephc-tls when
    // _tls_sessions[fd] holds a non-zero handle (ftps:// after AUTH TLS).
    // ================================================================
    emitter.blank();
    emitter.comment("--- runtime: ftp_send_recv ---");
    emitter.label_global("__rt_ftp_send_recv");
    emitter.instruction("sub sp, sp, #32");                                     // frame for descriptor + saved x29/x30
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address (blr below clobbers x30)
    emitter.instruction("add x29, sp, #16");                                    // establish helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the control descriptor

    // -- write phase: TLS-aware dispatch --
    abi::emit_symbol_address(emitter, "x13", "_tls_sessions");
    emitter.instruction("ldr x14, [x13, x0, lsl #3]");                          // _tls_sessions[fd] handle (0 = plain TCP)
    emitter.instruction("cbz x14, __rt_ftp_sr_plain_write");                    // no TLS → plain write syscall
    emitter.instruction("mov x0, x14");                                         // TLS handle as first arg (x1/x2 still hold cmd ptr/len)
    abi::emit_symbol_address(emitter, "x9", "_elephc_tls_write_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load the elephc_tls_write entry pointer
    emitter.instruction("blr x9");                                              // send the command through TLS
    emitter.instruction("b __rt_ftp_sr_read_phase");                            // continue at target label
    emitter.label("__rt_ftp_sr_plain_write");
    emitter.syscall(4);                                                         // plain write syscall

    // -- read phase: TLS-aware dispatch into _ftp_resp_buf --
    emitter.label("__rt_ftp_sr_read_phase");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload control fd
    abi::emit_symbol_address(emitter, "x13", "_tls_sessions");
    emitter.instruction("ldr x14, [x13, x0, lsl #3]");                          // load runtime value
    emitter.instruction("cbz x14, __rt_ftp_sr_plain_read");                     // no TLS → plain read syscall
    emitter.instruction("mov x0, x14");                                         // TLS handle as first arg
    abi::emit_symbol_address(emitter, "x1", "_ftp_resp_buf");
    emitter.instruction("mov x2, #4096");                                       // response buffer capacity
    abi::emit_symbol_address(emitter, "x9", "_elephc_tls_read_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load the elephc_tls_read entry pointer
    emitter.instruction("blr x9");                                              // read the reply through TLS
    emitter.instruction("b __rt_ftp_sr_done");                                  // continue at target label
    emitter.label("__rt_ftp_sr_plain_read");
    abi::emit_symbol_address(emitter, "x1", "_ftp_resp_buf");
    emitter.instruction("mov x2, #4096");                                       // response buffer capacity
    emitter.syscall(3);                                                         // plain read syscall

    emitter.label("__rt_ftp_sr_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the frame
    emitter.instruction("ret");                                                 // the reply now sits in _ftp_resp_buf

    // ================================================================
    // __rt_ftp_parse_pasv: parse a 227 reply into _ftp_data_addr.
    // Output: x0 = data-address length, or -1 when no `(...)` is found.
    // ================================================================
    emitter.blank();
    emitter.comment("--- runtime: ftp_parse_pasv ---");
    emitter.label_global("__rt_ftp_parse_pasv");
    emitter.instruction("sub sp, sp, #32");                                     // frame: [0]=addr base [8]=write index
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish the helper frame pointer

    abi::emit_symbol_address(emitter, "x0", "_ftp_resp_buf");
    emitter.instruction("mov x1, #0");                                          // reply scan index
    emitter.label("__rt_ftp_pp_find");
    emitter.instruction("cmp x1, #512");                                        // scanned the whole short reply?
    emitter.instruction("b.ge __rt_ftp_pp_fail");                               // no passive-mode tuple found
    emitter.instruction("ldrb w2, [x0, x1]");                                   // load a reply byte
    emitter.instruction("add x1, x1, #1");                                      // advance past it
    emitter.instruction("cmp w2, #40");                                         // is it '('?
    emitter.instruction("b.ne __rt_ftp_pp_find");                               // keep scanning for the tuple

    abi::emit_symbol_address(emitter, "x3", "_ftp_data_addr");
    abi::emit_symbol_address(emitter, "x4", "_ftp_tcp_prefix");
    emitter.instruction("mov x5, #0");                                          // data-address write index
    emitter.label("__rt_ftp_pp_pfx");
    emitter.instruction("cmp x5, #6");                                          // copied the whole \"tcp://\" prefix?
    emitter.instruction("b.ge __rt_ftp_pp_pfx_done");                           // prefix copied
    emitter.instruction("ldrb w6, [x4, x5]");                                   // load a prefix byte
    emitter.instruction("strb w6, [x3, x5]");                                   // store it into the data address
    emitter.instruction("add x5, x5, #1");                                      // advance the write index
    emitter.instruction("b __rt_ftp_pp_pfx");                                   // continue copying the prefix
    emitter.label("__rt_ftp_pp_pfx_done");

    emitter.instruction("mov x7, #0");                                          // comma counter
    emitter.label("__rt_ftp_pp_ip");
    emitter.instruction("cmp x5, #32");                                         // is the bounded data-address buffer near full?
    emitter.instruction("b.ge __rt_ftp_pp_fail");                               // a malformed tuple must not overflow the buffer
    emitter.instruction("ldrb w6, [x0, x1]");                                   // load a tuple byte
    emitter.instruction("add x1, x1, #1");                                      // advance the scan index
    emitter.instruction("cmp w6, #44");                                         // is it a ','?
    emitter.instruction("b.ne __rt_ftp_pp_ip_digit");                           // otherwise it is an address digit
    emitter.instruction("add x7, x7, #1");                                      // count the comma
    emitter.instruction("cmp x7, #4");                                          // reached the host/port boundary?
    emitter.instruction("b.ge __rt_ftp_pp_p1");                                 // the IPv4 octets are complete
    emitter.instruction("mov w6, #46");                                         // octet separator '.'
    emitter.instruction("strb w6, [x3, x5]");                                   // write the '.' into the data address
    emitter.instruction("add x5, x5, #1");                                      // advance the write index
    emitter.instruction("b __rt_ftp_pp_ip");                                    // continue copying octets
    emitter.label("__rt_ftp_pp_ip_digit");
    emitter.instruction("strb w6, [x3, x5]");                                   // copy the address digit
    emitter.instruction("add x5, x5, #1");                                      // advance the write index
    emitter.instruction("b __rt_ftp_pp_ip");                                    // continue copying octets

    emitter.label("__rt_ftp_pp_p1");
    emitter.instruction("mov x8, #0");                                          // first port byte accumulator
    emitter.label("__rt_ftp_pp_p1_loop");
    emitter.instruction("ldrb w6, [x0, x1]");                                   // load a port-byte digit
    emitter.instruction("add x1, x1, #1");                                      // advance the scan index
    emitter.instruction("cmp w6, #48");                                         // below ASCII '0'?
    emitter.instruction("b.lt __rt_ftp_pp_p1_done");                            // the first port byte is complete
    emitter.instruction("cmp w6, #57");                                         // above ASCII '9'?
    emitter.instruction("b.gt __rt_ftp_pp_p1_done");                            // the first port byte is complete
    emitter.instruction("sub w6, w6, #48");                                     // digit value
    emitter.instruction("mov x9, #10");                                         // decimal base
    emitter.instruction("mul x8, x8, x9");                                      // shift one decimal place
    emitter.instruction("add x8, x8, x6");                                      // add the new digit
    emitter.instruction("b __rt_ftp_pp_p1_loop");                               // continue parsing the first port byte
    emitter.label("__rt_ftp_pp_p1_done");

    emitter.instruction("mov x10, #0");                                         // second port byte accumulator
    emitter.label("__rt_ftp_pp_p2_loop");
    emitter.instruction("ldrb w6, [x0, x1]");                                   // load a port-byte digit
    emitter.instruction("add x1, x1, #1");                                      // advance the scan index
    emitter.instruction("cmp w6, #48");                                         // below ASCII '0'?
    emitter.instruction("b.lt __rt_ftp_pp_p2_done");                            // the second port byte is complete
    emitter.instruction("cmp w6, #57");                                         // above ASCII '9'?
    emitter.instruction("b.gt __rt_ftp_pp_p2_done");                            // the second port byte is complete
    emitter.instruction("sub w6, w6, #48");                                     // digit value
    emitter.instruction("mov x9, #10");                                         // decimal base
    emitter.instruction("mul x10, x10, x9");                                    // shift one decimal place
    emitter.instruction("add x10, x10, x6");                                    // add the new digit
    emitter.instruction("b __rt_ftp_pp_p2_loop");                               // continue parsing the second port byte
    emitter.label("__rt_ftp_pp_p2_done");

    emitter.instruction("lsl x8, x8, #8");                                      // port = first byte * 256
    emitter.instruction("add x8, x8, x10");                                     // port += second byte
    emitter.instruction("mov w6, #58");                                         // address/port separator ':'
    emitter.instruction("strb w6, [x3, x5]");                                   // write the ':' into the data address
    emitter.instruction("add x5, x5, #1");                                      // advance the write index
    emitter.instruction("str x3, [sp, #0]");                                    // save the data-address base across __rt_itoa
    emitter.instruction("str x5, [sp, #8]");                                    // save the write index across __rt_itoa
    emitter.instruction("mov x0, x8");                                          // port value into the __rt_itoa argument
    emitter.instruction("bl __rt_itoa");                                        // x1 = port digits, x2 = digit count
    emitter.instruction("ldr x3, [sp, #0]");                                    // reload the data-address base
    emitter.instruction("ldr x5, [sp, #8]");                                    // reload the write index
    emitter.instruction("mov x6, #0");                                          // port-digit copy index
    emitter.label("__rt_ftp_pp_port_copy");
    emitter.instruction("cmp x6, x2");                                          // copied every port digit?
    emitter.instruction("b.ge __rt_ftp_pp_done");                               // the data address is complete
    emitter.instruction("ldrb w7, [x1, x6]");                                   // load a port digit
    emitter.instruction("strb w7, [x3, x5]");                                   // append it to the data address
    emitter.instruction("add x5, x5, #1");                                      // advance the write index
    emitter.instruction("add x6, x6, #1");                                      // advance the copy index
    emitter.instruction("b __rt_ftp_pp_port_copy");                             // continue copying port digits
    emitter.label("__rt_ftp_pp_done");
    emitter.instruction("mov x0, x5");                                          // return the data-address length
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the frame
    emitter.instruction("ret");                                                 // return the parsed data-address length

    emitter.label("__rt_ftp_pp_fail");
    emitter.instruction("mov x0, #-1");                                         // -1 signals an unparseable reply
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the frame
    emitter.instruction("ret");                                                 // return the failure result

    // ================================================================
    // __rt_ftp_open: run the FTP handshake and return the data fd.
    // Input: x0/x1 = control address, x2/x3 = RETR command.
    // ================================================================
    emitter.blank();
    emitter.comment("--- runtime: ftp_open ---");
    emitter.label_global("__rt_ftp_open");
    emitter.instruction("sub sp, sp, #48");                                     // frame: [0]ctrl fd [8]retr ptr [16]retr len [24]data fd
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the helper frame pointer
    emitter.instruction("str x2, [sp, #8]");                                    // save the RETR command pointer
    emitter.instruction("str x3, [sp, #16]");                                   // save the RETR command length

    emitter.instruction("bl __rt_stream_socket_client");                        // connect the control socket, x0 = fd
    emitter.instruction("cmp x0, #0");                                          // did the control connection fail?
    emitter.instruction("b.lt __rt_ftp_open_fail");                             // propagate the failure
    emitter.instruction("str x0, [sp, #0]");                                    // save the control descriptor
    abi::emit_symbol_address(emitter, "x1", "_ftp_resp_buf");
    emitter.instruction("mov x2, #4096");                                       // response buffer capacity
    emitter.syscall(3);                                                         // drain the 220 greeting (pre-TLS, plain syscall)

    // -- ftps://: AUTH TLS handshake + control-channel TLS attach --
    abi::emit_symbol_address(emitter, "x9", "_ftp_use_tls");
    emitter.instruction("ldr x10, [x9]");                                       // load runtime value
    emitter.instruction("cbz x10, __rt_ftp_open_skip_auth_tls");                // plain ftp:// → skip the TLS upgrade
    // Send "AUTH TLS\r\n" via plain syscall (control is still cleartext).
    emitter.instruction("ldr x0, [sp, #0]");                                    // control fd
    abi::emit_symbol_address(emitter, "x1", "_ftp_auth_tls_cmd");
    emitter.instruction("mov x2, #10");                                         // strlen("AUTH TLS\\r\\n")
    emitter.syscall(4);                                                         // write
    emitter.instruction("ldr x0, [sp, #0]");                                    // load runtime value
    abi::emit_symbol_address(emitter, "x1", "_ftp_resp_buf");
    emitter.instruction("mov x2, #4096");                                       // prepare AArch64 call argument
    emitter.syscall(3);                                                         // read AUTH TLS reply (expect 234)
    // Promote the control fd to TLS via elephc_tls_attach_fd; store the
    // returned handle in _tls_sessions[fd] so __rt_ftp_send_recv routes
    // subsequent USER/PASS/PBSZ/PROT/PASV through elephc-tls.
    emitter.instruction("ldr x0, [sp, #0]");                                    // fd → first arg
    abi::emit_symbol_address(emitter, "x9", "_elephc_tls_attach_fd_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load runtime value
    emitter.instruction("cbz x9, __rt_ftp_open_fail");                          // missing elephc-tls runtime means FTPS open fails closed
    emitter.instruction("blr x9");                                              // x0 = TLS handle or -1
    emitter.instruction("cmp x0, #0");                                          // compare runtime values for the next branch
    emitter.instruction("b.lt __rt_ftp_open_fail");                             // TLS handshake failed
    emitter.instruction("ldr x10, [sp, #0]");                                   // fd
    abi::emit_symbol_address(emitter, "x11", "_tls_sessions");
    emitter.instruction("str x0, [x11, x10, lsl #3]");                          // _tls_sessions[fd] = handle
    emitter.label("__rt_ftp_open_skip_auth_tls");

    emitter.instruction("ldr x0, [sp, #0]");                                    // control descriptor for the USER command
    abi::emit_symbol_address(emitter, "x1", "_ftp_user_cmd");
    emitter.instruction(&format!("mov x2, #{}", USER_CMD_LEN));                 // USER command length
    emitter.instruction("bl __rt_ftp_send_recv");                               // log in anonymously

    emitter.instruction("ldr x0, [sp, #0]");                                    // control descriptor for the PASS command
    abi::emit_symbol_address(emitter, "x1", "_ftp_pass_cmd");
    emitter.instruction(&format!("mov x2, #{}", PASS_CMD_LEN));                 // PASS command length
    emitter.instruction("bl __rt_ftp_send_recv");                               // send the anonymous password

    emitter.instruction("ldr x0, [sp, #0]");                                    // control descriptor for the TYPE command
    abi::emit_symbol_address(emitter, "x1", "_ftp_type_cmd");
    emitter.instruction(&format!("mov x2, #{}", TYPE_CMD_LEN));                 // TYPE command length
    emitter.instruction("bl __rt_ftp_send_recv");                               // switch to binary transfers

    // -- ftps://: PBSZ 0 + PROT P to enable encrypted data channel --
    abi::emit_symbol_address(emitter, "x9", "_ftp_use_tls");
    emitter.instruction("ldr x10, [x9]");                                       // load runtime value
    emitter.instruction("cbz x10, __rt_ftp_open_skip_prot");                    // plain ftp:// → no PROT
    emitter.instruction("ldr x0, [sp, #0]");                                    // load runtime value
    abi::emit_symbol_address(emitter, "x1", "_ftp_pbsz_cmd");
    emitter.instruction("mov x2, #8");                                          // strlen("PBSZ 0\\r\\n")
    emitter.instruction("bl __rt_ftp_send_recv");                               // negotiate protection buffer size = 0
    emitter.instruction("ldr x0, [sp, #0]");                                    // load runtime value
    abi::emit_symbol_address(emitter, "x1", "_ftp_prot_p_cmd");
    emitter.instruction("mov x2, #8");                                          // strlen("PROT P\\r\\n")
    emitter.instruction("bl __rt_ftp_send_recv");                               // request private (encrypted) data channel
    emitter.label("__rt_ftp_open_skip_prot");

    emitter.instruction("ldr x0, [sp, #0]");                                    // control descriptor for the PASV command
    abi::emit_symbol_address(emitter, "x1", "_ftp_pasv_cmd");
    emitter.instruction(&format!("mov x2, #{}", PASV_CMD_LEN));                 // PASV command length
    emitter.instruction("bl __rt_ftp_send_recv");                               // request a passive-mode data port

    emitter.instruction("bl __rt_ftp_parse_pasv");                              // x0 = data-address length or -1
    emitter.instruction("cmp x0, #0");                                          // did the passive-mode reply parse?
    emitter.instruction("b.lt __rt_ftp_open_fail");                             // propagate the failure
    emitter.instruction("mov x1, x0");                                          // data-address length into argument 1
    abi::emit_symbol_address(emitter, "x0", "_ftp_data_addr");
    emitter.instruction("bl __rt_stream_socket_client");                        // connect the data socket, x0 = fd
    emitter.instruction("cmp x0, #0");                                          // did the data connection fail?
    emitter.instruction("b.lt __rt_ftp_open_fail");                             // propagate the failure
    emitter.instruction("str x0, [sp, #24]");                                   // save the data descriptor

    // -- ftps://: TLS-wrap the data fd so fread() routes through TLS --
    abi::emit_symbol_address(emitter, "x9", "_ftp_use_tls");
    emitter.instruction("ldr x10, [x9]");                                       // load runtime value
    emitter.instruction("cbz x10, __rt_ftp_open_skip_data_tls");                // plain ftp:// → no data-channel TLS
    emitter.instruction("ldr x0, [sp, #24]");                                   // data fd → first arg
    abi::emit_symbol_address(emitter, "x9", "_elephc_tls_attach_fd_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load runtime value
    emitter.instruction("cbz x9, __rt_ftp_open_fail");                          // missing elephc-tls runtime means FTPS open fails closed
    emitter.instruction("blr x9");                                              // x0 = TLS handle or -1
    emitter.instruction("cmp x0, #0");                                          // compare runtime values for the next branch
    emitter.instruction("b.lt __rt_ftp_open_fail");                             // data-channel TLS handshake failed
    emitter.instruction("ldr x10, [sp, #24]");                                  // data fd
    abi::emit_symbol_address(emitter, "x11", "_tls_sessions");
    emitter.instruction("str x0, [x11, x10, lsl #3]");                          // _tls_sessions[data_fd] = handle
    emitter.label("__rt_ftp_open_skip_data_tls");

    // -- optional REST <N>\r\n send when stream_context_options['ftp']['resume_pos'] is set --
    // The value is read as a string (consistent with stream_context_set_option's v1
    // string-tag limit). The byte-copy reuses the existing
    // __rt_http_build_copy_aarch64 helper since it's a generic
    // (dest=x9, src=x10, len=x11) memcpy that returns the advanced dest in x9.
    emitter.instruction("sub sp, sp, #16");                                     // temporary spill for (ptr, len) output
    emitter.instruction("str xzr, [sp, #0]");                                   // resume_pos_ptr default null
    emitter.instruction("str xzr, [sp, #8]");                                   // resume_pos_len default 0
    abi::emit_symbol_address(emitter, "x0", "_ftp_key_str");
    emitter.instruction("mov x1, #3");                                          // strlen("ftp") = 3
    abi::emit_symbol_address(emitter, "x2", "_ftp_resume_pos_key_str");
    emitter.instruction("mov x3, #10");                                         // strlen("resume_pos") = 10
    emitter.instruction("add x4, sp, #0");                                      // out_ptr_addr
    emitter.instruction("add x5, sp, #8");                                      // out_len_addr
    emitter.instruction("bl __rt_get_string_context_option");                   // x0 = 1 hit / 0 miss
    emitter.instruction("cbz x0, __rt_ftp_open_skip_rest");                     // branch when the checked value is zero or equal
    // Build "REST <N>\r\n" in _ftp_cmd_scratch.
    abi::emit_symbol_address(emitter, "x9", "_ftp_cmd_scratch");
    abi::emit_symbol_address(emitter, "x10", "_ftp_rest_prefix");
    emitter.instruction("mov x11, #5");                                         // strlen("REST ") = 5
    emitter.instruction("bl __rt_http_build_copy_aarch64");                     // x9 += 5
    emitter.instruction("ldr x10, [sp, #0]");                                   // resume_pos_ptr
    emitter.instruction("ldr x11, [sp, #8]");                                   // resume_pos_len
    emitter.instruction("bl __rt_http_build_copy_aarch64");                     // x9 += resume_pos_len
    emitter.instruction("mov w13, #13");                                        // ASCII '\r'
    emitter.instruction("strb w13, [x9]");                                      // store runtime value
    emitter.instruction("mov w13, #10");                                        // ASCII '\n'
    emitter.instruction("strb w13, [x9, #1]");                                  // store runtime value
    emitter.instruction("add x9, x9, #2");                                      // advance past CRLF
    abi::emit_symbol_address(emitter, "x10", "_ftp_cmd_scratch");
    emitter.instruction("sub x2, x9, x10");                                     // total length = end - base
    emitter.instruction("ldr x0, [sp, #16]");                                   // ctrl fd (lives at original [sp+0], now +16 due to spill)
    emitter.instruction("mov x1, x10");                                         // command pointer = scratch base
    emitter.instruction("bl __rt_ftp_send_recv");                               // send REST <N>\r\n and read the reply
    emitter.label("__rt_ftp_open_skip_rest");
    emitter.instruction("add sp, sp, #16");                                     // release the resume_pos spill

    emitter.instruction("ldr x0, [sp, #0]");                                    // control descriptor for the RETR command
    emitter.instruction("ldr x1, [sp, #8]");                                    // RETR command pointer
    emitter.instruction("ldr x2, [sp, #16]");                                   // RETR command length
    emitter.instruction("bl __rt_ftp_send_recv");                               // start the file transfer

    // Reset _ftp_use_tls so a subsequent plain ftp:// open is not contaminated.
    abi::emit_symbol_address(emitter, "x9", "_ftp_use_tls");
    emitter.instruction("str xzr, [x9]");                                       // store runtime value
    emitter.instruction("ldr x0, [sp, #24]");                                   // return the readable data descriptor
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the frame
    emitter.instruction("ret");                                                 // return the ftp:// stream descriptor

    emitter.label("__rt_ftp_open_fail");
    abi::emit_symbol_address(emitter, "x9", "_ftp_use_tls");
    emitter.instruction("str xzr, [x9]");                                       // clear the one-shot AUTH-TLS flag after any open failure
    emitter.instruction("mov x0, #-1");                                         // -1 signals a failed ftp:// open
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the frame
    emitter.instruction("ret");                                                 // return the failure result
}

/// Emits the Linux x86_64 stream runtime helper for ftp.
fn emit_ftp_linux_x86_64(emitter: &mut Emitter) {
    // ================================================================
    // __rt_ftp_send_recv: send a command and read the server reply.
    // ================================================================
    emitter.blank();
    emitter.comment("--- runtime: ftp_send_recv ---");
    emitter.label_global("__rt_ftp_send_recv");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 32");                                         // frame for the saved descriptor + alignment
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the control descriptor
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save command pointer (call write may clobber rsi)
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save command length (call write may clobber rdx)

    // -- write phase: TLS-aware dispatch --
    emitter.instruction("lea r10, [rip + _tls_sessions]");                      // load runtime data address
    emitter.instruction("mov r11, QWORD PTR [r10 + rdi * 8]");                  // _tls_sessions[fd] handle (0 = plain TCP)
    emitter.instruction("test r11, r11");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_ftp_sr_plain_write_x");                        // branch when the checked value is zero or equal
    emitter.instruction("mov rdi, r11");                                        // TLS handle as first arg
    emitter.instruction("mov r9, QWORD PTR [rip + _elephc_tls_write_fn]");      // prepare SysV call argument
    emitter.instruction("call r9");                                             // send the command through TLS
    emitter.instruction("jmp __rt_ftp_sr_read_phase_x");                        // continue at target label
    emitter.label("__rt_ftp_sr_plain_write_x");
    emitter.instruction("call write");                                          // send the command on the control socket

    // -- read phase: TLS-aware dispatch into _ftp_resp_buf --
    emitter.label("__rt_ftp_sr_read_phase_x");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload control fd
    emitter.instruction("lea r10, [rip + _tls_sessions]");                      // load runtime data address
    emitter.instruction("mov r11, QWORD PTR [r10 + rdi * 8]");                  // _tls_sessions[fd] handle
    emitter.instruction("test r11, r11");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_ftp_sr_plain_read_x");                         // branch when the checked value is zero or equal
    emitter.instruction("mov rdi, r11");                                        // TLS handle
    emitter.instruction("lea rsi, [rip + _ftp_resp_buf]");                      // load runtime data address
    emitter.instruction("mov rdx, 4096");                                       // prepare SysV call argument
    emitter.instruction("mov r9, QWORD PTR [rip + _elephc_tls_read_fn]");       // prepare SysV call argument
    emitter.instruction("call r9");                                             // read the reply through TLS
    emitter.instruction("jmp __rt_ftp_sr_done_x");                              // continue at target label
    emitter.label("__rt_ftp_sr_plain_read_x");
    emitter.instruction("lea rsi, [rip + _ftp_resp_buf]");                      // response buffer pointer
    emitter.instruction("mov rdx, 4096");                                       // response buffer capacity
    emitter.instruction("call read");                                           // read the server reply

    emitter.label("__rt_ftp_sr_done_x");
    emitter.instruction("add rsp, 32");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // the reply now sits in _ftp_resp_buf

    // ================================================================
    // __rt_ftp_parse_pasv: parse a 227 reply into _ftp_data_addr.
    // ================================================================
    emitter.blank();
    emitter.comment("--- runtime: ftp_parse_pasv ---");
    emitter.label_global("__rt_ftp_parse_pasv");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 32");                                         // frame for state saved across __rt_itoa

    emitter.instruction("lea rsi, [rip + _ftp_resp_buf]");                      // reply buffer base
    emitter.instruction("xor rcx, rcx");                                        // reply scan index
    emitter.label("__rt_ftp_pp_find_x86");
    emitter.instruction("cmp rcx, 512");                                        // scanned the whole short reply?
    emitter.instruction("jge __rt_ftp_pp_fail_x86");                            // no passive-mode tuple found
    emitter.instruction("movzx eax, BYTE PTR [rsi + rcx]");                     // load a reply byte
    emitter.instruction("inc rcx");                                             // advance past it
    emitter.instruction("cmp al, 40");                                          // is it '('?
    emitter.instruction("jne __rt_ftp_pp_find_x86");                            // keep scanning for the tuple

    emitter.instruction("lea rdi, [rip + _ftp_data_addr]");                     // data-address buffer base
    emitter.instruction("lea r8, [rip + _ftp_tcp_prefix]");                     // \"tcp://\" prefix base
    emitter.instruction("xor r9, r9");                                          // data-address write index
    emitter.label("__rt_ftp_pp_pfx_x86");
    emitter.instruction("cmp r9, 6");                                           // copied the whole \"tcp://\" prefix?
    emitter.instruction("jge __rt_ftp_pp_pfx_done_x86");                        // prefix copied
    emitter.instruction("movzx eax, BYTE PTR [r8 + r9]");                       // load a prefix byte
    emitter.instruction("mov BYTE PTR [rdi + r9], al");                         // store it into the data address
    emitter.instruction("inc r9");                                              // advance the write index
    emitter.instruction("jmp __rt_ftp_pp_pfx_x86");                             // continue copying the prefix
    emitter.label("__rt_ftp_pp_pfx_done_x86");

    emitter.instruction("xor r10, r10");                                        // comma counter
    emitter.label("__rt_ftp_pp_ip_x86");
    emitter.instruction("cmp r9, 32");                                          // is the bounded data-address buffer near full?
    emitter.instruction("jge __rt_ftp_pp_fail_x86");                            // a malformed tuple must not overflow the buffer
    emitter.instruction("movzx eax, BYTE PTR [rsi + rcx]");                     // load a tuple byte
    emitter.instruction("inc rcx");                                             // advance the scan index
    emitter.instruction("cmp al, 44");                                          // is it a ','?
    emitter.instruction("jne __rt_ftp_pp_ip_digit_x86");                        // otherwise it is an address digit
    emitter.instruction("inc r10");                                             // count the comma
    emitter.instruction("cmp r10, 4");                                          // reached the host/port boundary?
    emitter.instruction("jge __rt_ftp_pp_p1_x86");                              // the IPv4 octets are complete
    emitter.instruction("mov BYTE PTR [rdi + r9], 46");                         // write the octet separator '.'
    emitter.instruction("inc r9");                                              // advance the write index
    emitter.instruction("jmp __rt_ftp_pp_ip_x86");                              // continue copying octets
    emitter.label("__rt_ftp_pp_ip_digit_x86");
    emitter.instruction("mov BYTE PTR [rdi + r9], al");                         // copy the address digit
    emitter.instruction("inc r9");                                              // advance the write index
    emitter.instruction("jmp __rt_ftp_pp_ip_x86");                              // continue copying octets

    emitter.label("__rt_ftp_pp_p1_x86");
    emitter.instruction("xor r8, r8");                                          // first port byte accumulator
    emitter.label("__rt_ftp_pp_p1_loop_x86");
    emitter.instruction("movzx eax, BYTE PTR [rsi + rcx]");                     // load a port-byte digit
    emitter.instruction("inc rcx");                                             // advance the scan index
    emitter.instruction("cmp eax, 48");                                         // below ASCII '0'?
    emitter.instruction("jl __rt_ftp_pp_p1_done_x86");                          // the first port byte is complete
    emitter.instruction("cmp eax, 57");                                         // above ASCII '9'?
    emitter.instruction("jg __rt_ftp_pp_p1_done_x86");                          // the first port byte is complete
    emitter.instruction("sub eax, 48");                                         // digit value
    emitter.instruction("imul r8, r8, 10");                                     // shift one decimal place
    emitter.instruction("add r8, rax");                                         // add the new digit
    emitter.instruction("jmp __rt_ftp_pp_p1_loop_x86");                         // continue parsing the first port byte
    emitter.label("__rt_ftp_pp_p1_done_x86");

    emitter.instruction("xor r11, r11");                                        // second port byte accumulator
    emitter.label("__rt_ftp_pp_p2_loop_x86");
    emitter.instruction("movzx eax, BYTE PTR [rsi + rcx]");                     // load a port-byte digit
    emitter.instruction("inc rcx");                                             // advance the scan index
    emitter.instruction("cmp eax, 48");                                         // below ASCII '0'?
    emitter.instruction("jl __rt_ftp_pp_p2_done_x86");                          // the second port byte is complete
    emitter.instruction("cmp eax, 57");                                         // above ASCII '9'?
    emitter.instruction("jg __rt_ftp_pp_p2_done_x86");                          // the second port byte is complete
    emitter.instruction("sub eax, 48");                                         // digit value
    emitter.instruction("imul r11, r11, 10");                                   // shift one decimal place
    emitter.instruction("add r11, rax");                                        // add the new digit
    emitter.instruction("jmp __rt_ftp_pp_p2_loop_x86");                         // continue parsing the second port byte
    emitter.label("__rt_ftp_pp_p2_done_x86");

    emitter.instruction("shl r8, 8");                                           // port = first byte * 256
    emitter.instruction("add r8, r11");                                         // port += second byte
    emitter.instruction("mov BYTE PTR [rdi + r9], 58");                         // write the address/port separator ':'
    emitter.instruction("inc r9");                                              // advance the write index
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the data-address base across __rt_itoa
    emitter.instruction("mov QWORD PTR [rbp - 16], r9");                        // save the write index across __rt_itoa
    emitter.instruction("mov rax, r8");                                         // port value into the __rt_itoa argument
    emitter.instruction("call __rt_itoa");                                      // rax = port digits, rdx = digit count
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the data-address base
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload the write index
    emitter.instruction("xor rcx, rcx");                                        // port-digit copy index
    emitter.label("__rt_ftp_pp_port_copy_x86");
    emitter.instruction("cmp rcx, rdx");                                        // copied every port digit?
    emitter.instruction("jge __rt_ftp_pp_done_x86");                            // the data address is complete
    emitter.instruction("movzx r8d, BYTE PTR [rax + rcx]");                     // load a port digit
    emitter.instruction("mov BYTE PTR [rdi + r9], r8b");                        // append it to the data address
    emitter.instruction("inc r9");                                              // advance the write index
    emitter.instruction("inc rcx");                                             // advance the copy index
    emitter.instruction("jmp __rt_ftp_pp_port_copy_x86");                       // continue copying port digits
    emitter.label("__rt_ftp_pp_done_x86");
    emitter.instruction("mov rax, r9");                                         // return the data-address length
    emitter.instruction("add rsp, 32");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the parsed data-address length

    emitter.label("__rt_ftp_pp_fail_x86");
    emitter.instruction("mov rax, -1");                                         // -1 signals an unparseable reply
    emitter.instruction("add rsp, 32");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the failure result

    // ================================================================
    // __rt_ftp_open: run the FTP handshake and return the data fd.
    // ================================================================
    emitter.blank();
    emitter.comment("--- runtime: ftp_open ---");
    emitter.label_global("__rt_ftp_open");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 48");                                         // frame: ctrl fd, RETR cmd, data fd
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the RETR command pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // save the RETR command length

    emitter.instruction("call __rt_stream_socket_client");                      // connect the control socket, rax = fd
    emitter.instruction("cmp rax, 0");                                          // did the control connection fail?
    emitter.instruction("jl __rt_ftp_open_fail_x86");                           // propagate the failure
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the control descriptor
    emitter.instruction("mov rdi, rax");                                        // control descriptor for the greeting read
    emitter.instruction("lea rsi, [rip + _ftp_resp_buf]");                      // response buffer pointer
    emitter.instruction("mov rdx, 4096");                                       // response buffer capacity
    emitter.instruction("call read");                                           // drain the server greeting

    // -- ftps://: AUTH TLS handshake + control-channel TLS attach --
    emitter.instruction("mov rax, QWORD PTR [rip + _ftp_use_tls]");             // prepare runtime result value
    emitter.instruction("test rax, rax");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_ftp_open_skip_auth_tls_x");                    // branch when the checked value is zero or equal
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // ctrl fd
    emitter.instruction("lea rsi, [rip + _ftp_auth_tls_cmd]");                  // load runtime data address
    emitter.instruction("mov rdx, 10");                                         // strlen("AUTH TLS\\r\\n")
    emitter.instruction("call write");                                          // plain write (control still cleartext)
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // prepare SysV call argument
    emitter.instruction("lea rsi, [rip + _ftp_resp_buf]");                      // load runtime data address
    emitter.instruction("mov rdx, 4096");                                       // prepare SysV call argument
    emitter.instruction("call read");                                           // expect 234
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // fd → first arg
    emitter.instruction("mov r9, QWORD PTR [rip + _elephc_tls_attach_fd_fn]");  // prepare SysV call argument
    emitter.instruction("test r9, r9");                                         // missing elephc-tls runtime means FTPS open fails closed
    emitter.instruction("jz __rt_ftp_open_fail_x86");                           // return a failed stream when no TLS entry is available
    emitter.instruction("call r9");                                             // rax = TLS handle or -1
    emitter.instruction("cmp rax, 0");                                          // compare runtime values for the next branch
    emitter.instruction("jl __rt_ftp_open_fail_x86");                           // TLS handshake failed
    emitter.instruction("mov rcx, QWORD PTR [rbp - 8]");                        // fd
    emitter.instruction("lea r10, [rip + _tls_sessions]");                      // load runtime data address
    emitter.instruction("mov QWORD PTR [r10 + rcx * 8], rax");                  // _tls_sessions[fd] = handle
    emitter.label("__rt_ftp_open_skip_auth_tls_x");

    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // control descriptor for the USER command
    emitter.instruction("lea rsi, [rip + _ftp_user_cmd]");                      // USER command pointer
    emitter.instruction(&format!("mov rdx, {}", USER_CMD_LEN));                 // USER command length
    emitter.instruction("call __rt_ftp_send_recv");                             // log in anonymously

    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // control descriptor for the PASS command
    emitter.instruction("lea rsi, [rip + _ftp_pass_cmd]");                      // PASS command pointer
    emitter.instruction(&format!("mov rdx, {}", PASS_CMD_LEN));                 // PASS command length
    emitter.instruction("call __rt_ftp_send_recv");                             // send the anonymous password

    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // control descriptor for the TYPE command
    emitter.instruction("lea rsi, [rip + _ftp_type_cmd]");                      // TYPE command pointer
    emitter.instruction(&format!("mov rdx, {}", TYPE_CMD_LEN));                 // TYPE command length
    emitter.instruction("call __rt_ftp_send_recv");                             // switch to binary transfers

    // -- ftps://: PBSZ 0 + PROT P to enable encrypted data channel --
    emitter.instruction("mov rax, QWORD PTR [rip + _ftp_use_tls]");             // prepare runtime result value
    emitter.instruction("test rax, rax");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_ftp_open_skip_prot_x");                        // branch when the checked value is zero or equal
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // prepare SysV call argument
    emitter.instruction("lea rsi, [rip + _ftp_pbsz_cmd]");                      // load runtime data address
    emitter.instruction("mov rdx, 8");                                          // strlen("PBSZ 0\\r\\n")
    emitter.instruction("call __rt_ftp_send_recv");                             // call runtime helper
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // prepare SysV call argument
    emitter.instruction("lea rsi, [rip + _ftp_prot_p_cmd]");                    // load runtime data address
    emitter.instruction("mov rdx, 8");                                          // strlen("PROT P\\r\\n")
    emitter.instruction("call __rt_ftp_send_recv");                             // call runtime helper
    emitter.label("__rt_ftp_open_skip_prot_x");

    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // control descriptor for the PASV command
    emitter.instruction("lea rsi, [rip + _ftp_pasv_cmd]");                      // PASV command pointer
    emitter.instruction(&format!("mov rdx, {}", PASV_CMD_LEN));                 // PASV command length
    emitter.instruction("call __rt_ftp_send_recv");                             // request a passive-mode data port

    emitter.instruction("call __rt_ftp_parse_pasv");                            // rax = data-address length or -1
    emitter.instruction("cmp rax, 0");                                          // did the passive-mode reply parse?
    emitter.instruction("jl __rt_ftp_open_fail_x86");                           // propagate the failure
    emitter.instruction("mov rsi, rax");                                        // data-address length into argument 1
    emitter.instruction("lea rdi, [rip + _ftp_data_addr]");                     // data-address pointer into argument 0
    emitter.instruction("call __rt_stream_socket_client");                      // connect the data socket, rax = fd
    emitter.instruction("cmp rax, 0");                                          // did the data connection fail?
    emitter.instruction("jl __rt_ftp_open_fail_x86");                           // propagate the failure
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the data descriptor

    // -- ftps://: TLS-wrap the data fd so fread() routes through TLS --
    emitter.instruction("mov rax, QWORD PTR [rip + _ftp_use_tls]");             // prepare runtime result value
    emitter.instruction("test rax, rax");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_ftp_open_skip_data_tls_x");                    // branch when the checked value is zero or equal
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // data fd → first arg
    emitter.instruction("mov r9, QWORD PTR [rip + _elephc_tls_attach_fd_fn]");  // prepare SysV call argument
    emitter.instruction("test r9, r9");                                         // missing elephc-tls runtime means FTPS open fails closed
    emitter.instruction("jz __rt_ftp_open_fail_x86");                           // return a failed stream when no TLS entry is available
    emitter.instruction("call r9");                                             // rax = TLS handle or -1
    emitter.instruction("cmp rax, 0");                                          // compare runtime values for the next branch
    emitter.instruction("jl __rt_ftp_open_fail_x86");                           // data-channel TLS handshake failed
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // data fd
    emitter.instruction("lea r10, [rip + _tls_sessions]");                      // load runtime data address
    emitter.instruction("mov QWORD PTR [r10 + rcx * 8], rax");                  // _tls_sessions[data_fd] = handle
    emitter.label("__rt_ftp_open_skip_data_tls_x");

    // -- optional REST <N>\r\n send when stream_context_options['ftp']['resume_pos'] is set --
    emitter.instruction("sub rsp, 16");                                         // spill space for the (ptr, len) lookup output
    emitter.instruction("mov QWORD PTR [rsp + 0], 0");                          // resume_pos_ptr default null
    emitter.instruction("mov QWORD PTR [rsp + 8], 0");                          // resume_pos_len default 0
    emitter.instruction("lea rdi, [rip + _ftp_key_str]");                       // load runtime data address
    emitter.instruction("mov rsi, 3");                                          // strlen("ftp")
    emitter.instruction("lea rdx, [rip + _ftp_resume_pos_key_str]");            // load runtime data address
    emitter.instruction("mov rcx, 10");                                         // strlen("resume_pos")
    emitter.instruction("lea r8, [rsp + 0]");                                   // out_ptr_addr
    emitter.instruction("lea r9, [rsp + 8]");                                   // out_len_addr
    emitter.instruction("call __rt_get_string_context_option");                 // rax = 1 hit / 0 miss
    emitter.instruction("test rax, rax");                                       // check whether the runtime value is zero
    emitter.instruction("jz __rt_ftp_open_skip_rest_x86");                      // branch when the checked value is zero or equal
    // Build "REST <N>\r\n" in _ftp_cmd_scratch using the generic byte-copy helper.
    emitter.instruction("lea rdi, [rip + _ftp_cmd_scratch]");                   // write ptr
    emitter.instruction("lea rsi, [rip + _ftp_rest_prefix]");                   // src
    emitter.instruction("mov rdx, 5");                                          // strlen("REST ")
    emitter.instruction("call __rt_http_build_copy_x86");                       // rax = advanced write ptr
    emitter.instruction("mov rdi, rax");                                        // next dest
    emitter.instruction("mov rsi, QWORD PTR [rsp + 0]");                        // resume_pos_ptr
    emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");                        // resume_pos_len
    emitter.instruction("call __rt_http_build_copy_x86");                       // rax = advanced write ptr
    emitter.instruction("mov BYTE PTR [rax], 13");                              // '\r'
    emitter.instruction("mov BYTE PTR [rax + 1], 10");                          // '\n'
    emitter.instruction("add rax, 2");                                          // past CRLF
    emitter.instruction("lea r10, [rip + _ftp_cmd_scratch]");                   // base
    emitter.instruction("sub rax, r10");                                        // total length
    emitter.instruction("mov rdx, rax");                                        // length → 3rd arg
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // ctrl fd
    emitter.instruction("lea rsi, [rip + _ftp_cmd_scratch]");                   // command pointer
    emitter.instruction("call __rt_ftp_send_recv");                             // send REST <N>\r\n and read the reply
    emitter.label("__rt_ftp_open_skip_rest_x86");
    emitter.instruction("add rsp, 16");                                         // release the resume_pos spill

    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // control descriptor for the RETR command
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // RETR command pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // RETR command length
    emitter.instruction("call __rt_ftp_send_recv");                             // start the file transfer

    // Reset _ftp_use_tls so a subsequent plain ftp:// open is not contaminated.
    emitter.instruction("mov QWORD PTR [rip + _ftp_use_tls], 0");               // store runtime value
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // return the readable data descriptor
    emitter.instruction("add rsp, 48");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the ftp:// stream descriptor

    emitter.label("__rt_ftp_open_fail_x86");
    emitter.instruction("mov QWORD PTR [rip + _ftp_use_tls], 0");               // clear the one-shot AUTH-TLS flag after any open failure
    emitter.instruction("mov rax, -1");                                         // -1 signals a failed ftp:// open
    emitter.instruction("add rsp, 48");                                         // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the failure result
}
