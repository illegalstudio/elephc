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

    // Frame (48 bytes): [0]=hostname ptr [8]=hostname len [16]=port.
    emitter.instruction("sub sp, sp, #48");                                     // allocate the helper frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the hostname pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the hostname length
    emitter.instruction("str x2, [sp, #16]");                                   // save the port

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
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return the connected descriptor
}

/// Emits the Linux x86_64 stream runtime helper for fsockopen.
fn emit_fsockopen_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fsockopen ---");
    emitter.label_global("__rt_fsockopen");

    // Frame (rbp-relative): [-8]=hostname ptr [-16]=hostname len [-24]=port.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 32");                                         // reserve the helper spill slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the hostname pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the hostname length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the port

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
    emitter.instruction("add rsp, 32");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the connected descriptor
}
