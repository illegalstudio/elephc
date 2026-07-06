//! Purpose:
//! Emits the `__rt_stream_socket_recvfrom` runtime helper, which receives a
//! message from a socket through the `recvfrom` system call.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - The payload is received into an owned heap string so the result can be
//!   boxed as a `string|false` mixed value without aliasing scratch buffers.
//! - The 128-byte sender-sockaddr buffer is wide enough for IPv4
//!   (`sockaddr_in`), Unix-domain paths (`sockaddr_un` is 108 bytes), and
//!   the IPv6 layout (room for the future `sockaddr_in6`).
//! - On success the family byte returned by `recvfrom` selects the matching
//!   renderer: `__rt_format_sockaddr_in` for IPv4 peers,
//!   `__rt_format_sockaddr_unix` for Unix-domain peers, and an empty owned
//!   heap string for an unsupported / unset family (so the optional
//!   `$address` writeback stays well-typed). A failed receive clears the
//!   stashed-address globals.

use crate::codegen_support::{
    abi,
    emit::Emitter,
    platform::{Arch, Platform},
};

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// Byte offset of the address-family discriminator inside a freshly populated
/// sockaddr buffer; macOS keeps a 1-byte `sa_len` ahead of the family byte
/// (BSD layout), while Linux puts the 16-bit family at offset 0 directly.
fn family_byte_offset(platform: Platform) -> i64 {
    match platform {
        Platform::MacOS => 1,
        Platform::Linux => 0,
    }
}

/// stream_socket_recvfrom: receive a message from a socket descriptor.
/// Input:  AArch64 x0 = fd, x1 = length, x2 = flags
///         x86_64  rdi = fd, rsi = length, rdx = flags
/// Output: string pointer/length, or a null pointer on failure.
///         The sender address is stashed in `_recvfrom_addr_ptr` / `_len`.
pub fn emit_stream_socket_recvfrom(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stream_socket_recvfrom_linux_x86_64(emitter);
        return;
    }

    let plat = emitter.platform;
    let family_off = 32 + family_byte_offset(plat);
    emitter.blank();
    emitter.comment("--- runtime: stream_socket_recvfrom ---");
    emitter.label_global("__rt_stream_socket_recvfrom");

    // Frame (192 bytes): [0]=fd [8]=length [16]=flags [24]=buffer
    //   [32..160) sockaddr (128 bytes)
    //   [160]=addrlen [168]=byte count [176]=x29 [184]=x30.
    emitter.instruction("sub sp, sp, #192");                                    // frame for the receive state and the sender sockaddr
    emitter.instruction("stp x29, x30, [sp, #176]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #176");                                   // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the socket descriptor
    emitter.instruction("str x1, [sp, #8]");                                    // save the requested length
    emitter.instruction("str x2, [sp, #16]");                                   // save the receive flags

    // -- allocate an owned heap buffer for the payload --
    emitter.instruction("mov x0, x1");                                          // allocation size = requested length
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the buffer, x0 = pointer
    emitter.instruction("mov x9, #1");                                          // heap kind 1 = persisted elephc string
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the buffer as an owned string
    emitter.instruction("str x0, [sp, #24]");                                   // save the buffer pointer

    // -- zero the sender sockaddr leading bytes and set the addrlen parameter --
    emitter.instruction("str xzr, [sp, #32]");                                  // clear the first 8 bytes of the sockaddr
    emitter.instruction("str xzr, [sp, #40]");                                  // clear the next 8 bytes of the sockaddr
    emitter.instruction("mov w9, #128");                                        // sender sockaddr capacity
    emitter.instruction("str w9, [sp, #160]");                                  // addrlen in/out parameter = 128

    // -- recvfrom(fd, buf, length, flags, &sockaddr, &addrlen) --
    emitter.instruction("ldr x0, [sp, #0]");                                    // socket descriptor
    emitter.instruction("ldr x1, [sp, #24]");                                   // buffer pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // requested length
    emitter.instruction("ldr x3, [sp, #16]");                                   // receive flags
    emitter.instruction("add x4, sp, #32");                                     // pointer to the sender sockaddr
    emitter.instruction("add x5, sp, #160");                                    // pointer to the addrlen parameter
    emitter.syscall(29);
    if plat.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: a negative result means failure
    }
    emitter.instruction(&plat.branch_on_syscall_success("__rt_ssr_ok"));        // continue when recvfrom succeeded

    // -- failure: clear the stashed address and return a null string --
    abi::emit_symbol_address(emitter, "x9", "_recvfrom_addr_ptr");
    emitter.instruction("str xzr, [x9]");                                       // clear the stashed sender address pointer
    abi::emit_symbol_address(emitter, "x9", "_recvfrom_addr_len");
    emitter.instruction("str xzr, [x9]");                                       // clear the stashed sender address length
    emitter.instruction("mov x1, #0");                                          // a null pointer signals a failed receive
    emitter.instruction("mov x2, #0");                                          // zero length for the failure case
    emitter.instruction("ldp x29, x30, [sp, #176]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #192");                                    // release the frame
    emitter.instruction("ret");                                                 // return the failure result

    // -- success: stash the formatted sender address, return the payload --
    emitter.label("__rt_ssr_ok");
    emitter.instruction("str x0, [sp, #168]");                                  // save the received byte count

    // -- dispatch on the sockaddr's address family --
    emitter.instruction("ldr w9, [sp, #160]");                                  // reload addrlen returned by recvfrom
    emitter.instruction("cmp w9, #2");                                          // is there at least a family-byte's worth of data?
    emitter.instruction("b.lt __rt_ssr_empty_addr");                            // connected/no-peer recvfrom: empty $address
    let af_inet6 = plat.af_inet6();
    emitter.instruction(&format!("ldrb w10, [sp, #{}]", family_off));           // load the address-family discriminator
    emitter.instruction("cmp w10, #2");                                         // AF_INET = 2 on both platforms
    emitter.instruction("b.eq __rt_ssr_inet");                                  // IPv4 peer: format A.B.C.D:port
    emitter.instruction("cmp w10, #1");                                         // AF_UNIX = 1 on both platforms
    emitter.instruction("b.eq __rt_ssr_unix");                                  // Unix-domain peer: format sun_path
    emitter.instruction(&format!("cmp w10, #{}", af_inet6));                    // AF_INET6 = 30 on macOS / 10 on Linux
    emitter.instruction("b.eq __rt_ssr_inet6");                                 // IPv6 peer: format [ipv6]:port
    emitter.instruction("b __rt_ssr_empty_addr");                               // unsupported family: empty $address

    // -- AF_INET peer: hand the captured sockaddr to the IPv4 formatter --
    emitter.label("__rt_ssr_inet");
    emitter.instruction("add x0, sp, #32");                                     // pointer to the captured sender sockaddr
    emitter.instruction("bl __rt_format_sockaddr_in");                          // x1 = address string, x2 = length
    emitter.instruction("b __rt_ssr_stash_addr");                               // common stash + return

    // -- AF_UNIX peer: hand the captured sockaddr to the Unix formatter --
    emitter.label("__rt_ssr_unix");
    emitter.instruction("add x0, sp, #32");                                     // pointer to the captured sender sockaddr
    emitter.instruction("ldr w1, [sp, #160]");                                  // addrlen returned by recvfrom (sun_path length depends on it)
    emitter.instruction("bl __rt_format_sockaddr_unix");                        // x1 = sun_path string, x2 = length
    emitter.instruction("b __rt_ssr_stash_addr");                               // common stash + return

    // -- AF_INET6 peer: hand the captured sockaddr to the IPv6 formatter --
    emitter.label("__rt_ssr_inet6");
    emitter.instruction("add x0, sp, #32");                                     // pointer to the captured sender sockaddr_in6
    emitter.instruction("bl __rt_format_sockaddr_in6");                         // x1 = address string, x2 = length
    emitter.instruction("b __rt_ssr_stash_addr");                               // common stash + return

    // -- empty $address: connected socket, AF_INET6, or unsupported family --
    emitter.label("__rt_ssr_empty_addr");
    emitter.instruction("mov x0, #1");                                          // alloc one byte so the heap header is well-formed
    emitter.instruction("bl __rt_heap_alloc");                                  // x0 = single-byte buffer
    emitter.instruction("mov x9, #1");                                          // heap kind 1 = persisted elephc string
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the buffer as an owned string
    emitter.instruction("mov x1, x0");                                          // empty address pointer
    emitter.instruction("mov x2, #0");                                          // empty address length

    emitter.label("__rt_ssr_stash_addr");
    abi::emit_symbol_address(emitter, "x9", "_recvfrom_addr_ptr");
    emitter.instruction("str x1, [x9]");                                        // stash the sender address pointer
    abi::emit_symbol_address(emitter, "x9", "_recvfrom_addr_len");
    emitter.instruction("str x2, [x9]");                                        // stash the sender address length
    emitter.instruction("ldr x1, [sp, #24]");                                   // owned buffer pointer becomes the result
    emitter.instruction("ldr x2, [sp, #168]");                                  // received byte count becomes the length
    emitter.instruction("ldp x29, x30, [sp, #176]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #192");                                    // release the frame
    emitter.instruction("ret");                                                 // return the received string slice
}

/// Emits the Linux x86_64 stream runtime helper for stream socket recvfrom.
fn emit_stream_socket_recvfrom_linux_x86_64(emitter: &mut Emitter) {
    let family_off = family_byte_offset(Platform::Linux);
    emitter.blank();
    emitter.comment("--- runtime: stream_socket_recvfrom ---");
    emitter.label_global("__rt_stream_socket_recvfrom");

    // Frame (rbp-relative): [-8]=fd [-16]=length [-24]=flags [-32]=buffer
    //   [-160..-32)  sockaddr (128 bytes)
    //   [-168]=addrlen [-176]=byte count
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 176");                                        // frame for the receive state and the sender sockaddr
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the socket descriptor
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the requested length
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the receive flags

    // -- allocate an owned heap buffer for the payload --
    emitter.instruction("mov rax, rsi");                                        // allocation size = requested length
    emitter.instruction("call __rt_heap_alloc");                                // allocate the buffer, rax = pointer
    emitter.instruction(&format!(                                               // owned-string heap-kind word with the x86_64 heap marker
        "mov r10, 0x{:x}",
        (X86_64_HEAP_MAGIC_HI32 << 32) | 1
    ));
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the buffer as an owned string
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the buffer pointer

    // -- zero the sender sockaddr leading bytes and set the addrlen parameter --
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // clear the first 8 bytes of the sockaddr
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // clear the next 8 bytes of the sockaddr
    emitter.instruction("mov DWORD PTR [rbp - 168], 128");                      // addrlen in/out parameter = 128

    // -- recvfrom(fd, buf, length, flags, &sockaddr, &addrlen) --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // socket descriptor
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // buffer pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // requested length
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // receive flags
    emitter.instruction("lea r8, [rbp - 160]");                                 // pointer to the sender sockaddr
    emitter.instruction("lea r9, [rbp - 168]");                                 // pointer to the addrlen parameter
    emitter.instruction("mov eax, 45");                                         // Linux x86_64 syscall 45 = recvfrom
    emitter.instruction("syscall");                                             // receive the message
    emitter.instruction("cmp rax, 0");                                          // did recvfrom fail?
    emitter.instruction("jl __rt_ssr_fail_x86");                                // a negative result means failure

    // -- success: stash the formatted sender address, return the payload --
    emitter.instruction("mov QWORD PTR [rbp - 176], rax");                      // save the received byte count

    // -- dispatch on the sockaddr's address family --
    emitter.instruction("movsxd r10, DWORD PTR [rbp - 168]");                   // reload addrlen returned by recvfrom (signed widen)
    emitter.instruction("cmp r10, 2");                                          // is there at least a family-byte's worth of data?
    emitter.instruction("jl __rt_ssr_empty_addr_x86");                          // connected/no-peer recvfrom: empty $address
    let family_addr_disp = -160 + family_off;
    emitter.instruction(&format!(                                               // load the address-family discriminator
        "movzx r11d, BYTE PTR [rbp - {}]",
        -family_addr_disp
    ));
    let af_inet6 = Platform::Linux.af_inet6();
    emitter.instruction("cmp r11d, 2");                                         // AF_INET = 2 on Linux
    emitter.instruction("je __rt_ssr_inet_x86");                                // IPv4 peer: format A.B.C.D:port
    emitter.instruction("cmp r11d, 1");                                         // AF_UNIX = 1 on Linux
    emitter.instruction("je __rt_ssr_unix_x86");                                // Unix-domain peer: format sun_path
    emitter.instruction(&format!("cmp r11d, {}", af_inet6));                    // AF_INET6 = 10 on Linux
    emitter.instruction("je __rt_ssr_inet6_x86");                               // IPv6 peer: format [ipv6]:port
    emitter.instruction("jmp __rt_ssr_empty_addr_x86");                         // unsupported family: empty $address

    // -- AF_INET peer: hand the captured sockaddr to the IPv4 formatter --
    emitter.label("__rt_ssr_inet_x86");
    emitter.instruction("lea rdi, [rbp - 160]");                                // pointer to the captured sender sockaddr
    emitter.instruction("call __rt_format_sockaddr_in");                        // rax = address string, rdx = length
    emitter.instruction("jmp __rt_ssr_stash_addr_x86");                         // common stash + return

    // -- AF_UNIX peer: hand the captured sockaddr to the Unix formatter --
    emitter.label("__rt_ssr_unix_x86");
    emitter.instruction("lea rdi, [rbp - 160]");                                // pointer to the captured sender sockaddr
    emitter.instruction("movsxd rsi, DWORD PTR [rbp - 168]");                   // addrlen returned by recvfrom
    emitter.instruction("call __rt_format_sockaddr_unix");                      // rax = sun_path string, rdx = length
    emitter.instruction("jmp __rt_ssr_stash_addr_x86");                         // common stash + return

    // -- AF_INET6 peer: hand the captured sockaddr to the IPv6 formatter --
    emitter.label("__rt_ssr_inet6_x86");
    emitter.instruction("lea rdi, [rbp - 160]");                                // pointer to the captured sender sockaddr_in6
    emitter.instruction("call __rt_format_sockaddr_in6");                       // rax = address string, rdx = length
    emitter.instruction("jmp __rt_ssr_stash_addr_x86");                         // common stash + return

    // -- empty $address: connected socket, AF_INET6, or unsupported family --
    emitter.label("__rt_ssr_empty_addr_x86");
    emitter.instruction("mov rax, 1");                                          // alloc one byte so the heap header is well-formed
    emitter.instruction("call __rt_heap_alloc");                                // rax = single-byte buffer
    emitter.instruction(&format!(                                               // owned-string heap-kind word with the x86_64 heap marker
        "mov r10, 0x{:x}",
        (X86_64_HEAP_MAGIC_HI32 << 32) | 1
    ));
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the buffer as an owned string
    emitter.instruction("xor edx, edx");                                        // empty address length

    emitter.label("__rt_ssr_stash_addr_x86");
    abi::emit_symbol_address(emitter, "r10", "_recvfrom_addr_ptr");             // address of the stashed-pointer global
    emitter.instruction("mov QWORD PTR [r10], rax");                            // stash the sender address pointer
    abi::emit_symbol_address(emitter, "r10", "_recvfrom_addr_len");             // address of the stashed-length global
    emitter.instruction("mov QWORD PTR [r10], rdx");                            // stash the sender address length
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // owned buffer pointer becomes the result
    emitter.instruction("mov rdx, QWORD PTR [rbp - 176]");                      // received byte count becomes the length
    emitter.instruction("add rsp, 176");                                        // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the received string slice

    emitter.label("__rt_ssr_fail_x86");
    abi::emit_symbol_address(emitter, "r10", "_recvfrom_addr_ptr");             // address of the stashed-pointer global
    emitter.instruction("mov QWORD PTR [r10], 0");                              // clear the stashed sender address pointer
    abi::emit_symbol_address(emitter, "r10", "_recvfrom_addr_len");             // address of the stashed-length global
    emitter.instruction("mov QWORD PTR [r10], 0");                              // clear the stashed sender address length
    emitter.instruction("xor eax, eax");                                        // a null pointer signals a failed receive
    emitter.instruction("xor edx, edx");                                        // zero length for the failure case
    emitter.instruction("add rsp, 176");                                        // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the failure result
}
