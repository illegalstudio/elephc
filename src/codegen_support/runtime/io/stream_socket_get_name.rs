//! Purpose:
//! Emits the `__rt_stream_socket_get_name` runtime helper, which formats the
//! local or peer address of a socket as a PHP string.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - The buffer is large enough to hold a `sockaddr_un` (Unix-domain paths
//!   reach 108 bytes), `sockaddr_in`, or `sockaddr_in6`; the dispatch
//!   inspects the address family byte returned by `getsockname` /
//!   `getpeername` and branches to the matching formatter.
//! - AF_INET renders the packed IPv4 octets through `__rt_long2ip` and
//!   formats the port inline as `A.B.C.D:port`.
//! - AF_UNIX returns the `sun_path` string as-is (NUL-terminated), so PHP
//!   code sees the filesystem path the socket was bound to.
//! - Any other family (e.g. AF_INET6) currently returns a null pointer; the
//!   builtin boxes that as PHP false. IPv6 support is a follow-up.

use crate::codegen_support::{
    emit::Emitter,
    platform::{Arch, Platform},
};


/// Byte offset of the address-family discriminator inside a freshly populated
/// sockaddr buffer. macOS keeps a 1-byte `sa_len` ahead of the family byte
/// (BSD layout), while Linux puts the 16-bit family at offset 0 directly.
fn family_byte_offset(platform: Platform) -> u32 {
    match platform {
        Platform::MacOS => 1,
        Platform::Linux => 0,
        Platform::Windows => 0, // Windows WinSock sockaddr puts the 16-bit family at offset 0, same as Linux
    }
}

/// stream_socket_get_name: format a socket's local or peer address.
/// Input:  AArch64 x0 = fd, x1 = remote flag (0 = local, 1 = peer)
///         x86_64  rdi = fd, rsi = remote flag
/// Output: string pointer/length, or a null pointer on failure
pub fn emit_stream_socket_get_name(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stream_socket_get_name_linux_x86_64(emitter);
        return;
    }

    let plat = emitter.platform;
    let family_off = 16 + family_byte_offset(plat);
    emitter.blank();
    emitter.comment("--- runtime: stream_socket_get_name ---");
    emitter.label_global("__rt_stream_socket_get_name");

    // Frame (208 bytes):
    //   [0..16)    saved x29 / x30
    //   [16..144)  sockaddr buffer (fits sockaddr_un / sockaddr_in / sockaddr_in6)
    //   [144)      addrlen (in/out parameter, 4 bytes + padding)
    //   [152)      ip string pointer (inet path)
    //   [160)      ip string length  (inet path)
    //   [168)      port scratch slot (inet path, 6 bytes used, 8 reserved)
    //   [176)      port string pointer
    //   [184)      port string length
    emitter.instruction("stp x29, x30, [sp, #-208]!");                          // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    emitter.instruction("mov w9, #128");                                        // sockaddr buffer capacity
    emitter.instruction("str w9, [sp, #144]");                                  // addrlen in/out parameter = 128
    emitter.instruction("add x2, sp, #144");                                    // pointer to the addrlen parameter
    emitter.instruction("add x3, sp, #16");                                     // pointer to the sockaddr buffer
    emitter.instruction("cbz x1, __rt_ssgn_local");                             // a zero remote flag selects the local name

    // -- getpeername(fd, &sockaddr, &addrlen) --
    emitter.instruction("mov x1, x3");                                          // sockaddr pointer argument
    emitter.syscall(31);
    emitter.instruction("b __rt_ssgn_after");                                   // continue to the result check
    emitter.label("__rt_ssgn_local");

    // -- getsockname(fd, &sockaddr, &addrlen) --
    emitter.instruction("mov x1, x3");                                          // sockaddr pointer argument
    emitter.syscall(32);

    emitter.label("__rt_ssgn_after");
    if plat.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: a negative result means failure
    }
    emitter.instruction(&plat.branch_on_syscall_success("__rt_ssgn_ok"));       // continue when the syscall succeeded
    emitter.instruction("mov x1, #0");                                          // a null pointer signals a failed lookup
    emitter.instruction("mov x2, #0");                                          // zero length for the failure case
    emitter.instruction("ldp x29, x30, [sp], #208");                            // restore frame pointer and return address
    emitter.instruction("ret");                                                 // return the failure result

    emitter.label("__rt_ssgn_ok");
    // -- dispatch on the sockaddr's address family byte --
    let af_inet6 = plat.af_inet6();
    emitter.instruction(&format!("ldrb w9, [sp, #{}]", family_off));            // load the address-family discriminator
    emitter.instruction("cmp w9, #2");                                          // AF_INET = 2 on both platforms
    emitter.instruction("b.eq __rt_ssgn_inet");                                 // IPv4 path: A.B.C.D:port formatting
    emitter.instruction("cmp w9, #1");                                          // AF_UNIX = 1 on both platforms
    emitter.instruction("b.eq __rt_ssgn_unix");                                 // Unix-domain path: surface sun_path
    emitter.instruction(&format!("cmp w9, #{}", af_inet6));                     // AF_INET6 = 30 on macOS / 10 on Linux
    emitter.instruction("b.eq __rt_ssgn_inet6");                                // IPv6 path: [ipv6]:port formatting

    // -- unknown family: report failure to the caller --
    emitter.instruction("mov x1, #0");                                          // a null pointer signals an unsupported family
    emitter.instruction("mov x2, #0");                                          // zero length for the failure case
    emitter.instruction("ldp x29, x30, [sp], #208");                            // restore frame pointer and return address
    emitter.instruction("ret");                                                 // return the failure result

    // -- IPv6 formatting: delegate to __rt_format_sockaddr_in6 --
    emitter.label("__rt_ssgn_inet6");
    emitter.instruction("add x0, sp, #16");                                     // pointer to the captured sockaddr_in6
    emitter.instruction("bl __rt_format_sockaddr_in6");                         // x1 = address string, x2 = length
    emitter.instruction("ldp x29, x30, [sp], #208");                            // restore frame pointer and return address
    emitter.instruction("ret");                                                 // return the formatted IPv6 address string

    // -- IPv4 formatting --------------------------------------------------
    emitter.label("__rt_ssgn_inet");
    // -- extract the port (sockaddr_in offset 2, network byte order) --
    emitter.instruction("ldrb w9, [sp, #18]");                                  // port high byte
    emitter.instruction("ldrb w10, [sp, #19]");                                 // port low byte
    emitter.instruction("lsl w9, w9, #8");                                      // shift the high byte into place
    emitter.instruction("orr w9, w9, w10");                                     // w9 = host-order port

    // -- extract the packed IPv4 address (sockaddr_in offset 4) --
    emitter.instruction("ldrb w10, [sp, #20]");                                 // address octet 0
    emitter.instruction("lsl w10, w10, #24");                                   // octet 0 to the high byte
    emitter.instruction("ldrb w11, [sp, #21]");                                 // address octet 1
    emitter.instruction("lsl w11, w11, #16");                                   // octet 1 to the second byte
    emitter.instruction("orr w10, w10, w11");                                   // merge octet 1
    emitter.instruction("ldrb w11, [sp, #22]");                                 // address octet 2
    emitter.instruction("lsl w11, w11, #8");                                    // octet 2 to the third byte
    emitter.instruction("orr w10, w10, w11");                                   // merge octet 2
    emitter.instruction("ldrb w11, [sp, #23]");                                 // address octet 3
    emitter.instruction("orr w10, w10, w11");                                   // w10 = packed IPv4 address

    // -- format the port inline into the scratch slot, right-to-left --
    emitter.instruction("mov x12, #5");                                         // scratch cursor (six-byte window)
    emitter.instruction("mov x13, #10");                                        // decimal base
    emitter.label("__rt_ssgn_port");
    emitter.instruction("udiv x14, x9, x13");                                   // quotient = port / 10
    emitter.instruction("msub x15, x14, x13, x9");                              // remainder = port - quotient * 10
    emitter.instruction("add x15, x15, #48");                                   // remainder to an ASCII digit
    emitter.instruction("add x16, sp, #168");                                   // base of the port scratch slot
    emitter.instruction("strb w15, [x16, x12]");                                // store the digit at the cursor
    emitter.instruction("sub x12, x12, #1");                                    // move the cursor left
    emitter.instruction("mov x9, x14");                                         // port = port / 10
    emitter.instruction("cbnz x9, __rt_ssgn_port");                             // keep formatting until the port is zero
    emitter.instruction("mov x5, #5");                                          // last cursor index
    emitter.instruction("sub x5, x5, x12");                                     // port length = 5 - cursor
    emitter.instruction("add x12, x12, #1");                                    // cursor now points at the first digit
    emitter.instruction("add x4, sp, #168");                                    // base of the port scratch slot
    emitter.instruction("add x4, x4, x12");                                     // x4 = port string pointer
    emitter.instruction("str x4, [sp, #176]");                                  // save the port string pointer
    emitter.instruction("str x5, [sp, #184]");                                  // save the port string length

    // -- render the IPv4 address through long2ip --
    emitter.instruction("mov x0, x10");                                         // packed address argument
    emitter.instruction("bl __rt_long2ip");                                     // x1 = address string, x2 = length
    emitter.instruction("str x1, [sp, #152]");                                  // save the address string pointer
    emitter.instruction("str x2, [sp, #160]");                                  // save the address string length

    // -- allocate an owned heap string for "A.B.C.D:port" --
    emitter.instruction("ldr x3, [sp, #184]");                                  // port string length
    emitter.instruction("add x0, x2, x3");                                      // total length = ip + port
    emitter.instruction("add x0, x0, #1");                                      // plus the ':' separator
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the buffer, x0 = pointer
    emitter.instruction("mov x9, #1");                                          // heap kind 1 = persisted elephc string
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the buffer as an owned string

    // -- copy the address bytes into the buffer --
    emitter.instruction("ldr x4, [sp, #152]");                                  // address string pointer
    emitter.instruction("ldr x5, [sp, #160]");                                  // address string length
    emitter.instruction("mov x6, #0");                                          // copy index
    emitter.label("__rt_ssgn_copy_ip");
    emitter.instruction("cmp x6, x5");                                          // copied every address byte?
    emitter.instruction("b.hs __rt_ssgn_copy_ip_done");                         // address copy complete
    emitter.instruction("ldrb w7, [x4, x6]");                                   // load an address byte
    emitter.instruction("strb w7, [x0, x6]");                                   // store it into the buffer
    emitter.instruction("add x6, x6, #1");                                      // advance the copy index
    emitter.instruction("b __rt_ssgn_copy_ip");                                 // keep copying the address
    emitter.label("__rt_ssgn_copy_ip_done");

    // -- write the ':' separator, then copy the port bytes --
    emitter.instruction("mov w7, #58");                                         // ASCII ':' separator
    emitter.instruction("strb w7, [x0, x5]");                                   // write the separator after the address
    emitter.instruction("add x8, x5, #1");                                      // destination offset after "A.B.C.D:"
    emitter.instruction("ldr x4, [sp, #176]");                                  // port string pointer
    emitter.instruction("ldr x5, [sp, #184]");                                  // port string length
    emitter.instruction("mov x6, #0");                                          // copy index
    emitter.label("__rt_ssgn_copy_port");
    emitter.instruction("cmp x6, x5");                                          // copied every port byte?
    emitter.instruction("b.hs __rt_ssgn_copy_port_done");                       // port copy complete
    emitter.instruction("ldrb w7, [x4, x6]");                                   // load a port byte
    emitter.instruction("add x9, x8, x6");                                      // destination index past the separator
    emitter.instruction("strb w7, [x0, x9]");                                   // store it into the buffer
    emitter.instruction("add x6, x6, #1");                                      // advance the copy index
    emitter.instruction("b __rt_ssgn_copy_port");                               // keep copying the port
    emitter.label("__rt_ssgn_copy_port_done");

    // -- return the buffer pointer and total length --
    emitter.instruction("mov x1, x0");                                          // result string pointer
    emitter.instruction("ldr x9, [sp, #160]");                                  // address string length
    emitter.instruction("ldr x10, [sp, #184]");                                 // port string length
    emitter.instruction("add x2, x9, x10");                                     // total length = ip + port
    emitter.instruction("add x2, x2, #1");                                      // plus the ':' separator
    emitter.instruction("ldp x29, x30, [sp], #208");                            // restore frame pointer and return address
    emitter.instruction("ret");                                                 // return the formatted address string

    // -- Unix-domain formatting -------------------------------------------
    emitter.label("__rt_ssgn_unix");
    // sun_path lives at buffer offset 2 on both Linux (`sa_family_t` is two
    // bytes) and macOS (`sun_len` + `sun_family` are one byte each).
    emitter.instruction("add x10, sp, #18");                                    // sun_path pointer
    emitter.instruction("ldr w13, [sp, #144]");                                 // addrlen returned by the syscall
    emitter.instruction("subs w13, w13, #2");                                   // sun_path region length (addrlen - sa_family bytes)
    emitter.instruction("b.le __rt_ssgn_unix_unnamed");                         // unbound socket: addrlen<=2 → empty path

    // -- find the NUL terminator (or stop at the addrlen-derived max) --
    emitter.instruction("mov x11, #0");                                         // sun_path length accumulator
    emitter.label("__rt_ssgn_unix_scan");
    emitter.instruction("cmp x11, x13");                                        // hit the addrlen-derived upper bound?
    emitter.instruction("b.hs __rt_ssgn_unix_scan_done");                       // stop when we exhaust the reported bytes
    emitter.instruction("ldrb w14, [x10, x11]");                                // peek at the next sun_path byte
    emitter.instruction("cbz w14, __rt_ssgn_unix_scan_done");                   // stop at the C-string terminator
    emitter.instruction("add x11, x11, #1");                                    // advance the cursor
    emitter.instruction("b __rt_ssgn_unix_scan");                               // keep scanning the path bytes
    emitter.label("__rt_ssgn_unix_scan_done");

    // -- allocate an owned heap string for the path --
    emitter.instruction("str x11, [sp, #184]");                                 // save the sun_path length across the alloc
    emitter.instruction("str x10, [sp, #176]");                                 // save the sun_path pointer across the alloc
    emitter.instruction("mov x0, x11");                                         // alloc size = sun_path length
    emitter.instruction("cmp x0, #0");                                          // empty path needs at least one byte for the heap header layout
    emitter.instruction("csel x0, x0, xzr, ne");                                // keep zero allocations zero
    emitter.instruction("cmp x0, #0");                                          // ensure we still ask heap_alloc for a slot
    emitter.instruction("b.ne __rt_ssgn_unix_alloc");                           // alloc the real path bytes
    emitter.instruction("mov x0, #1");                                          // empty path: alloc a single-byte slot so the heap header is valid
    emitter.label("__rt_ssgn_unix_alloc");
    emitter.instruction("bl __rt_heap_alloc");                                  // x0 = persisted-string buffer pointer
    emitter.instruction("mov x9, #1");                                          // heap kind 1 = persisted elephc string
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the buffer as an owned string

    // -- copy sun_path into the heap string --
    emitter.instruction("ldr x10, [sp, #176]");                                 // reload the sun_path pointer
    emitter.instruction("ldr x11, [sp, #184]");                                 // reload the sun_path length
    emitter.instruction("mov x6, #0");                                          // copy index
    emitter.label("__rt_ssgn_unix_copy");
    emitter.instruction("cmp x6, x11");                                         // copied every path byte?
    emitter.instruction("b.hs __rt_ssgn_unix_copy_done");                       // path copy complete
    emitter.instruction("ldrb w7, [x10, x6]");                                  // load a path byte
    emitter.instruction("strb w7, [x0, x6]");                                   // store it into the buffer
    emitter.instruction("add x6, x6, #1");                                      // advance the copy index
    emitter.instruction("b __rt_ssgn_unix_copy");                               // keep copying the path
    emitter.label("__rt_ssgn_unix_copy_done");

    emitter.instruction("mov x1, x0");                                          // result string pointer
    emitter.instruction("mov x2, x11");                                         // result string length = sun_path length
    emitter.instruction("ldp x29, x30, [sp], #208");                            // restore frame pointer and return address
    emitter.instruction("ret");                                                 // return the sun_path string

    emitter.label("__rt_ssgn_unix_unnamed");
    // -- unbound Unix socket: return an empty heap-allocated string --
    emitter.instruction("mov x0, #1");                                          // alloc one byte so the heap header is well-formed
    emitter.instruction("bl __rt_heap_alloc");                                  // x0 = single-byte buffer
    emitter.instruction("mov x9, #1");                                          // heap kind 1 = persisted elephc string
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the buffer as an owned string
    emitter.instruction("mov x1, x0");                                          // result string pointer
    emitter.instruction("mov x2, #0");                                          // result length = 0 (empty string)
    emitter.instruction("ldp x29, x30, [sp], #208");                            // restore frame pointer and return address
    emitter.instruction("ret");                                                 // return the empty path string
}

/// Emits the Linux x86_64 stream runtime helper for stream socket get name.
fn emit_stream_socket_get_name_linux_x86_64(emitter: &mut Emitter) {
    let family_off_in_buffer = family_byte_offset(Platform::Linux);
    emitter.blank();
    emitter.comment("--- runtime: stream_socket_get_name ---");
    emitter.label_global("__rt_stream_socket_get_name");

    // Frame (208 bytes, rbp-relative):
    //   [rbp-128..rbp)   sockaddr buffer (fits sockaddr_un / sockaddr_in / sockaddr_in6)
    //   [rbp-136)        addrlen (in/out parameter)
    //   [rbp-144)        ip string pointer (inet path)
    //   [rbp-152)        ip string length  (inet path)
    //   [rbp-160)        port scratch slot (inet path)
    //   [rbp-168)        port string pointer
    //   [rbp-176)        port string length
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 208");                                        // frame for the sockaddr buffer and the format state
    emitter.instruction("mov DWORD PTR [rbp - 136], 128");                      // addrlen in/out parameter = 128
    emitter.instruction("test rsi, rsi");                                       // examine the remote flag
    emitter.instruction("jnz __rt_ssgn_peer_x86");                              // a non-zero remote flag selects the peer name

    // -- getsockname(fd, &sockaddr, &addrlen) --
    emitter.instruction("lea rsi, [rbp - 128]");                                // sockaddr pointer argument
    emitter.instruction("lea rdx, [rbp - 136]");                                // addrlen pointer argument
    emitter.instruction("mov eax, 51");                                         // Linux x86_64 syscall 51 = getsockname
    emitter.instruction("syscall");                                             // read the local socket address
    emitter.instruction("jmp __rt_ssgn_after_x86");                             // continue to the result check
    emitter.label("__rt_ssgn_peer_x86");

    // -- getpeername(fd, &sockaddr, &addrlen) --
    emitter.instruction("lea rsi, [rbp - 128]");                                // sockaddr pointer argument
    emitter.instruction("lea rdx, [rbp - 136]");                                // addrlen pointer argument
    emitter.instruction("mov eax, 52");                                         // Linux x86_64 syscall 52 = getpeername
    emitter.instruction("syscall");                                             // read the peer socket address

    emitter.label("__rt_ssgn_after_x86");
    emitter.instruction("cmp rax, 0");                                          // did the syscall fail?
    emitter.instruction("jl __rt_ssgn_fail_x86");                               // a negative result means failure

    // -- dispatch on the address-family byte (Linux: low byte of sa_family) --
    let family_addr_disp: i64 = -128 + family_off_in_buffer as i64;
    emitter.instruction(&format!(                                               // read the address-family discriminator
        "movzx eax, BYTE PTR [rbp - {}]",
        -family_addr_disp
    ));
    let af_inet6 = Platform::Linux.af_inet6();
    emitter.instruction("cmp eax, 2");                                          // AF_INET = 2 on Linux
    emitter.instruction("je __rt_ssgn_inet_x86");                               // IPv4 path: A.B.C.D:port formatting
    emitter.instruction("cmp eax, 1");                                          // AF_UNIX = 1 on Linux
    emitter.instruction("je __rt_ssgn_unix_x86");                               // Unix-domain path: surface sun_path
    emitter.instruction(&format!("cmp eax, {}", af_inet6));                     // AF_INET6 = 10 on Linux
    emitter.instruction("je __rt_ssgn_inet6_x86");                              // IPv6 path: [ipv6]:port formatting
    emitter.instruction("jmp __rt_ssgn_fail_x86");                              // unsupported family → null pointer

    // -- IPv6 formatting: delegate to __rt_format_sockaddr_in6 --
    emitter.label("__rt_ssgn_inet6_x86");
    emitter.instruction("lea rdi, [rbp - 128]");                                // pointer to the captured sockaddr_in6
    emitter.instruction("call __rt_format_sockaddr_in6");                       // rax = address string, rdx = length
    emitter.instruction("add rsp, 208");                                        // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the formatted IPv6 address string

    // -- IPv4 formatting --------------------------------------------------
    emitter.label("__rt_ssgn_inet_x86");
    // -- extract the port (sockaddr_in offset 2, network byte order) --
    emitter.instruction("movzx r8d, BYTE PTR [rbp - 126]");                     // port high byte
    emitter.instruction("movzx r9d, BYTE PTR [rbp - 125]");                     // port low byte
    emitter.instruction("shl r8d, 8");                                          // shift the high byte into place
    emitter.instruction("or r8d, r9d");                                         // r8 = host-order port

    // -- extract the packed IPv4 address (sockaddr_in offset 4) --
    emitter.instruction("movzx r9d, BYTE PTR [rbp - 124]");                     // address octet 0
    emitter.instruction("shl r9d, 24");                                         // octet 0 to the high byte
    emitter.instruction("movzx r10d, BYTE PTR [rbp - 123]");                    // address octet 1
    emitter.instruction("shl r10d, 16");                                        // octet 1 to the second byte
    emitter.instruction("or r9d, r10d");                                        // merge octet 1
    emitter.instruction("movzx r10d, BYTE PTR [rbp - 122]");                    // address octet 2
    emitter.instruction("shl r10d, 8");                                         // octet 2 to the third byte
    emitter.instruction("or r9d, r10d");                                        // merge octet 2
    emitter.instruction("movzx r10d, BYTE PTR [rbp - 121]");                    // address octet 3
    emitter.instruction("or r9d, r10d");                                        // r9 = packed IPv4 address

    // -- format the port inline into the scratch slot, right-to-left --
    emitter.instruction("mov rcx, 5");                                          // scratch cursor (six-byte window)
    emitter.label("__rt_ssgn_port_x86");
    emitter.instruction("mov rax, r8");                                         // current port value
    emitter.instruction("xor edx, edx");                                        // clear the division high word
    emitter.instruction("mov r11, 10");                                         // decimal base divisor
    emitter.instruction("div r11");                                             // rax = port / 10, rdx = port % 10
    emitter.instruction("add dl, 48");                                          // remainder to an ASCII digit
    emitter.instruction("lea r11, [rbp - 160]");                                // base of the port scratch slot
    emitter.instruction("mov BYTE PTR [r11 + rcx], dl");                        // store the digit at the cursor
    emitter.instruction("dec rcx");                                             // move the cursor left
    emitter.instruction("mov r8, rax");                                         // port = port / 10
    emitter.instruction("test r8, r8");                                         // is the port now zero?
    emitter.instruction("jnz __rt_ssgn_port_x86");                              // keep formatting until the port is zero
    emitter.instruction("mov rax, 5");                                          // last cursor index
    emitter.instruction("sub rax, rcx");                                        // port length = 5 - cursor
    emitter.instruction("mov QWORD PTR [rbp - 176], rax");                      // save the port string length
    emitter.instruction("inc rcx");                                             // cursor now points at the first digit
    emitter.instruction("lea r11, [rbp - 160]");                                // base of the port scratch slot
    emitter.instruction("add r11, rcx");                                        // r11 = port string pointer
    emitter.instruction("mov QWORD PTR [rbp - 168], r11");                      // save the port string pointer

    // -- render the IPv4 address through long2ip --
    emitter.instruction("mov rdi, r9");                                         // packed address argument
    emitter.instruction("call __rt_long2ip");                                   // rax = address string, rdx = length
    emitter.instruction("mov QWORD PTR [rbp - 144], rax");                      // save the address string pointer
    emitter.instruction("mov QWORD PTR [rbp - 152], rdx");                      // save the address string length

    // -- allocate an owned heap string for "A.B.C.D:port" --
    emitter.instruction("mov rax, rdx");                                        // address string length
    emitter.instruction("add rax, QWORD PTR [rbp - 176]");                      // plus the port string length
    emitter.instruction("add rax, 1");                                          // plus the ':' separator
    emitter.instruction("call __rt_heap_alloc");                                // allocate the buffer, rax = pointer
    emitter.instruction(&format!(                                               // owned-string heap-kind word with the x86_64 heap marker
        "mov r10, 0x{:x}",
        crate::codegen_support::sentinels::x86_64_heap_kind_word(1)
    ));
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the buffer as an owned string

    // -- copy the address bytes into the buffer --
    emitter.instruction("mov r8, QWORD PTR [rbp - 144]");                       // address string pointer
    emitter.instruction("mov r9, QWORD PTR [rbp - 152]");                       // address string length
    emitter.instruction("xor rcx, rcx");                                        // copy index
    emitter.label("__rt_ssgn_copy_ip_x86");
    emitter.instruction("cmp rcx, r9");                                         // copied every address byte?
    emitter.instruction("jae __rt_ssgn_copy_ip_done_x86");                      // address copy complete
    emitter.instruction("movzx edx, BYTE PTR [r8 + rcx]");                      // load an address byte
    emitter.instruction("mov BYTE PTR [rax + rcx], dl");                        // store it into the buffer
    emitter.instruction("inc rcx");                                             // advance the copy index
    emitter.instruction("jmp __rt_ssgn_copy_ip_x86");                           // keep copying the address
    emitter.label("__rt_ssgn_copy_ip_done_x86");

    // -- write the ':' separator, then copy the port bytes --
    emitter.instruction("mov BYTE PTR [rax + r9], 58");                         // write ':' after the address
    emitter.instruction("mov r8, QWORD PTR [rbp - 168]");                       // port string pointer
    emitter.instruction("mov r11, QWORD PTR [rbp - 176]");                      // port string length
    emitter.instruction("xor rcx, rcx");                                        // copy index
    emitter.label("__rt_ssgn_copy_port_x86");
    emitter.instruction("cmp rcx, r11");                                        // copied every port byte?
    emitter.instruction("jae __rt_ssgn_copy_port_done_x86");                    // port copy complete
    emitter.instruction("movzx edx, BYTE PTR [r8 + rcx]");                      // load a port byte
    emitter.instruction("mov rdi, r9");                                         // address string length
    emitter.instruction("add rdi, 1");                                          // plus the ':' separator
    emitter.instruction("add rdi, rcx");                                        // destination index in the buffer
    emitter.instruction("mov BYTE PTR [rax + rdi], dl");                        // store it into the buffer
    emitter.instruction("inc rcx");                                             // advance the copy index
    emitter.instruction("jmp __rt_ssgn_copy_port_x86");                         // keep copying the port
    emitter.label("__rt_ssgn_copy_port_done_x86");

    // -- return the buffer pointer and total length --
    emitter.instruction("mov rdx, r9");                                         // address string length
    emitter.instruction("add rdx, 1");                                          // plus the ':' separator
    emitter.instruction("add rdx, r11");                                        // plus the port string length
    emitter.instruction("add rsp, 208");                                        // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the formatted address string

    // -- Unix-domain formatting -------------------------------------------
    emitter.label("__rt_ssgn_unix_x86");
    // sun_path starts at buffer offset 2 (sa_family_t is two bytes on Linux).
    emitter.instruction("lea r8, [rbp - 126]");                                 // sun_path pointer
    emitter.instruction("movsxd r9, DWORD PTR [rbp - 136]");                    // addrlen returned by the syscall (signed widen)
    emitter.instruction("sub r9, 2");                                           // sun_path region length (addrlen - sa_family bytes)
    emitter.instruction("jle __rt_ssgn_unix_unnamed_x86");                      // unbound socket: addrlen<=2 → empty path

    // -- find the NUL terminator (or stop at the addrlen-derived max) --
    emitter.instruction("xor rcx, rcx");                                        // sun_path length accumulator
    emitter.label("__rt_ssgn_unix_scan_x86");
    emitter.instruction("cmp rcx, r9");                                         // hit the addrlen-derived upper bound?
    emitter.instruction("jae __rt_ssgn_unix_scan_done_x86");                    // stop when we exhaust the reported bytes
    emitter.instruction("movzx edx, BYTE PTR [r8 + rcx]");                      // peek at the next sun_path byte
    emitter.instruction("test dl, dl");                                         // is it the C-string terminator?
    emitter.instruction("je __rt_ssgn_unix_scan_done_x86");                     // stop at the NUL byte
    emitter.instruction("inc rcx");                                             // advance the cursor
    emitter.instruction("jmp __rt_ssgn_unix_scan_x86");                         // keep scanning the path bytes
    emitter.label("__rt_ssgn_unix_scan_done_x86");

    // -- allocate an owned heap string for the path --
    emitter.instruction("mov QWORD PTR [rbp - 176], rcx");                      // save the sun_path length across the alloc
    emitter.instruction("mov QWORD PTR [rbp - 168], r8");                       // save the sun_path pointer across the alloc
    emitter.instruction("mov rax, rcx");                                        // alloc size = sun_path length
    emitter.instruction("test rax, rax");                                       // empty path?
    emitter.instruction("jnz __rt_ssgn_unix_alloc_x86");                        // non-empty: allocate the actual length
    emitter.instruction("mov rax, 1");                                          // empty path: alloc one byte so the heap header is valid
    emitter.label("__rt_ssgn_unix_alloc_x86");
    emitter.instruction("call __rt_heap_alloc");                                // rax = persisted-string buffer pointer
    emitter.instruction(&format!(                                               // owned-string heap-kind word with the x86_64 heap marker
        "mov r10, 0x{:x}",
        crate::codegen_support::sentinels::x86_64_heap_kind_word(1)
    ));
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the buffer as an owned string

    // -- copy sun_path into the heap string --
    emitter.instruction("mov r8, QWORD PTR [rbp - 168]");                       // reload the sun_path pointer
    emitter.instruction("mov r9, QWORD PTR [rbp - 176]");                       // reload the sun_path length
    emitter.instruction("xor rcx, rcx");                                        // copy index
    emitter.label("__rt_ssgn_unix_copy_x86");
    emitter.instruction("cmp rcx, r9");                                         // copied every path byte?
    emitter.instruction("jae __rt_ssgn_unix_copy_done_x86");                    // path copy complete
    emitter.instruction("movzx edx, BYTE PTR [r8 + rcx]");                      // load a path byte
    emitter.instruction("mov BYTE PTR [rax + rcx], dl");                        // store it into the buffer
    emitter.instruction("inc rcx");                                             // advance the copy index
    emitter.instruction("jmp __rt_ssgn_unix_copy_x86");                         // keep copying the path
    emitter.label("__rt_ssgn_unix_copy_done_x86");

    emitter.instruction("mov rdx, r9");                                         // result string length = sun_path length
    emitter.instruction("add rsp, 208");                                        // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the sun_path string

    emitter.label("__rt_ssgn_unix_unnamed_x86");
    // -- unbound Unix socket: return an empty heap-allocated string --
    emitter.instruction("mov rax, 1");                                          // alloc one byte so the heap header is well-formed
    emitter.instruction("call __rt_heap_alloc");                                // rax = single-byte buffer
    emitter.instruction(&format!(                                               // owned-string heap-kind word with the x86_64 heap marker
        "mov r10, 0x{:x}",
        crate::codegen_support::sentinels::x86_64_heap_kind_word(1)
    ));
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the buffer as an owned string
    emitter.instruction("xor edx, edx");                                        // result length = 0 (empty string)
    emitter.instruction("add rsp, 208");                                        // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the empty path string

    emitter.label("__rt_ssgn_fail_x86");
    emitter.instruction("xor eax, eax");                                        // a null pointer signals a failed lookup
    emitter.instruction("xor edx, edx");                                        // zero length for the failure case
    emitter.instruction("add rsp, 208");                                        // release the frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the failure result
}
