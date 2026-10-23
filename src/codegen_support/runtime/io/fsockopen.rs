//! Purpose:
//! Emits the `fsockopen` runtime helper `__rt_fsockopen`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - `__rt_fsockopen` is the entry point invoked by the `fsockopen` builtin.
//!
//! Key details:
//! - Assembles a `tcp://host:port` address from the runtime hostname string and
//!   port integer into the `_fsockopen_addr` scratch buffer (`__rt_itoa` formats
//!   the port), then delegates to `__rt_stream_socket_client` to connect.
//! - Reuses the shared `_ftp_tcp_prefix` `"tcp://"` literal.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits the `__rt_fsockopen` runtime helper.
/// Input:  AArch64 x0 = hostname ptr, x1 = hostname len, x2 = port.
///         x86_64  rdi = hostname ptr, rsi = hostname len, rdx = port.
/// Output: a connected TCP descriptor, or -1 on failure.
pub fn emit_fsockopen(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_fsockopen_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: fsockopen ---");
    emitter.label_global("__rt_fsockopen");

    // Frame (64 bytes): [0]=hostname ptr [8]=hostname len [16]=port [24]=tls flag/fd
    // [32]=peer-name ptr [40]=peer-name len.
    emitter.instruction("sub sp, sp, #64");                                     // allocate the helper frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the hostname pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the hostname length
    emitter.instruction("str x2, [sp, #16]");                                   // save the port
    emitter.instruction("str xzr, [sp, #24]");                                  // no TLS session until the scheme asks for one
    emitter.instruction("str x0, [sp, #32]");                                   // peer name for the TLS handshake: the bare host
    emitter.instruction("str x1, [sp, #40]");                                   // and its byte length

    // -- a tls:// or ssl:// scheme wraps the TCP stream in elephc-tls --
    emitter.instruction("ldr x4, [sp, #0]");                                    // hostname pointer
    emitter.instruction("ldr x7, [sp, #8]");                                    // hostname length
    emitter.instruction("cmp x7, #6");                                          // a scheme prefix needs six bytes
    emitter.instruction("b.lt __rt_fsockopen_no_tls");                          // too short for a scheme
    emitter.instruction("ldrb w8, [x4, #0]");                                   // first hostname byte
    emitter.instruction("cmp w8, #115");                                        // 's' selects the ssl:// spelling
    emitter.instruction("b.eq __rt_fsockopen_ssl_scheme");                      // check ssl://
    emitter.instruction("cmp w8, #116");                                        // 't' selects the tls:// spelling
    emitter.instruction("b.ne __rt_fsockopen_no_tls");                          // any other byte leaves the host alone
    emitter.instruction("ldrb w8, [x4, #1]");                                   // 'l'
    emitter.instruction("cmp w8, #108");                                        // tls:// second byte
    emitter.instruction("b.ne __rt_fsockopen_no_tls");                          // not tls://
    emitter.instruction("ldrb w8, [x4, #2]");                                   // 's'
    emitter.instruction("cmp w8, #115");                                        // tls:// third byte
    emitter.instruction("b.ne __rt_fsockopen_no_tls");                          // not tls://
    emitter.instruction("b __rt_fsockopen_scheme_colon");                       // both spellings share the ": //" tail
    emitter.label("__rt_fsockopen_ssl_scheme");
    emitter.instruction("ldrb w8, [x4, #1]");                                   // 's'
    emitter.instruction("cmp w8, #115");                                        // ssl:// second byte
    emitter.instruction("b.ne __rt_fsockopen_no_tls");                          // not ssl://
    emitter.instruction("ldrb w8, [x4, #2]");                                   // 'l'
    emitter.instruction("cmp w8, #108");                                        // ssl:// third byte
    emitter.instruction("b.ne __rt_fsockopen_no_tls");                          // not ssl://
    emitter.label("__rt_fsockopen_scheme_colon");
    emitter.instruction("ldrb w8, [x4, #3]");                                   // ':'
    emitter.instruction("cmp w8, #58");                                         // require the scheme separator
    emitter.instruction("b.ne __rt_fsockopen_no_tls");                          // not a scheme we wrap
    emitter.instruction("ldrb w8, [x4, #4]");                                   // '/'
    emitter.instruction("cmp w8, #47");                                         // first slash
    emitter.instruction("b.ne __rt_fsockopen_no_tls");                          // not a scheme we wrap
    emitter.instruction("ldrb w8, [x4, #5]");                                   // '/'
    emitter.instruction("cmp w8, #47");                                         // second slash
    emitter.instruction("b.ne __rt_fsockopen_no_tls");                          // not a scheme we wrap
    emitter.instruction("mov x9, #1");                                          // remember that the stream must be TLS-wrapped
    emitter.instruction("str x9, [sp, #24]");                                   // publish the TLS flag
    emitter.instruction("add x9, x4, #6");                                      // strip the scheme from the address host
    emitter.instruction("str x9, [sp, #0]");                                    // and from the TCP address
    emitter.instruction("str x9, [sp, #32]");                                   // the peer name never carries a scheme either
    emitter.instruction("sub x9, x7, #6");                                      // remaining hostname length
    emitter.instruction("str x9, [sp, #8]");                                    // publish the stripped length
    emitter.instruction("str x9, [sp, #40]");                                   // and the peer-name length
    emitter.label("__rt_fsockopen_no_tls");

    // -- copy the "tcp://" scheme prefix into the address buffer --
    abi::emit_symbol_address(emitter, "x3", "_fsockopen_addr");
    abi::emit_symbol_address(emitter, "x4", "_ftp_tcp_prefix");
    emitter.instruction("mov x5, #0");                                          // address write index
    emitter.label("__rt_fsockopen_pfx");
    emitter.instruction("cmp x5, #6");                                          // copied the whole \"tcp://\" prefix?
    emitter.instruction("b.ge __rt_fsockopen_pfx_done");                        // prefix copied
    emitter.instruction("ldrb w6, [x4, x5]");                                   // load a prefix byte
    emitter.instruction("strb w6, [x3, x5]");                                   // store it into the address buffer
    emitter.instruction("add x5, x5, #1");                                      // advance the write index
    emitter.instruction("b __rt_fsockopen_pfx");                                // continue copying the prefix
    emitter.label("__rt_fsockopen_pfx_done");

    // -- append the hostname --
    emitter.instruction("ldr x4, [sp, #0]");                                    // hostname pointer
    emitter.instruction("ldr x7, [sp, #8]");                                    // hostname length
    emitter.instruction("mov x6, #0");                                          // hostname read index
    emitter.label("__rt_fsockopen_host");
    emitter.instruction("cmp x6, x7");                                          // copied the whole hostname?
    emitter.instruction("b.ge __rt_fsockopen_host_done");                       // hostname copied
    emitter.instruction("ldrb w8, [x4, x6]");                                   // load a hostname byte
    emitter.instruction("strb w8, [x3, x5]");                                   // append it to the address buffer
    emitter.instruction("add x5, x5, #1");                                      // advance the write index
    emitter.instruction("add x6, x6, #1");                                      // advance the hostname read index
    emitter.instruction("b __rt_fsockopen_host");                               // continue copying the hostname
    emitter.label("__rt_fsockopen_host_done");

    // -- append the ':' host/port separator --
    emitter.instruction("mov w8, #58");                                         // ':' separates the host and port
    emitter.instruction("strb w8, [x3, x5]");                                   // write the separator
    emitter.instruction("add x5, x5, #1");                                      // advance the write index

    // -- format the port with __rt_itoa and append its digits --
    emitter.instruction("ldr x0, [sp, #16]");                                   // port value into the __rt_itoa argument
    emitter.instruction("str x3, [sp, #0]");                                    // save the address base across __rt_itoa
    emitter.instruction("str x5, [sp, #8]");                                    // save the write index across __rt_itoa
    emitter.instruction("bl __rt_itoa");                                        // x1 = digit pointer, x2 = digit count
    emitter.instruction("ldr x3, [sp, #0]");                                    // reload the address base
    emitter.instruction("ldr x5, [sp, #8]");                                    // reload the write index
    emitter.instruction("mov x6, #0");                                          // port-digit copy index
    emitter.label("__rt_fsockopen_port");
    emitter.instruction("cmp x6, x2");                                          // copied every port digit?
    emitter.instruction("b.ge __rt_fsockopen_port_done");                       // the address is complete
    emitter.instruction("ldrb w8, [x1, x6]");                                   // load a port digit
    emitter.instruction("strb w8, [x3, x5]");                                   // append it to the address buffer
    emitter.instruction("add x5, x5, #1");                                      // advance the write index
    emitter.instruction("add x6, x6, #1");                                      // advance the digit copy index
    emitter.instruction("b __rt_fsockopen_port");                               // continue copying port digits
    emitter.label("__rt_fsockopen_port_done");

    // -- connect to the assembled tcp://host:port address --
    abi::emit_symbol_address(emitter, "x0", "_fsockopen_addr");
    emitter.instruction("mov x1, x5");                                          // total assembled address length
    emitter.instruction("bl __rt_stream_socket_client");                        // connect, x0 = fd or -1
    emitter.instruction("cmp x0, #0");                                          // did the TCP connection fail?
    emitter.instruction("b.lt __rt_fsockopen_done");                            // propagate the failure unchanged
    emitter.instruction("ldr x9, [sp, #24]");                                   // is this a TLS stream?
    emitter.instruction("cbz x9, __rt_fsockopen_done");                         // plain tcp:// needs no handshake
    // -- promote the connected descriptor to a TLS session (php's tls:// wrapper) --
    emitter.instruction("str x0, [sp, #24]");                                   // park the descriptor while the handshake runs
    abi::emit_symbol_address(emitter, "x9", "_elephc_tls_attach_fd_fn");         // published elephc-tls entry
    emitter.instruction("ldr x9, [x9]");                                        // load the runtime function pointer
    emitter.instruction("cbz x9, __rt_fsockopen_tls_fail");                     // a missing TLS runtime fails closed
    emitter.instruction("ldr x0, [sp, #24]");                                   // descriptor → first argument
    emitter.instruction("ldr x1, [sp, #32]");                                   // peer name pointer → second argument
    emitter.instruction("ldr x2, [sp, #40]");                                   // peer name length → third argument
    emitter.instruction("blr x9");                                              // complete the handshake, x0 = handle or -1
    emitter.instruction("cmp x0, #0");                                          // did the handshake succeed?
    emitter.instruction("b.lt __rt_fsockopen_tls_fail");                        // a failed handshake reports failure
    emitter.instruction("ldr x10, [sp, #24]");                                  // descriptor whose reads/writes become TLS
    abi::emit_symbol_address(emitter, "x11", "_tls_sessions");                  // fd → session handle table
    emitter.instruction("str x0, [x11, x10, lsl #3]");                          // route __rt_fread/__rt_fwrite through the session
    emitter.instruction("ldr x0, [sp, #24]");                                   // return the descriptor
    emitter.instruction("b __rt_fsockopen_done");                               // finish the helper
    emitter.label("__rt_fsockopen_tls_fail");
    emitter.instruction("ldr x0, [sp, #24]");                                   // descriptor to close
    emitter.syscall(6);                                                         // close(2): the handshake failed
    abi::emit_load_int_immediate(emitter, "x0", -1);                            // report failure, as php's wrapper does
    emitter.label("__rt_fsockopen_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the connected descriptor
}

/// Emits the Linux x86_64 stream runtime helper for fsockopen.
fn emit_fsockopen_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fsockopen ---");
    emitter.label_global("__rt_fsockopen");

    // Frame (rbp-relative): [-8]=hostname ptr [-16]=hostname len [-24]=port
    // [-32]=tls flag/fd [-40]=peer-name ptr [-48]=peer-name len.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 48");                                         // reserve the helper spill slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the hostname pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the hostname length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the port
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // no TLS session until the scheme asks for one
    emitter.instruction("mov QWORD PTR [rbp - 40], rdi");                       // peer name for the TLS handshake: the bare host
    emitter.instruction("mov QWORD PTR [rbp - 48], rsi");                       // and its byte length

    // -- a tls:// or ssl:// scheme wraps the TCP stream in elephc-tls --
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // hostname pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // hostname length
    emitter.instruction("cmp r10, 6");                                          // a scheme prefix needs six bytes
    emitter.instruction("jl __rt_fsockopen_no_tls_x86");                        // too short for a scheme
    emitter.instruction("movzx eax, BYTE PTR [r9]");                            // first hostname byte
    emitter.instruction("cmp al, 115");                                         // 's' selects the ssl:// spelling
    emitter.instruction("je __rt_fsockopen_ssl_scheme_x86");                    // check ssl://
    emitter.instruction("cmp al, 116");                                         // 't' selects the tls:// spelling
    emitter.instruction("jne __rt_fsockopen_no_tls_x86");                       // any other byte leaves the host alone
    emitter.instruction("movzx eax, BYTE PTR [r9 + 1]");                        // 'l'
    emitter.instruction("cmp al, 108");                                         // tls:// second byte
    emitter.instruction("jne __rt_fsockopen_no_tls_x86");                       // not tls://
    emitter.instruction("movzx eax, BYTE PTR [r9 + 2]");                        // 's'
    emitter.instruction("cmp al, 115");                                         // tls:// third byte
    emitter.instruction("jne __rt_fsockopen_no_tls_x86");                       // not tls://
    emitter.instruction("jmp __rt_fsockopen_scheme_colon_x86");                 // both spellings share the "://" tail
    emitter.label("__rt_fsockopen_ssl_scheme_x86");
    emitter.instruction("movzx eax, BYTE PTR [r9 + 1]");                        // 's'
    emitter.instruction("cmp al, 115");                                         // ssl:// second byte
    emitter.instruction("jne __rt_fsockopen_no_tls_x86");                       // not ssl://
    emitter.instruction("movzx eax, BYTE PTR [r9 + 2]");                        // 'l'
    emitter.instruction("cmp al, 108");                                         // ssl:// third byte
    emitter.instruction("jne __rt_fsockopen_no_tls_x86");                       // not ssl://
    emitter.label("__rt_fsockopen_scheme_colon_x86");
    emitter.instruction("movzx eax, BYTE PTR [r9 + 3]");                        // ':'
    emitter.instruction("cmp al, 58");                                          // require the scheme separator
    emitter.instruction("jne __rt_fsockopen_no_tls_x86");                       // not a scheme we wrap
    emitter.instruction("movzx eax, BYTE PTR [r9 + 4]");                        // '/'
    emitter.instruction("cmp al, 47");                                          // first slash
    emitter.instruction("jne __rt_fsockopen_no_tls_x86");                       // not a scheme we wrap
    emitter.instruction("movzx eax, BYTE PTR [r9 + 5]");                        // '/'
    emitter.instruction("cmp al, 47");                                          // second slash
    emitter.instruction("jne __rt_fsockopen_no_tls_x86");                       // not a scheme we wrap
    emitter.instruction("mov QWORD PTR [rbp - 32], 1");                         // remember that the stream must be TLS-wrapped
    emitter.instruction("add r9, 6");                                           // strip the scheme from the address host
    emitter.instruction("mov QWORD PTR [rbp - 8], r9");                         // publish the stripped pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // the peer name never carries a scheme either
    emitter.instruction("sub r10, 6");                                          // remaining hostname length
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // publish the stripped length
    emitter.instruction("mov QWORD PTR [rbp - 48], r10");                       // and the peer-name length
    emitter.label("__rt_fsockopen_no_tls_x86");

    // -- copy the "tcp://" scheme prefix into the address buffer --
    abi::emit_symbol_address(emitter, "r8", "_fsockopen_addr");                 // address buffer base
    abi::emit_symbol_address(emitter, "r9", "_ftp_tcp_prefix");                 // \"tcp://\" prefix base
    emitter.instruction("xor rcx, rcx");                                        // address write index
    emitter.label("__rt_fsockopen_pfx_x86");
    emitter.instruction("cmp rcx, 6");                                          // copied the whole \"tcp://\" prefix?
    emitter.instruction("jge __rt_fsockopen_pfx_done_x86");                     // prefix copied
    emitter.instruction("movzx eax, BYTE PTR [r9 + rcx]");                      // load a prefix byte
    emitter.instruction("mov BYTE PTR [r8 + rcx], al");                         // store it into the address buffer
    emitter.instruction("inc rcx");                                             // advance the write index
    emitter.instruction("jmp __rt_fsockopen_pfx_x86");                          // continue copying the prefix
    emitter.label("__rt_fsockopen_pfx_done_x86");

    // -- append the hostname --
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // hostname pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // hostname length
    emitter.instruction("xor rdx, rdx");                                        // hostname read index
    emitter.label("__rt_fsockopen_host_x86");
    emitter.instruction("cmp rdx, r10");                                        // copied the whole hostname?
    emitter.instruction("jge __rt_fsockopen_host_done_x86");                    // hostname copied
    emitter.instruction("movzx eax, BYTE PTR [r9 + rdx]");                      // load a hostname byte
    emitter.instruction("mov BYTE PTR [r8 + rcx], al");                         // append it to the address buffer
    emitter.instruction("inc rcx");                                             // advance the write index
    emitter.instruction("inc rdx");                                             // advance the hostname read index
    emitter.instruction("jmp __rt_fsockopen_host_x86");                         // continue copying the hostname
    emitter.label("__rt_fsockopen_host_done_x86");

    // -- append the ':' host/port separator --
    emitter.instruction("mov BYTE PTR [r8 + rcx], 58");                         // ':' separates the host and port
    emitter.instruction("inc rcx");                                             // advance the write index

    // -- format the port with __rt_itoa and append its digits --
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // port value into the __rt_itoa argument
    emitter.instruction("mov QWORD PTR [rbp - 8], r8");                         // save the address base across __rt_itoa
    emitter.instruction("mov QWORD PTR [rbp - 16], rcx");                       // save the write index across __rt_itoa
    emitter.instruction("call __rt_itoa");                                      // rax = digit pointer, rdx = digit count
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload the address base
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the write index
    emitter.instruction("xor r9, r9");                                          // port-digit copy index
    emitter.label("__rt_fsockopen_port_x86");
    emitter.instruction("cmp r9, rdx");                                         // copied every port digit?
    emitter.instruction("jge __rt_fsockopen_port_done_x86");                    // the address is complete
    emitter.instruction("movzx r10d, BYTE PTR [rax + r9]");                     // load a port digit
    emitter.instruction("mov BYTE PTR [r8 + rcx], r10b");                       // append it to the address buffer
    emitter.instruction("inc rcx");                                             // advance the write index
    emitter.instruction("inc r9");                                              // advance the digit copy index
    emitter.instruction("jmp __rt_fsockopen_port_x86");                         // continue copying port digits
    emitter.label("__rt_fsockopen_port_done_x86");

    // -- connect to the assembled tcp://host:port address --
    abi::emit_symbol_address(emitter, "rdi", "_fsockopen_addr");                // assembled address pointer
    emitter.instruction("mov rsi, rcx");                                        // total assembled address length
    emitter.instruction("call __rt_stream_socket_client");                      // connect, rax = fd or -1
    emitter.instruction("test rax, rax");                                       // did the TCP connection fail?
    emitter.instruction("js __rt_fsockopen_done_x86");                          // propagate the failure unchanged
    emitter.instruction("cmp QWORD PTR [rbp - 32], 0");                         // is this a TLS stream?
    emitter.instruction("je __rt_fsockopen_done_x86");                          // plain tcp:// needs no handshake
    // -- promote the connected descriptor to a TLS session (php's tls:// wrapper) --
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // park the descriptor while the handshake runs
    abi::emit_load_symbol_to_reg(emitter, "r11", "_elephc_tls_attach_fd_fn", 0); // published elephc-tls entry
    emitter.instruction("test r11, r11");                                       // a missing TLS runtime fails closed
    emitter.instruction("jz __rt_fsockopen_tls_fail_x86");                      // no handshake possible
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // descriptor → first argument
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // peer name pointer → second argument
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // peer name length → third argument
    emitter.instruction("call r11");                                            // complete the handshake, rax = handle or -1
    emitter.instruction("test rax, rax");                                       // did the handshake succeed?
    emitter.instruction("js __rt_fsockopen_tls_fail_x86");                      // a failed handshake reports failure
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // descriptor whose reads/writes become TLS
    abi::emit_symbol_address(emitter, "r11", "_tls_sessions");                  // fd → session handle table
    emitter.instruction("mov QWORD PTR [r11 + r10 * 8], rax");                  // route __rt_fread/__rt_fwrite through the session
    emitter.instruction("mov rax, r10");                                        // return the descriptor
    emitter.instruction("jmp __rt_fsockopen_done_x86");                         // finish the helper
    emitter.label("__rt_fsockopen_tls_fail_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // descriptor to close
    emitter.instruction("mov eax, 3");                                          // Linux x86_64 syscall 3 = close
    emitter.instruction("syscall");                                             // close(2): the handshake failed
    emitter.instruction("mov rax, -1");                                         // report failure, as php's wrapper does
    emitter.label("__rt_fsockopen_done_x86");
    emitter.instruction("add rsp, 48");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the connected descriptor
}
