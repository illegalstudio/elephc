//! Purpose:
//! Emits the `__rt_stream_socket_accept` runtime helper, which accepts a
//! pending connection on a listening socket and captures the peer address
//! for the optional `$peer_name` out-parameter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::io`.
//! - The `stream_socket_accept` builtin emitter, which threads in the
//!   timeout (in microseconds; `-1` = infinite wait) and reads the stashed
//!   peer address out of `_accept_peer_ptr` / `_accept_peer_len` when the
//!   user passed a `&$peer_name` variable.
//!
//! Key details:
//! - When `timeout_us >= 0` the helper gates `accept()` behind a single-fd
//!   `select()` (macOS) / `pselect6()` (Linux). A timeout (`select` returns
//!   0) lowers to a -1 result that the builtin boxes as PHP `false`.
//! - The captured `sockaddr` is dispatched on its family byte and rendered
//!   through `__rt_format_sockaddr_in` (IPv4) or
//!   `__rt_format_sockaddr_unix` (Unix-domain). Anything else (AF_INET6
//!   today, future families) lowers to an empty string. A failed accept
//!   clears both globals.

use crate::codegen::{
    abi,
    emit::Emitter,
    platform::{Arch, Platform},
};

/// Byte offset of the address-family discriminator inside a freshly populated
/// sockaddr buffer; macOS keeps a 1-byte `sa_len` ahead of the family byte
/// (BSD layout), while Linux puts the 16-bit family at offset 0 directly.
fn family_byte_offset(platform: Platform) -> i64 {
    match platform {
        Platform::MacOS => 1,
        Platform::Linux => 0,
    }
}

/// stream_socket_accept: accept a pending connection on a listening socket,
/// optionally gated by a microsecond timeout, capturing the peer address.
/// Input:  AArch64 x0 = listening fd, x1 = timeout_us (-1 = infinite)
///         x86_64  rdi = listening fd, rsi = timeout_us (-1 = infinite)
/// Output: accepted descriptor, or -1 on failure / timeout
pub fn emit_stream_socket_accept(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stream_socket_accept_linux_x86_64(emitter);
        return;
    }

    let plat = emitter.platform;
    let linux = plat == Platform::Linux;
    let family_off = 32 + family_byte_offset(plat);
    emitter.blank();
    emitter.comment("--- runtime: stream_socket_accept ---");
    emitter.label_global("__rt_stream_socket_accept");

    // Frame (224 bytes):
    //   [0..16)    saved x29 / x30
    //   [16)       saved fd
    //   [24)       saved timeout_us
    //   [32..160)  sockaddr buffer (sockaddr_in / sockaddr_un / sockaddr_in6 fit)
    //   [160)      addrlen (4 + 4 pad)
    //   [168)      fd_set word 0 (covers fds 0..63)
    //   [176)      fd_set word 1 (covers fds 64..127)
    //   [184)      timeout struct: seconds field
    //   [192)      timeout struct: micro/nanoseconds field
    //   [200)      accepted fd
    //   [208..224) padding for 16-byte alignment
    emitter.instruction("sub sp, sp, #224");                                    // allocate the helper frame
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("add x29, sp, #0");                                     // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #16]");                                   // save the listening fd
    emitter.instruction("str x1, [sp, #24]");                                   // save the timeout in microseconds

    // -- timeout < 0 skips the select gate and blocks indefinitely --
    emitter.instruction("cmp x1, #0");                                          // is the timeout negative (infinite)?
    emitter.instruction("b.lt __rt_ssa_blocking_accept");                       // jump straight to accept for the infinite case

    // -- prepare the fd_set: zero both words, then set the bit for our fd --
    emitter.instruction("str xzr, [sp, #168]");                                 // clear fd_set word 0
    emitter.instruction("str xzr, [sp, #176]");                                 // clear fd_set word 1
    emitter.instruction("cmp x0, #128");                                        // fds beyond 127 fall outside the two-word bitmap
    emitter.instruction("b.ge __rt_ssa_fail_clear");                            // out-of-range fd: bail out
    emitter.instruction("mov x9, x0");                                          // descriptor index for the bit position
    emitter.instruction("cmp x9, #64");                                         // does the descriptor fall into word 0?
    emitter.instruction("b.lt __rt_ssa_set_word0");                             // descriptor < 64: set the bit in word 0
    emitter.instruction("sub x9, x9, #64");                                     // subtract 64 to address word 1
    emitter.instruction("mov x10, #1");                                         // bit seed for word 1
    emitter.instruction("lsl x10, x10, x9");                                    // shift the bit into the descriptor position
    emitter.instruction("str x10, [sp, #176]");                                 // set the descriptor bit in word 1
    emitter.instruction("b __rt_ssa_fdset_done");                               // skip word-0 setup
    emitter.label("__rt_ssa_set_word0");
    emitter.instruction("mov x10, #1");                                         // bit seed for word 0
    emitter.instruction("lsl x10, x10, x9");                                    // shift the bit into the descriptor position
    emitter.instruction("str x10, [sp, #168]");                                 // set the descriptor bit in word 0
    emitter.label("__rt_ssa_fdset_done");

    // -- build the timeout struct from timeout_us --
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the timeout in microseconds
    emitter.instruction("mov x10, #0x4240");                                    // low 16 bits of 1_000_000 (0xF4240)
    emitter.instruction("movk x10, #0xF, lsl #16");                             // upper bits make x10 = 1_000_000 (one second in us)
    emitter.instruction("udiv x11, x9, x10");                                   // seconds = timeout_us / 1_000_000
    emitter.instruction("msub x12, x11, x10, x9");                              // micro_remainder = timeout_us - seconds*1_000_000
    emitter.instruction("str x11, [sp, #184]");                                 // store the timeout seconds field
    if linux {
        emitter.instruction("mov x10, #1000");                                  // a microsecond holds 1000 nanoseconds
        emitter.instruction("mul x12, x12, x10");                               // convert microseconds to nanoseconds for timespec
    }
    emitter.instruction("str x12, [sp, #192]");                                 // store the timeout fractional field

    // -- select(nfds=128, readfds=&fd_set, writefds=NULL, exceptfds=NULL, &timeout) --
    emitter.instruction("mov x0, #128");                                        // examine descriptors 0..127
    emitter.instruction("add x1, sp, #168");                                    // pointer to the readfds fd_set
    emitter.instruction("mov x2, #0");                                          // no writefds
    emitter.instruction("mov x3, #0");                                          // no exceptfds
    emitter.instruction("add x4, sp, #184");                                    // pointer to the timeout struct
    emitter.instruction("mov x5, #0");                                          // signal mask pointer (NULL); ignored by macOS select
    emitter.syscall(93);
    emitter.instruction("cmp x0, #0");                                          // did select fail or time out?
    emitter.instruction("b.le __rt_ssa_fail_clear");                            // 0 = timeout, <0 = error → report failure

    emitter.label("__rt_ssa_blocking_accept");
    // -- accept(fd, &sockaddr, &addrlen) --
    emitter.instruction("mov w9, #128");                                        // sockaddr buffer capacity
    emitter.instruction("str w9, [sp, #160]");                                  // addrlen in/out parameter = 128
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the listening fd
    emitter.instruction("add x1, sp, #32");                                     // pointer to the peer sockaddr buffer
    emitter.instruction("add x2, sp, #160");                                    // pointer to the addrlen parameter
    emitter.syscall(30);
    if plat.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: a negative descriptor means failure
    }
    emitter.instruction(&plat.branch_on_syscall_success("__rt_ssa_accept_ok")); // continue when accept succeeded
    emitter.instruction("b __rt_ssa_fail_clear");                               // accept() failed: clear globals and report -1

    emitter.label("__rt_ssa_accept_ok");
    emitter.instruction("str x0, [sp, #200]");                                  // save the accepted fd

    // -- dispatch on the captured sockaddr's address family --
    emitter.instruction("ldr w9, [sp, #160]");                                  // reload addrlen returned by accept
    emitter.instruction("cmp w9, #2");                                          // is there at least a family-byte's worth of data?
    emitter.instruction("b.lt __rt_ssa_empty_peer");                            // no peer info: empty $peer_name
    let af_inet6 = plat.af_inet6();
    emitter.instruction(&format!("ldrb w10, [sp, #{}]", family_off));           // load the address-family discriminator
    emitter.instruction("cmp w10, #2");                                         // AF_INET = 2 on both platforms
    emitter.instruction("b.eq __rt_ssa_peer_inet");                             // IPv4 peer: format A.B.C.D:port
    emitter.instruction("cmp w10, #1");                                         // AF_UNIX = 1 on both platforms
    emitter.instruction("b.eq __rt_ssa_peer_unix");                             // Unix-domain peer: format sun_path
    emitter.instruction(&format!("cmp w10, #{}", af_inet6));                    // AF_INET6 = 30 on macOS / 10 on Linux
    emitter.instruction("b.eq __rt_ssa_peer_inet6");                            // IPv6 peer: format [ipv6]:port
    emitter.instruction("b __rt_ssa_empty_peer");                               // unsupported family: empty $peer_name

    // -- AF_INET peer: render via __rt_format_sockaddr_in --
    emitter.label("__rt_ssa_peer_inet");
    emitter.instruction("add x0, sp, #32");                                     // pointer to the captured peer sockaddr
    emitter.instruction("bl __rt_format_sockaddr_in");                          // x1 = address string, x2 = length
    emitter.instruction("b __rt_ssa_stash_peer");                               // common stash + return

    // -- AF_UNIX peer: render via __rt_format_sockaddr_unix --
    emitter.label("__rt_ssa_peer_unix");
    emitter.instruction("add x0, sp, #32");                                     // pointer to the captured peer sockaddr
    emitter.instruction("ldr w1, [sp, #160]");                                  // addrlen returned by accept
    emitter.instruction("bl __rt_format_sockaddr_unix");                        // x1 = sun_path string, x2 = length
    emitter.instruction("b __rt_ssa_stash_peer");                               // common stash + return

    // -- AF_INET6 peer: render via __rt_format_sockaddr_in6 --
    emitter.label("__rt_ssa_peer_inet6");
    emitter.instruction("add x0, sp, #32");                                     // pointer to the captured peer sockaddr_in6
    emitter.instruction("bl __rt_format_sockaddr_in6");                         // x1 = address string, x2 = length
    emitter.instruction("b __rt_ssa_stash_peer");                               // common stash + return

    // -- empty $peer_name: connected/unnamed peer or unsupported family --
    emitter.label("__rt_ssa_empty_peer");
    emitter.instruction("mov x0, #1");                                          // alloc one byte so the heap header is well-formed
    emitter.instruction("bl __rt_heap_alloc");                                  // x0 = single-byte buffer
    emitter.instruction("mov x9, #1");                                          // heap kind 1 = persisted elephc string
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the buffer as an owned string
    emitter.instruction("mov x1, x0");                                          // empty address pointer
    emitter.instruction("mov x2, #0");                                          // empty address length

    emitter.label("__rt_ssa_stash_peer");
    abi::emit_symbol_address(emitter, "x9", "_accept_peer_ptr");
    emitter.instruction("str x1, [x9]");                                        // stash the peer address pointer
    abi::emit_symbol_address(emitter, "x9", "_accept_peer_len");
    emitter.instruction("str x2, [x9]");                                        // stash the peer address length
    emitter.instruction("ldr x0, [sp, #200]");                                  // return the accepted descriptor
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #224");                                    // release the helper frame
    emitter.instruction("ret");                                                 // return the accepted fd

    emitter.label("__rt_ssa_fail_clear");
    abi::emit_symbol_address(emitter, "x9", "_accept_peer_ptr");
    emitter.instruction("str xzr, [x9]");                                       // clear the stashed peer address pointer
    abi::emit_symbol_address(emitter, "x9", "_accept_peer_len");
    emitter.instruction("str xzr, [x9]");                                       // clear the stashed peer address length
    emitter.instruction("mov x0, #-1");                                         // -1 reports a failed / timed-out accept
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #224");                                    // release the helper frame
    emitter.instruction("ret");                                                 // return the failure result
}

fn emit_stream_socket_accept_linux_x86_64(emitter: &mut Emitter) {
    let family_off = family_byte_offset(Platform::Linux);
    emitter.blank();
    emitter.comment("--- runtime: stream_socket_accept ---");
    emitter.label_global("__rt_stream_socket_accept");

    // Frame (rbp-relative):
    //   [-8]   saved fd
    //   [-16]  saved timeout_us
    //   [-144) sockaddr buffer (128 bytes)
    //   [-152) addrlen (4 + 4 pad)
    //   [-160) fd_set word 0 (covers fds 0..63)
    //   [-168) fd_set word 1 (covers fds 64..127)
    //   [-176) timeout struct: seconds field
    //   [-184) timeout struct: nanoseconds field
    //   [-192) accepted fd
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 192");                                        // allocate the helper frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the listening fd
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the timeout in microseconds

    // -- timeout < 0 skips the select gate and blocks indefinitely --
    emitter.instruction("cmp rsi, 0");                                          // is the timeout negative (infinite)?
    emitter.instruction("jl __rt_ssa_blocking_accept_x86");                     // jump straight to accept for the infinite case

    // -- prepare the fd_set: zero both words, then set the bit for our fd --
    emitter.instruction("mov QWORD PTR [rbp - 160], 0");                        // clear fd_set word 0
    emitter.instruction("mov QWORD PTR [rbp - 168], 0");                        // clear fd_set word 1
    emitter.instruction("cmp rdi, 128");                                        // fds beyond 127 fall outside the two-word bitmap
    emitter.instruction("jae __rt_ssa_fail_clear_x86");                         // out-of-range fd: bail out
    emitter.instruction("cmp rdi, 64");                                         // does the descriptor fall into word 0?
    emitter.instruction("jge __rt_ssa_set_word1_x86");                          // descriptor >= 64: set the bit in word 1
    emitter.instruction("mov rcx, rdi");                                        // descriptor index for the shift count
    emitter.instruction("mov rax, 1");                                          // bit seed
    emitter.instruction("shl rax, cl");                                         // shift the bit into the descriptor position
    emitter.instruction("mov QWORD PTR [rbp - 160], rax");                      // set the descriptor bit in word 0
    emitter.instruction("jmp __rt_ssa_fdset_done_x86");                         // skip word-1 setup
    emitter.label("__rt_ssa_set_word1_x86");
    emitter.instruction("mov rcx, rdi");                                        // descriptor index for the shift count
    emitter.instruction("sub rcx, 64");                                         // subtract 64 to address word 1
    emitter.instruction("mov rax, 1");                                          // bit seed
    emitter.instruction("shl rax, cl");                                         // shift the bit into the descriptor position
    emitter.instruction("mov QWORD PTR [rbp - 168], rax");                      // set the descriptor bit in word 1
    emitter.label("__rt_ssa_fdset_done_x86");

    // -- build the timeout struct from timeout_us --
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the timeout in microseconds
    emitter.instruction("xor edx, edx");                                        // clear the division high word
    emitter.instruction("mov rcx, 1000000");                                    // one second in microseconds
    emitter.instruction("div rcx");                                             // rax = seconds, rdx = micro_remainder
    emitter.instruction("mov QWORD PTR [rbp - 176], rax");                      // store the timeout seconds field
    emitter.instruction("imul rdx, rdx, 1000");                                 // convert microseconds to nanoseconds for timespec
    emitter.instruction("mov QWORD PTR [rbp - 184], rdx");                      // store the timeout nanoseconds field

    // -- pselect6(nfds=128, readfds, writefds=NULL, exceptfds=NULL, &timeout, sigmask=NULL) --
    emitter.instruction("mov edi, 128");                                        // examine descriptors 0..127
    emitter.instruction("lea rsi, [rbp - 160]");                                // pointer to the readfds fd_set
    emitter.instruction("xor edx, edx");                                        // no writefds
    emitter.instruction("xor r10d, r10d");                                      // no exceptfds
    emitter.instruction("lea r8, [rbp - 176]");                                 // pointer to the timeout struct
    emitter.instruction("xor r9d, r9d");                                        // signal mask pointer (NULL)
    emitter.instruction("mov eax, 270");                                        // Linux x86_64 syscall 270 = pselect6
    emitter.instruction("syscall");                                             // wait for descriptor readiness
    emitter.instruction("cmp rax, 0");                                          // did select fail or time out?
    emitter.instruction("jle __rt_ssa_fail_clear_x86");                         // 0 = timeout, <0 = error → report failure

    emitter.label("__rt_ssa_blocking_accept_x86");
    // -- accept(fd, &sockaddr, &addrlen) --
    emitter.instruction("mov DWORD PTR [rbp - 152], 128");                      // addrlen in/out parameter = 128
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the listening fd
    emitter.instruction("lea rsi, [rbp - 144]");                                // pointer to the peer sockaddr buffer
    emitter.instruction("lea rdx, [rbp - 152]");                                // pointer to the addrlen parameter
    emitter.instruction("mov eax, 43");                                         // Linux x86_64 syscall 43 = accept
    emitter.instruction("syscall");                                             // accept the connection
    emitter.instruction("test rax, rax");                                       // did accept() return a valid descriptor?
    emitter.instruction("js __rt_ssa_fail_clear_x86");                          // accept() failed: clear globals and report -1
    emitter.instruction("mov QWORD PTR [rbp - 192], rax");                      // save the accepted fd

    // -- dispatch on the captured sockaddr's address family --
    emitter.instruction("movsxd r10, DWORD PTR [rbp - 152]");                   // reload addrlen returned by accept (signed widen)
    emitter.instruction("cmp r10, 2");                                          // is there at least a family-byte's worth of data?
    emitter.instruction("jl __rt_ssa_empty_peer_x86");                          // no peer info: empty $peer_name
    let family_addr_disp = -144 + family_off;
    emitter.instruction(&format!(
        "movzx r11d, BYTE PTR [rbp - {}]",
        -family_addr_disp
    ));                                                                         // load the address-family discriminator
    let af_inet6_linux = Platform::Linux.af_inet6();
    emitter.instruction("cmp r11d, 2");                                         // AF_INET = 2 on Linux
    emitter.instruction("je __rt_ssa_peer_inet_x86");                           // IPv4 peer: format A.B.C.D:port
    emitter.instruction("cmp r11d, 1");                                         // AF_UNIX = 1 on Linux
    emitter.instruction("je __rt_ssa_peer_unix_x86");                           // Unix-domain peer: format sun_path
    emitter.instruction(&format!("cmp r11d, {}", af_inet6_linux));              // AF_INET6 = 10 on Linux
    emitter.instruction("je __rt_ssa_peer_inet6_x86");                          // IPv6 peer: format [ipv6]:port
    emitter.instruction("jmp __rt_ssa_empty_peer_x86");                         // unsupported family: empty $peer_name

    // -- AF_INET peer: render via __rt_format_sockaddr_in --
    emitter.label("__rt_ssa_peer_inet_x86");
    emitter.instruction("lea rdi, [rbp - 144]");                                // pointer to the captured peer sockaddr
    emitter.instruction("call __rt_format_sockaddr_in");                        // rax = address string, rdx = length
    emitter.instruction("jmp __rt_ssa_stash_peer_x86");                         // common stash + return

    // -- AF_INET6 peer: render via __rt_format_sockaddr_in6 --
    emitter.label("__rt_ssa_peer_inet6_x86");
    emitter.instruction("lea rdi, [rbp - 144]");                                // pointer to the captured peer sockaddr_in6
    emitter.instruction("call __rt_format_sockaddr_in6");                       // rax = address string, rdx = length
    emitter.instruction("jmp __rt_ssa_stash_peer_x86");                         // common stash + return

    // -- AF_UNIX peer: render via __rt_format_sockaddr_unix --
    emitter.label("__rt_ssa_peer_unix_x86");
    emitter.instruction("lea rdi, [rbp - 144]");                                // pointer to the captured peer sockaddr
    emitter.instruction("movsxd rsi, DWORD PTR [rbp - 152]");                   // addrlen returned by accept
    emitter.instruction("call __rt_format_sockaddr_unix");                      // rax = sun_path string, rdx = length
    emitter.instruction("jmp __rt_ssa_stash_peer_x86");                         // common stash + return

    // -- empty $peer_name: connected/unnamed peer or unsupported family --
    emitter.label("__rt_ssa_empty_peer_x86");
    emitter.instruction("mov rax, 1");                                          // alloc one byte so the heap header is well-formed
    emitter.instruction("call __rt_heap_alloc");                                // rax = single-byte buffer
    emitter.instruction("mov r10, 0x454C504800000001");                         // owned-string heap-kind word with the x86_64 heap marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the buffer as an owned string
    emitter.instruction("xor edx, edx");                                        // empty address length

    emitter.label("__rt_ssa_stash_peer_x86");
    emitter.instruction("lea r10, [rip + _accept_peer_ptr]");                   // address of the stashed-pointer global
    emitter.instruction("mov QWORD PTR [r10], rax");                            // stash the peer address pointer
    emitter.instruction("lea r10, [rip + _accept_peer_len]");                   // address of the stashed-length global
    emitter.instruction("mov QWORD PTR [r10], rdx");                            // stash the peer address length
    emitter.instruction("mov rax, QWORD PTR [rbp - 192]");                      // return the accepted descriptor
    emitter.instruction("add rsp, 192");                                        // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the accepted fd

    emitter.label("__rt_ssa_fail_clear_x86");
    emitter.instruction("lea r10, [rip + _accept_peer_ptr]");                   // address of the stashed-pointer global
    emitter.instruction("mov QWORD PTR [r10], 0");                              // clear the stashed peer address pointer
    emitter.instruction("lea r10, [rip + _accept_peer_len]");                   // address of the stashed-length global
    emitter.instruction("mov QWORD PTR [r10], 0");                              // clear the stashed peer address length
    emitter.instruction("mov rax, -1");                                         // -1 reports a failed / timed-out accept
    emitter.instruction("add rsp, 192");                                        // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the failure result
}
