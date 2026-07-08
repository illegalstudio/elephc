//! Purpose:
//! Emits the sockaddr-rendering runtime helpers. Three entry points:
//!
//! - `__rt_format_sockaddr_in` renders an IPv4 `sockaddr_in` as an owned
//!   `A.B.C.D:port` heap string.
//! - `__rt_format_sockaddr_in6` renders an IPv6 `sockaddr_in6` as an owned
//!   `[xxxx:...:xxxx]:port` heap string via libc `inet_ntop`. The
//!   bracketed-form matches PHP's IPv6 socket-name representation and is
//!   the URI form parsed by `stream_socket_client("[ipv6]:port")`.
//! - `__rt_format_sockaddr_unix` extracts the `sun_path` from a
//!   `sockaddr_un` (up to `addrlen - sa_family_size` bytes or the first
//!   NUL terminator) into an owned heap string; an unbound socket lowers
//!   to an empty string instead of garbage parsed out of zero bytes.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - `__rt_stream_socket_recvfrom` and friends dispatch on the address-family
//!   byte of the captured sender sockaddr and call the matching renderer.

use crate::codegen_support::{emit::Emitter, platform::Arch};

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// format_sockaddr_in: render a `sockaddr_in` as `A.B.C.D:port`.
/// Input:  AArch64 x0 = sockaddr_in pointer / x86_64 rdi = sockaddr_in pointer
/// Output: an owned heap string (AArch64 x1/x2, x86_64 rax/rdx)
pub fn emit_format_sockaddr_in(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_format_sockaddr_in_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: format_sockaddr_in ---");
    emitter.label_global("__rt_format_sockaddr_in");

    // Frame (64 bytes): [0]=packed IPv4 [8]=ip ptr [16]=ip len [24]=port scratch
    //                   [32]=port ptr [40]=port len [48]=x29 [56]=x30
    emitter.instruction("sub sp, sp, #64");                                     // allocate the formatting frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer

    // -- extract the port (sockaddr_in offset 2, network byte order) --
    emitter.instruction("ldrb w9, [x0, #2]");                                   // port high byte
    emitter.instruction("ldrb w10, [x0, #3]");                                  // port low byte
    emitter.instruction("lsl w9, w9, #8");                                      // shift the high byte into place
    emitter.instruction("orr w9, w9, w10");                                     // w9 = host-order port

    // -- extract the packed IPv4 address (sockaddr_in offset 4) --
    emitter.instruction("ldrb w10, [x0, #4]");                                  // address octet 0
    emitter.instruction("lsl w10, w10, #24");                                   // octet 0 to the high byte
    emitter.instruction("ldrb w11, [x0, #5]");                                  // address octet 1
    emitter.instruction("lsl w11, w11, #16");                                   // octet 1 to the second byte
    emitter.instruction("orr w10, w10, w11");                                   // merge octet 1
    emitter.instruction("ldrb w11, [x0, #6]");                                  // address octet 2
    emitter.instruction("lsl w11, w11, #8");                                    // octet 2 to the third byte
    emitter.instruction("orr w10, w10, w11");                                   // merge octet 2
    emitter.instruction("ldrb w11, [x0, #7]");                                  // address octet 3
    emitter.instruction("orr w10, w10, w11");                                   // w10 = packed IPv4 address
    emitter.instruction("str x10, [sp, #0]");                                   // save the packed IPv4 address

    // -- format the port inline into the scratch slot, right-to-left --
    emitter.instruction("mov x12, #5");                                         // scratch cursor (six-byte window)
    emitter.instruction("mov x13, #10");                                        // decimal base
    emitter.label("__rt_fsa_port");
    emitter.instruction("udiv x14, x9, x13");                                   // quotient = port / 10
    emitter.instruction("msub x15, x14, x13, x9");                              // remainder = port - quotient * 10
    emitter.instruction("add x15, x15, #48");                                   // remainder to an ASCII digit
    emitter.instruction("add x16, sp, #24");                                    // base of the port scratch slot
    emitter.instruction("strb w15, [x16, x12]");                                // store the digit at the cursor
    emitter.instruction("sub x12, x12, #1");                                    // move the cursor left
    emitter.instruction("mov x9, x14");                                         // port = port / 10
    emitter.instruction("cbnz x9, __rt_fsa_port");                              // keep formatting until the port is zero
    emitter.instruction("mov x5, #5");                                          // last cursor index
    emitter.instruction("sub x5, x5, x12");                                     // port length = 5 - cursor
    emitter.instruction("add x12, x12, #1");                                    // cursor now points at the first digit
    emitter.instruction("add x4, sp, #24");                                     // base of the port scratch slot
    emitter.instruction("add x4, x4, x12");                                     // x4 = port string pointer
    emitter.instruction("str x4, [sp, #32]");                                   // save the port string pointer
    emitter.instruction("str x5, [sp, #40]");                                   // save the port string length

    // -- render the IPv4 address through long2ip --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the packed IPv4 address
    emitter.instruction("bl __rt_long2ip");                                     // x1 = address string, x2 = length
    emitter.instruction("str x1, [sp, #8]");                                    // save the address string pointer
    emitter.instruction("str x2, [sp, #16]");                                   // save the address string length

    // -- allocate an owned heap string for "A.B.C.D:port" --
    emitter.instruction("ldr x3, [sp, #40]");                                   // port string length
    emitter.instruction("add x0, x2, x3");                                      // total length = ip + port
    emitter.instruction("add x0, x0, #1");                                      // plus the ':' separator
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the buffer, x0 = pointer
    emitter.instruction("mov x9, #1");                                          // heap kind 1 = persisted elephc string
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the buffer as an owned string

    // -- copy the address bytes into the buffer --
    emitter.instruction("ldr x4, [sp, #8]");                                    // address string pointer
    emitter.instruction("ldr x5, [sp, #16]");                                   // address string length
    emitter.instruction("mov x6, #0");                                          // copy index
    emitter.label("__rt_fsa_copy_ip");
    emitter.instruction("cmp x6, x5");                                          // copied every address byte?
    emitter.instruction("b.hs __rt_fsa_copy_ip_done");                          // address copy complete
    emitter.instruction("ldrb w7, [x4, x6]");                                   // load an address byte
    emitter.instruction("strb w7, [x0, x6]");                                   // store it into the buffer
    emitter.instruction("add x6, x6, #1");                                      // advance the copy index
    emitter.instruction("b __rt_fsa_copy_ip");                                  // keep copying the address
    emitter.label("__rt_fsa_copy_ip_done");

    // -- write the ':' separator, then copy the port bytes --
    emitter.instruction("mov w7, #58");                                         // ASCII ':' separator
    emitter.instruction("strb w7, [x0, x5]");                                   // write the separator after the address
    emitter.instruction("add x8, x5, #1");                                      // destination offset after "A.B.C.D:"
    emitter.instruction("ldr x4, [sp, #32]");                                   // port string pointer
    emitter.instruction("ldr x5, [sp, #40]");                                   // port string length
    emitter.instruction("mov x6, #0");                                          // copy index
    emitter.label("__rt_fsa_copy_port");
    emitter.instruction("cmp x6, x5");                                          // copied every port byte?
    emitter.instruction("b.hs __rt_fsa_copy_port_done");                        // port copy complete
    emitter.instruction("ldrb w7, [x4, x6]");                                   // load a port byte
    emitter.instruction("add x9, x8, x6");                                      // destination index past the separator
    emitter.instruction("strb w7, [x0, x9]");                                   // store it into the buffer
    emitter.instruction("add x6, x6, #1");                                      // advance the copy index
    emitter.instruction("b __rt_fsa_copy_port");                                // keep copying the port
    emitter.label("__rt_fsa_copy_port_done");

    // -- return the buffer pointer and total length --
    emitter.instruction("mov x1, x0");                                          // result string pointer
    emitter.instruction("ldr x9, [sp, #16]");                                   // address string length
    emitter.instruction("ldr x10, [sp, #40]");                                  // port string length
    emitter.instruction("add x2, x9, x10");                                     // total length = ip + port
    emitter.instruction("add x2, x2, #1");                                      // plus the ':' separator
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the formatting frame
    emitter.instruction("ret");                                                 // return the formatted address string
}

/// Emits the Linux x86_64 stream runtime helper for format sockaddr in.
fn emit_format_sockaddr_in_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: format_sockaddr_in ---");
    emitter.label_global("__rt_format_sockaddr_in");

    // Frame (rbp-relative): [-8]=packed IPv4 [-16]=ip ptr [-24]=ip len
    //                       [-32]=port scratch [-40]=port ptr [-48]=port len
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 48");                                         // allocate the formatting frame

    // -- extract the port (sockaddr_in offset 2, network byte order) --
    emitter.instruction("movzx r8d, BYTE PTR [rdi + 2]");                       // port high byte
    emitter.instruction("movzx r9d, BYTE PTR [rdi + 3]");                       // port low byte
    emitter.instruction("shl r8d, 8");                                          // shift the high byte into place
    emitter.instruction("or r8d, r9d");                                         // r8 = host-order port

    // -- extract the packed IPv4 address (sockaddr_in offset 4) --
    emitter.instruction("movzx r9d, BYTE PTR [rdi + 4]");                       // address octet 0
    emitter.instruction("shl r9d, 24");                                         // octet 0 to the high byte
    emitter.instruction("movzx r10d, BYTE PTR [rdi + 5]");                      // address octet 1
    emitter.instruction("shl r10d, 16");                                        // octet 1 to the second byte
    emitter.instruction("or r9d, r10d");                                        // merge octet 1
    emitter.instruction("movzx r10d, BYTE PTR [rdi + 6]");                      // address octet 2
    emitter.instruction("shl r10d, 8");                                         // octet 2 to the third byte
    emitter.instruction("or r9d, r10d");                                        // merge octet 2
    emitter.instruction("movzx r10d, BYTE PTR [rdi + 7]");                      // address octet 3
    emitter.instruction("or r9d, r10d");                                        // r9 = packed IPv4 address
    emitter.instruction("mov QWORD PTR [rbp - 8], r9");                         // save the packed IPv4 address

    // -- format the port inline into the scratch slot, right-to-left --
    emitter.instruction("mov rcx, 5");                                          // scratch cursor (six-byte window)
    emitter.label("__rt_fsa_port_x86");
    emitter.instruction("mov rax, r8");                                         // current port value
    emitter.instruction("xor edx, edx");                                        // clear the division high word
    emitter.instruction("mov r11, 10");                                         // decimal base divisor
    emitter.instruction("div r11");                                             // rax = port / 10, rdx = port % 10
    emitter.instruction("add dl, 48");                                          // remainder to an ASCII digit
    emitter.instruction("lea r11, [rbp - 32]");                                 // base of the port scratch slot
    emitter.instruction("mov BYTE PTR [r11 + rcx], dl");                        // store the digit at the cursor
    emitter.instruction("dec rcx");                                             // move the cursor left
    emitter.instruction("mov r8, rax");                                         // port = port / 10
    emitter.instruction("test r8, r8");                                         // is the port now zero?
    emitter.instruction("jnz __rt_fsa_port_x86");                               // keep formatting until the port is zero
    emitter.instruction("mov rax, 5");                                          // last cursor index
    emitter.instruction("sub rax, rcx");                                        // port length = 5 - cursor
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // save the port string length
    emitter.instruction("inc rcx");                                             // cursor now points at the first digit
    emitter.instruction("lea r11, [rbp - 32]");                                 // base of the port scratch slot
    emitter.instruction("add r11, rcx");                                        // r11 = port string pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], r11");                       // save the port string pointer

    // -- render the IPv4 address through long2ip --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the packed IPv4 address
    emitter.instruction("call __rt_long2ip");                                   // rax = address string, rdx = length
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the address string pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the address string length

    // -- allocate an owned heap string for "A.B.C.D:port" --
    emitter.instruction("mov rax, rdx");                                        // address string length
    emitter.instruction("add rax, QWORD PTR [rbp - 48]");                       // plus the port string length
    emitter.instruction("add rax, 1");                                          // plus the ':' separator
    emitter.instruction("call __rt_heap_alloc");                                // allocate the buffer, rax = pointer
    emitter.instruction(&format!(                                               // owned-string heap-kind word with the x86_64 heap marker
        "mov r10, 0x{:x}",
        (X86_64_HEAP_MAGIC_HI32 << 32) | 1
    ));
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the buffer as an owned string

    // -- copy the address bytes into the buffer --
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // address string pointer
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // address string length
    emitter.instruction("xor rcx, rcx");                                        // copy index
    emitter.label("__rt_fsa_copy_ip_x86");
    emitter.instruction("cmp rcx, r9");                                         // copied every address byte?
    emitter.instruction("jae __rt_fsa_copy_ip_done_x86");                       // address copy complete
    emitter.instruction("movzx edx, BYTE PTR [r8 + rcx]");                      // load an address byte
    emitter.instruction("mov BYTE PTR [rax + rcx], dl");                        // store it into the buffer
    emitter.instruction("inc rcx");                                             // advance the copy index
    emitter.instruction("jmp __rt_fsa_copy_ip_x86");                            // keep copying the address
    emitter.label("__rt_fsa_copy_ip_done_x86");

    // -- write the ':' separator, then copy the port bytes --
    emitter.instruction("mov BYTE PTR [rax + r9], 58");                         // write ':' after the address
    emitter.instruction("mov r8, QWORD PTR [rbp - 40]");                        // port string pointer
    emitter.instruction("mov r11, QWORD PTR [rbp - 48]");                       // port string length
    emitter.instruction("xor rcx, rcx");                                        // copy index
    emitter.label("__rt_fsa_copy_port_x86");
    emitter.instruction("cmp rcx, r11");                                        // copied every port byte?
    emitter.instruction("jae __rt_fsa_copy_port_done_x86");                     // port copy complete
    emitter.instruction("movzx edx, BYTE PTR [r8 + rcx]");                      // load a port byte
    emitter.instruction("mov rdi, r9");                                         // address string length
    emitter.instruction("add rdi, 1");                                          // plus the ':' separator
    emitter.instruction("add rdi, rcx");                                        // destination index in the buffer
    emitter.instruction("mov BYTE PTR [rax + rdi], dl");                        // store it into the buffer
    emitter.instruction("inc rcx");                                             // advance the copy index
    emitter.instruction("jmp __rt_fsa_copy_port_x86");                          // keep copying the port
    emitter.label("__rt_fsa_copy_port_done_x86");

    // -- return the buffer pointer and total length --
    emitter.instruction("mov rdx, r9");                                         // address string length
    emitter.instruction("add rdx, 1");                                          // plus the ':' separator
    emitter.instruction("add rdx, r11");                                        // plus the port string length
    emitter.instruction("add rsp, 48");                                         // release the formatting frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the formatted address string
}

/// format_sockaddr_in6: render a `sockaddr_in6` as `[ipv6_canonical]:port`.
/// Input:  AArch64 x0 = sockaddr_in6 pointer / x86_64 rdi = sockaddr_in6 pointer
/// Output: an owned heap string (AArch64 x1/x2, x86_64 rax/rdx).
/// libc inet_ntop handles the `::` shortening; a parse failure lowers to a
/// null pointer / zero length that the caller boxes as PHP false.
pub fn emit_format_sockaddr_in6(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_format_sockaddr_in6_linux_x86_64(emitter);
        return;
    }

    let plat = emitter.platform;
    let af_inet6 = plat.af_inet6();
    emitter.blank();
    emitter.comment("--- runtime: format_sockaddr_in6 ---");
    emitter.label_global("__rt_format_sockaddr_in6");

    // Frame (128 bytes):
    //   [0..16) saved x29/x30
    //   [16)    saved sockaddr_in6 pointer
    //   [24)    host-order port (i64)
    //   [32..80) inet_ntop output buffer (INET6_ADDRSTRLEN = 46; padded)
    //   [80)    ipv6 string pointer
    //   [88)    ipv6 string length
    //   [96)    port-scratch (6 bytes used, 8 reserved)
    //   [104)   port string pointer
    //   [112)   port string length
    //   [120..128) padding
    emitter.instruction("sub sp, sp, #128");                                    // allocate the helper frame
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #16]");                                   // save the sockaddr_in6 pointer

    // -- extract the port (sockaddr_in6 offset 2, network byte order) --
    emitter.instruction("ldrb w9, [x0, #2]");                                   // port high byte
    emitter.instruction("ldrb w10, [x0, #3]");                                  // port low byte
    emitter.instruction("lsl w9, w9, #8");                                      // shift the high byte into place
    emitter.instruction("orr w9, w9, w10");                                     // w9 = host-order port
    emitter.instruction("str x9, [sp, #24]");                                   // stash the port for later formatting

    // -- inet_ntop(AF_INET6, &sin6_addr, buf, 46) --
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the sockaddr_in6 pointer
    emitter.instruction("add x1, x0, #8");                                      // &sin6_addr (offset 8 in sockaddr_in6)
    emitter.instruction("add x2, sp, #32");                                     // out buffer (46-byte INET6_ADDRSTRLEN)
    emitter.instruction("mov x3, #46");                                         // INET6_ADDRSTRLEN
    emitter.instruction(&format!("mov x0, #{}", af_inet6));                     // family: AF_INET6
    emitter.bl_c("inet_ntop");                                                  // x0 = buf pointer (== sp+32) or NULL on failure
    emitter.instruction("cbz x0, __rt_fsa6_fail");                              // libc returned NULL → bail

    // -- strlen(buf) to find the IPv6 textual length --
    emitter.instruction("mov x9, #0");                                          // length counter
    emitter.label("__rt_fsa6_strlen");
    emitter.instruction("ldrb w10, [x0, x9]");                                  // load the next byte
    emitter.instruction("cbz w10, __rt_fsa6_strlen_done");                      // stop at the NUL terminator
    emitter.instruction("add x9, x9, #1");                                      // advance the cursor
    emitter.instruction("b __rt_fsa6_strlen");                                  // continue
    emitter.label("__rt_fsa6_strlen_done");
    emitter.instruction("str x0, [sp, #80]");                                   // save the IPv6 string pointer
    emitter.instruction("str x9, [sp, #88]");                                   // save the IPv6 string length

    // -- format the port inline into the scratch slot, right-to-left --
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the host-order port
    emitter.instruction("mov x12, #5");                                         // scratch cursor (six-byte window)
    emitter.instruction("mov x13, #10");                                        // decimal base
    emitter.label("__rt_fsa6_port");
    emitter.instruction("udiv x14, x9, x13");                                   // quotient = port / 10
    emitter.instruction("msub x15, x14, x13, x9");                              // remainder = port - quotient * 10
    emitter.instruction("add x15, x15, #48");                                   // remainder to ASCII digit
    emitter.instruction("add x16, sp, #96");                                    // base of port scratch slot
    emitter.instruction("strb w15, [x16, x12]");                                // store the digit at the cursor
    emitter.instruction("sub x12, x12, #1");                                    // move the cursor left
    emitter.instruction("mov x9, x14");                                         // port = port / 10
    emitter.instruction("cbnz x9, __rt_fsa6_port");                             // keep formatting until the port is zero
    emitter.instruction("mov x5, #5");                                          // last cursor index
    emitter.instruction("sub x5, x5, x12");                                     // port length = 5 - cursor
    emitter.instruction("add x12, x12, #1");                                    // first port digit index
    emitter.instruction("add x4, sp, #96");                                     // base of port scratch slot
    emitter.instruction("add x4, x4, x12");                                     // x4 = port string pointer
    emitter.instruction("str x4, [sp, #104]");                                  // save the port string pointer
    emitter.instruction("str x5, [sp, #112]");                                  // save the port string length

    // -- allocate the heap string: "[" + ipv6 + "]:" + port --
    emitter.instruction("ldr x9, [sp, #88]");                                   // ipv6 length
    emitter.instruction("ldr x10, [sp, #112]");                                 // port length
    emitter.instruction("add x0, x9, x10");                                     // ipv6 + port
    emitter.instruction("add x0, x0, #3");                                      // plus '[', ']', ':' separators
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the buffer, x0 = pointer
    emitter.instruction("mov x9, #1");                                          // heap kind 1 = persisted elephc string
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the buffer as an owned string

    // -- write '[' at index 0 --
    emitter.instruction("mov w11, #91");                                        // ASCII '['
    emitter.instruction("strb w11, [x0]");                                      // write the opening bracket

    // -- copy ipv6 bytes starting at index 1 --
    emitter.instruction("ldr x4, [sp, #80]");                                   // ipv6 string pointer
    emitter.instruction("ldr x5, [sp, #88]");                                   // ipv6 string length
    emitter.instruction("mov x6, #0");                                          // copy index
    emitter.label("__rt_fsa6_copy_ip");
    emitter.instruction("cmp x6, x5");                                          // copied every ipv6 byte?
    emitter.instruction("b.hs __rt_fsa6_copy_ip_done");                         // ipv6 copy complete
    emitter.instruction("ldrb w7, [x4, x6]");                                   // load an ipv6 byte
    emitter.instruction("add x9, x6, #1");                                      // destination = 1 + index (past '[')
    emitter.instruction("strb w7, [x0, x9]");                                   // store into the buffer
    emitter.instruction("add x6, x6, #1");                                      // advance the copy index
    emitter.instruction("b __rt_fsa6_copy_ip");                                 // keep copying
    emitter.label("__rt_fsa6_copy_ip_done");

    // -- write ']:' after the ipv6 bytes --
    emitter.instruction("add x9, x5, #1");                                      // index of ']' = ipv6_len + 1
    emitter.instruction("mov w11, #93");                                        // ASCII ']'
    emitter.instruction("strb w11, [x0, x9]");                                  // write the closing bracket
    emitter.instruction("add x9, x9, #1");                                      // index of ':' = ipv6_len + 2
    emitter.instruction("mov w11, #58");                                        // ASCII ':'
    emitter.instruction("strb w11, [x0, x9]");                                  // write the port separator

    // -- copy port bytes after "[ipv6]:" (offset = ipv6_len + 3) --
    emitter.instruction("ldr x4, [sp, #104]");                                  // port string pointer
    emitter.instruction("ldr x5, [sp, #112]");                                  // port string length
    emitter.instruction("ldr x10, [sp, #88]");                                  // ipv6 length
    emitter.instruction("add x10, x10, #3");                                    // destination base = ipv6_len + 3
    emitter.instruction("mov x6, #0");                                          // copy index
    emitter.label("__rt_fsa6_copy_port");
    emitter.instruction("cmp x6, x5");                                          // copied every port byte?
    emitter.instruction("b.hs __rt_fsa6_copy_port_done");                       // port copy complete
    emitter.instruction("ldrb w7, [x4, x6]");                                   // load a port byte
    emitter.instruction("add x9, x10, x6");                                     // destination index
    emitter.instruction("strb w7, [x0, x9]");                                   // store into the buffer
    emitter.instruction("add x6, x6, #1");                                      // advance the copy index
    emitter.instruction("b __rt_fsa6_copy_port");                               // keep copying
    emitter.label("__rt_fsa6_copy_port_done");

    // -- return the buffer pointer and total length --
    emitter.instruction("mov x1, x0");                                          // result string pointer
    emitter.instruction("ldr x9, [sp, #88]");                                   // ipv6 length
    emitter.instruction("ldr x10, [sp, #112]");                                 // port length
    emitter.instruction("add x2, x9, x10");                                     // total = ipv6 + port
    emitter.instruction("add x2, x2, #3");                                      // plus '[', ']', ':'
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #128");                                    // release the frame
    emitter.instruction("ret");                                                 // return the formatted address string

    emitter.label("__rt_fsa6_fail");
    emitter.instruction("mov x1, #0");                                          // null pointer signals an unformattable address
    emitter.instruction("mov x2, #0");                                          // zero length for the failure case
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #128");                                    // release the frame
    emitter.instruction("ret");                                                 // return the failure result
}

/// Emits the Linux x86_64 stream runtime helper for format sockaddr in6.
fn emit_format_sockaddr_in6_linux_x86_64(emitter: &mut Emitter) {
    let af_inet6 = crate::codegen_support::platform::Platform::Linux.af_inet6();
    emitter.blank();
    emitter.comment("--- runtime: format_sockaddr_in6 ---");
    emitter.label_global("__rt_format_sockaddr_in6");

    // Frame (rbp-relative):
    //   [-8)   saved sockaddr_in6 pointer
    //   [-16)  host-order port
    //   [-64..-16) inet_ntop output buffer (46 + padding)
    //   [-72)  ipv6 string pointer
    //   [-80)  ipv6 string length
    //   [-88)  port-scratch (6 bytes used, 8 reserved)
    //   [-96)  port string pointer
    //   [-104) port string length
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 112");                                        // allocate the helper frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the sockaddr_in6 pointer

    // -- extract the port (sockaddr_in6 offset 2, network byte order) --
    emitter.instruction("movzx r8d, BYTE PTR [rdi + 2]");                       // port high byte
    emitter.instruction("movzx r9d, BYTE PTR [rdi + 3]");                       // port low byte
    emitter.instruction("shl r8d, 8");                                          // shift the high byte into place
    emitter.instruction("or r8d, r9d");                                         // r8 = host-order port
    emitter.instruction("mov QWORD PTR [rbp - 16], r8");                        // stash the port

    // -- inet_ntop(AF_INET6, &sin6_addr, buf, 46) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the sockaddr_in6 pointer
    emitter.instruction("lea rsi, [rdi + 8]");                                  // &sin6_addr (offset 8)
    emitter.instruction("lea rdx, [rbp - 64]");                                 // out buffer (46-byte INET6_ADDRSTRLEN)
    emitter.instruction("mov ecx, 46");                                         // INET6_ADDRSTRLEN
    emitter.instruction(&format!("mov edi, {}", af_inet6));                     // family: AF_INET6
    emitter.instruction("call inet_ntop");                                      // rax = buf pointer or NULL
    emitter.instruction("test rax, rax");                                       // libc returned NULL?
    emitter.instruction("jz __rt_fsa6_fail_x86");                               // bail on failure

    // -- strlen(buf) to find the IPv6 textual length --
    emitter.instruction("xor rcx, rcx");                                        // length counter
    emitter.label("__rt_fsa6_strlen_x86");
    emitter.instruction("movzx edx, BYTE PTR [rax + rcx]");                     // load the next byte
    emitter.instruction("test dl, dl");                                         // NUL?
    emitter.instruction("je __rt_fsa6_strlen_done_x86");                        // stop at the NUL terminator
    emitter.instruction("inc rcx");                                             // advance
    emitter.instruction("jmp __rt_fsa6_strlen_x86");                            // continue
    emitter.label("__rt_fsa6_strlen_done_x86");
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // save the IPv6 string pointer
    emitter.instruction("mov QWORD PTR [rbp - 80], rcx");                       // save the IPv6 string length

    // -- format the port inline into the scratch slot, right-to-left --
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // reload the host-order port
    emitter.instruction("mov rcx, 5");                                          // scratch cursor (six-byte window)
    emitter.label("__rt_fsa6_port_x86");
    emitter.instruction("mov rax, r8");                                         // current port value
    emitter.instruction("xor edx, edx");                                        // clear division high word
    emitter.instruction("mov r11, 10");                                         // decimal base divisor
    emitter.instruction("div r11");                                             // rax = port / 10, rdx = port % 10
    emitter.instruction("add dl, 48");                                          // remainder to ASCII digit
    emitter.instruction("lea r11, [rbp - 88]");                                 // base of the port scratch slot
    emitter.instruction("mov BYTE PTR [r11 + rcx], dl");                        // store the digit at the cursor
    emitter.instruction("dec rcx");                                             // move the cursor left
    emitter.instruction("mov r8, rax");                                         // port = port / 10
    emitter.instruction("test r8, r8");                                         // is the port now zero?
    emitter.instruction("jnz __rt_fsa6_port_x86");                              // keep formatting until the port is zero
    emitter.instruction("mov rax, 5");                                          // last cursor index
    emitter.instruction("sub rax, rcx");                                        // port length = 5 - cursor
    emitter.instruction("mov QWORD PTR [rbp - 104], rax");                      // save the port string length
    emitter.instruction("inc rcx");                                             // cursor now points at the first digit
    emitter.instruction("lea r11, [rbp - 88]");                                 // base of the port scratch slot
    emitter.instruction("add r11, rcx");                                        // r11 = port string pointer
    emitter.instruction("mov QWORD PTR [rbp - 96], r11");                       // save the port string pointer

    // -- allocate the heap string: "[" + ipv6 + "]:" + port --
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // ipv6 length
    emitter.instruction("add rax, QWORD PTR [rbp - 104]");                      // plus port length
    emitter.instruction("add rax, 3");                                          // plus '[', ']', ':'
    emitter.instruction("call __rt_heap_alloc");                                // allocate the buffer, rax = pointer
    emitter.instruction("mov r10, 0x4540504800000001");                         // owned-string heap-kind word with the x86_64 heap marker (low byte 1 = persisted elephc string)
    // The marker constant matches X86_64_HEAP_MAGIC_HI32 (0x454C5048 << 32) | 1
    emitter.instruction("mov r10, 0x454C504800000001");                         // (corrected: heap_magic_hi32 = 0x454C5048)
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the buffer as an owned string

    // -- write '[' at index 0 --
    emitter.instruction("mov BYTE PTR [rax], 91");                              // ASCII '['

    // -- copy ipv6 bytes starting at index 1 --
    emitter.instruction("mov r8, QWORD PTR [rbp - 72]");                        // ipv6 string pointer
    emitter.instruction("mov r9, QWORD PTR [rbp - 80]");                        // ipv6 string length
    emitter.instruction("xor rcx, rcx");                                        // copy index
    emitter.label("__rt_fsa6_copy_ip_x86");
    emitter.instruction("cmp rcx, r9");                                         // copied every ipv6 byte?
    emitter.instruction("jae __rt_fsa6_copy_ip_done_x86");                      // ipv6 copy complete
    emitter.instruction("movzx edx, BYTE PTR [r8 + rcx]");                      // load an ipv6 byte
    emitter.instruction("lea r11, [rcx + 1]");                                  // destination = 1 + index (past '[')
    emitter.instruction("mov BYTE PTR [rax + r11], dl");                        // store into the buffer
    emitter.instruction("inc rcx");                                             // advance the copy index
    emitter.instruction("jmp __rt_fsa6_copy_ip_x86");                           // keep copying
    emitter.label("__rt_fsa6_copy_ip_done_x86");

    // -- write ']:' after the ipv6 bytes --
    emitter.instruction("lea r11, [r9 + 1]");                                   // index of ']'
    emitter.instruction("mov BYTE PTR [rax + r11], 93");                        // ASCII ']'
    emitter.instruction("inc r11");                                             // index of ':'
    emitter.instruction("mov BYTE PTR [rax + r11], 58");                        // ASCII ':'

    // -- copy port bytes after "[ipv6]:" (offset = ipv6_len + 3) --
    emitter.instruction("mov r8, QWORD PTR [rbp - 96]");                        // port string pointer
    emitter.instruction("mov r10, QWORD PTR [rbp - 104]");                      // port string length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 80]");                       // ipv6 length
    emitter.instruction("add rdi, 3");                                          // destination base = ipv6_len + 3
    emitter.instruction("xor rcx, rcx");                                        // copy index
    emitter.label("__rt_fsa6_copy_port_x86");
    emitter.instruction("cmp rcx, r10");                                        // copied every port byte?
    emitter.instruction("jae __rt_fsa6_copy_port_done_x86");                    // port copy complete
    emitter.instruction("movzx edx, BYTE PTR [r8 + rcx]");                      // load a port byte
    emitter.instruction("mov r11, rdi");                                        // destination base
    emitter.instruction("add r11, rcx");                                        // plus copy index
    emitter.instruction("mov BYTE PTR [rax + r11], dl");                        // store into the buffer
    emitter.instruction("inc rcx");                                             // advance the copy index
    emitter.instruction("jmp __rt_fsa6_copy_port_x86");                         // keep copying
    emitter.label("__rt_fsa6_copy_port_done_x86");

    // -- return the buffer pointer and total length --
    emitter.instruction("mov rdx, QWORD PTR [rbp - 80]");                       // ipv6 length
    emitter.instruction("add rdx, QWORD PTR [rbp - 104]");                      // plus port length
    emitter.instruction("add rdx, 3");                                          // plus '[', ']', ':'
    emitter.instruction("add rsp, 112");                                        // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the formatted address string

    emitter.label("__rt_fsa6_fail_x86");
    emitter.instruction("xor eax, eax");                                        // null pointer signals an unformattable address
    emitter.instruction("xor edx, edx");                                        // zero length for the failure case
    emitter.instruction("add rsp, 112");                                        // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the failure result
}

/// format_sockaddr_unix: render a `sockaddr_un` as its `sun_path` string.
/// Input:  AArch64 x0 = sockaddr_un pointer, x1 = addrlen (from the syscall)
///         x86_64  rdi = sockaddr_un pointer, rsi = addrlen
/// Output: an owned heap string (AArch64 x1/x2, x86_64 rax/rdx). An unbound
///         socket (addrlen <= 2) returns an empty heap string.
pub fn emit_format_sockaddr_unix(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_format_sockaddr_unix_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: format_sockaddr_unix ---");
    emitter.label_global("__rt_format_sockaddr_unix");

    // Frame (32 bytes): [0]=sun_path ptr [8]=sun_path len [16]=x29 [24]=x30.
    emitter.instruction("stp x29, x30, [sp, #-32]!");                           // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer

    // sun_path lives at offset 2 on both Linux (`sa_family_t` is two bytes) and
    // macOS (`sun_len` + `sun_family` are one byte each).
    emitter.instruction("add x9, x0, #2");                                      // sun_path pointer
    emitter.instruction("subs x10, x1, #2");                                    // sun_path region length (addrlen - sa_family bytes)
    emitter.instruction("b.le __rt_fsu_unnamed");                               // unbound socket: addrlen<=2 → empty path

    // -- find the NUL terminator (or stop at the addrlen-derived max) --
    emitter.instruction("mov x11, #0");                                         // sun_path length accumulator
    emitter.label("__rt_fsu_scan");
    emitter.instruction("cmp x11, x10");                                        // hit the addrlen-derived upper bound?
    emitter.instruction("b.hs __rt_fsu_scan_done");                             // stop when we exhaust the reported bytes
    emitter.instruction("ldrb w12, [x9, x11]");                                 // peek at the next sun_path byte
    emitter.instruction("cbz w12, __rt_fsu_scan_done");                         // stop at the C-string terminator
    emitter.instruction("add x11, x11, #1");                                    // advance the cursor
    emitter.instruction("b __rt_fsu_scan");                                     // keep scanning the path bytes
    emitter.label("__rt_fsu_scan_done");
    emitter.instruction("cbz x11, __rt_fsu_unnamed");                           // a zero-length path is the unnamed case

    // -- allocate an owned heap string for the path --
    emitter.instruction("str x9, [sp, #0]");                                    // save the sun_path pointer across the alloc
    emitter.instruction("str x11, [sp, #8]");                                   // save the sun_path length across the alloc
    emitter.instruction("mov x0, x11");                                         // alloc size = sun_path length
    emitter.instruction("bl __rt_heap_alloc");                                  // x0 = persisted-string buffer pointer
    emitter.instruction("mov x9, #1");                                          // heap kind 1 = persisted elephc string
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the buffer as an owned string

    // -- copy sun_path into the heap string --
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the sun_path pointer
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload the sun_path length
    emitter.instruction("mov x6, #0");                                          // copy index
    emitter.label("__rt_fsu_copy");
    emitter.instruction("cmp x6, x11");                                         // copied every path byte?
    emitter.instruction("b.hs __rt_fsu_copy_done");                             // path copy complete
    emitter.instruction("ldrb w7, [x9, x6]");                                   // load a path byte
    emitter.instruction("strb w7, [x0, x6]");                                   // store it into the buffer
    emitter.instruction("add x6, x6, #1");                                      // advance the copy index
    emitter.instruction("b __rt_fsu_copy");                                     // keep copying the path
    emitter.label("__rt_fsu_copy_done");

    emitter.instruction("mov x1, x0");                                          // result string pointer
    emitter.instruction("mov x2, x11");                                         // result string length = sun_path length
    emitter.instruction("ldp x29, x30, [sp], #32");                             // restore frame pointer and return address
    emitter.instruction("ret");                                                 // return the sun_path string

    emitter.label("__rt_fsu_unnamed");
    // -- unbound Unix socket: return an empty heap-allocated string --
    emitter.instruction("mov x0, #1");                                          // alloc one byte so the heap header is well-formed
    emitter.instruction("bl __rt_heap_alloc");                                  // x0 = single-byte buffer
    emitter.instruction("mov x9, #1");                                          // heap kind 1 = persisted elephc string
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the buffer as an owned string
    emitter.instruction("mov x1, x0");                                          // result string pointer
    emitter.instruction("mov x2, #0");                                          // result length = 0 (empty string)
    emitter.instruction("ldp x29, x30, [sp], #32");                             // restore frame pointer and return address
    emitter.instruction("ret");                                                 // return the empty path string
}

/// Emits the Linux x86_64 stream runtime helper for format sockaddr unix.
fn emit_format_sockaddr_unix_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: format_sockaddr_unix ---");
    emitter.label_global("__rt_format_sockaddr_unix");

    // Frame (16 bytes, rbp-relative): [-8]=sun_path ptr/len scratch slot.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 32");                                         // scratch for sun_path ptr/len across the alloc

    // sun_path starts at buffer offset 2 (sa_family_t is two bytes on Linux).
    emitter.instruction("lea r8, [rdi + 2]");                                   // sun_path pointer
    emitter.instruction("mov r9, rsi");                                         // addrlen
    emitter.instruction("sub r9, 2");                                           // sun_path region length (addrlen - sa_family bytes)
    emitter.instruction("jle __rt_fsu_unnamed_x86");                            // unbound socket: addrlen<=2 → empty path

    // -- find the NUL terminator (or stop at the addrlen-derived max) --
    emitter.instruction("xor rcx, rcx");                                        // sun_path length accumulator
    emitter.label("__rt_fsu_scan_x86");
    emitter.instruction("cmp rcx, r9");                                         // hit the addrlen-derived upper bound?
    emitter.instruction("jae __rt_fsu_scan_done_x86");                          // stop when we exhaust the reported bytes
    emitter.instruction("movzx edx, BYTE PTR [r8 + rcx]");                      // peek at the next sun_path byte
    emitter.instruction("test dl, dl");                                         // is it the C-string terminator?
    emitter.instruction("je __rt_fsu_scan_done_x86");                           // stop at the NUL byte
    emitter.instruction("inc rcx");                                             // advance the cursor
    emitter.instruction("jmp __rt_fsu_scan_x86");                               // keep scanning the path bytes
    emitter.label("__rt_fsu_scan_done_x86");
    emitter.instruction("test rcx, rcx");                                       // zero-length path?
    emitter.instruction("jz __rt_fsu_unnamed_x86");                             // treat it as the unnamed case

    // -- allocate an owned heap string for the path --
    emitter.instruction("mov QWORD PTR [rbp - 8], r8");                         // save the sun_path pointer across the alloc
    emitter.instruction("mov QWORD PTR [rbp - 16], rcx");                       // save the sun_path length across the alloc
    emitter.instruction("mov rax, rcx");                                        // alloc size = sun_path length
    emitter.instruction("call __rt_heap_alloc");                                // rax = persisted-string buffer pointer
    emitter.instruction(&format!(                                               // owned-string heap-kind word with the x86_64 heap marker
        "mov r10, 0x{:x}",
        (X86_64_HEAP_MAGIC_HI32 << 32) | 1
    ));
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the buffer as an owned string

    // -- copy sun_path into the heap string --
    emitter.instruction("mov r8, QWORD PTR [rbp - 8]");                         // reload the sun_path pointer
    emitter.instruction("mov r9, QWORD PTR [rbp - 16]");                        // reload the sun_path length
    emitter.instruction("xor rcx, rcx");                                        // copy index
    emitter.label("__rt_fsu_copy_x86");
    emitter.instruction("cmp rcx, r9");                                         // copied every path byte?
    emitter.instruction("jae __rt_fsu_copy_done_x86");                          // path copy complete
    emitter.instruction("movzx edx, BYTE PTR [r8 + rcx]");                      // load a path byte
    emitter.instruction("mov BYTE PTR [rax + rcx], dl");                        // store it into the buffer
    emitter.instruction("inc rcx");                                             // advance the copy index
    emitter.instruction("jmp __rt_fsu_copy_x86");                               // keep copying the path
    emitter.label("__rt_fsu_copy_done_x86");

    emitter.instruction("mov rdx, r9");                                         // result string length = sun_path length
    emitter.instruction("add rsp, 32");                                         // release the scratch slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the sun_path string

    emitter.label("__rt_fsu_unnamed_x86");
    // -- unbound Unix socket: return an empty heap-allocated string --
    emitter.instruction("mov rax, 1");                                          // alloc one byte so the heap header is well-formed
    emitter.instruction("call __rt_heap_alloc");                                // rax = single-byte buffer
    emitter.instruction(&format!(                                               // owned-string heap-kind word with the x86_64 heap marker
        "mov r10, 0x{:x}",
        (X86_64_HEAP_MAGIC_HI32 << 32) | 1
    ));
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the buffer as an owned string
    emitter.instruction("xor edx, edx");                                        // result length = 0 (empty string)
    emitter.instruction("add rsp, 32");                                         // release the scratch slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the empty path string
}
