//! Purpose:
//! Emits the `__rt_unix_socket_server` runtime helper, which opens a bound
//! Unix-domain socket on a filesystem path. The socket type (SOCK_STREAM /
//! SOCK_DGRAM) is passed in by the caller so this one helper backs both the
//! `unix://` (stream, listens for connections) and `udg://` (datagram, bind
//! only) transports.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::io`.
//! - `__rt_stream_socket_server` tail-branches here for `unix://` and
//!   `udg://` addresses; the caller is responsible for advancing the address
//!   pointer past the scheme prefix before the branch.
//!
//! Key details:
//! - The incoming pointer/length already point at the filesystem path. The
//!   path is copied into a `sockaddr_un` whose `sun_path` starts at offset 2
//!   on both macOS and Linux.
//! - `listen()` runs only for `SOCK_STREAM`; a `SOCK_DGRAM` socket binds and
//!   returns immediately because datagrams don't go through `accept()`.
//! - Returns the bound descriptor, or -1 on failure.

use crate::codegen::{emit::Emitter, platform::Arch, platform::Platform};

/// unix_socket_server: open a bound Unix-domain socket.
/// Input:  AArch64 x0 = path pointer, x1 = path length, x2 = sock_type
///         x86_64  rdi = path pointer, rsi = path length, rdx = sock_type
///         where sock_type is 1 (SOCK_STREAM, for `unix://`) or 2
///         (SOCK_DGRAM, for `udg://`).
/// Output: bound descriptor, or -1 on failure
pub fn emit_unix_socket_server(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_unix_socket_server_linux_x86_64(emitter);
        return;
    }

    let plat = emitter.platform;
    emitter.blank();
    emitter.comment("--- runtime: unix_socket_server ---");
    emitter.label_global("__rt_unix_socket_server");

    // Frame: [0) fd, [8) path ptr, [16) path len, [24..152) sockaddr_un,
    //        [152) sock_type carried across the syscalls.
    emitter.instruction("sub sp, sp, #160");                                    // frame for fd, path state, sockaddr_un and sock_type
    emitter.instruction("str x0, [sp, #8]");                                    // save the filesystem path pointer (caller pre-shifted past the scheme)
    emitter.instruction("str x1, [sp, #16]");                                   // save the filesystem path length (caller pre-shifted past the scheme)
    emitter.instruction("str x2, [sp, #152]");                                  // save the sock_type for the socket() syscall and the listen() gate

    // -- socket(AF_UNIX, sock_type, 0) --
    emitter.instruction("mov x0, #1");                                          // AF_UNIX
    emitter.instruction("ldr x1, [sp, #152]");                                  // SOCK_STREAM (unix://) or SOCK_DGRAM (udg://)
    emitter.instruction("mov x2, #0");                                          // default protocol
    emitter.syscall(97);
    if plat.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: a negative descriptor means failure
    }
    emitter.instruction(&plat.branch_on_syscall_success("__rt_unix_socket_server_sock_ok")); // continue when socket succeeded
    emitter.instruction("b __rt_unix_socket_server_fail");                      // socket() failed
    emitter.label("__rt_unix_socket_server_sock_ok");
    emitter.instruction("str x0, [sp, #0]");                                    // save the socket descriptor

    // -- build the sockaddr_un at [sp, #24]: family then sun_path --
    emit_sockaddr_un_aarch64(emitter, plat);

    // -- bind(fd, &sockaddr_un, 2 + pathlen + 1) --
    emitter.instruction("ldr x0, [sp, #0]");                                    // socket descriptor
    emitter.instruction("add x1, sp, #24");                                     // pointer to the sockaddr_un
    emitter.instruction("ldr x2, [sp, #16]");                                   // path length
    emitter.instruction("add x2, x2, #3");                                      // address length = 2 family + path + NUL
    emitter.syscall(104);
    if plat.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: a negative result means failure
    }
    emitter.instruction(&plat.branch_on_syscall_success("__rt_unix_socket_server_bind_ok")); // continue when bind succeeded
    emitter.instruction("b __rt_unix_socket_server_fail_close");                // bind() failed
    emitter.label("__rt_unix_socket_server_bind_ok");

    // -- listen(fd, 128) for SOCK_STREAM only; SOCK_DGRAM has no accept() --
    emitter.instruction("ldr x9, [sp, #152]");                                  // reload the sock_type
    emitter.instruction("cmp x9, #1");                                          // is this a SOCK_STREAM (unix://) socket?
    emitter.instruction("b.ne __rt_unix_socket_server_ok");                     // SOCK_DGRAM (udg://) skips listen(): bind alone suffices
    // This helper is a leaf (no saved x30), so spill the return address around
    // the only call it makes before reading the configured backlog.
    emitter.instruction("str x30, [sp, #-16]!");                                // save the return address across the backlog call
    emitter.instruction("bl __rt_socket_backlog");                              // resolve the configured backlog (default 128)
    emitter.instruction("mov x1, x0");                                          // backlog → listen() arg 1
    emitter.instruction("ldr x30, [sp], #16");                                  // restore the return address
    emitter.instruction("ldr x0, [sp, #0]");                                    // socket descriptor (reload after the call clobbers x0)
    emitter.syscall(106);
    if plat.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: a negative result means failure
    }
    emitter.instruction(&plat.branch_on_syscall_success("__rt_unix_socket_server_ok")); // continue when listen succeeded
    emitter.instruction("b __rt_unix_socket_server_fail_close");                // listen() failed

    emitter.label("__rt_unix_socket_server_ok");
    emitter.instruction("ldr x0, [sp, #0]");                                    // return the listening descriptor
    emitter.instruction("add sp, sp, #160");                                    // release the frame
    emitter.instruction("ret");                                                 // return the listening socket

    emitter.label("__rt_unix_socket_server_fail_close");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the socket descriptor
    emitter.syscall(6);

    emitter.label("__rt_unix_socket_server_fail");
    emitter.instruction("mov x0, #-1");                                         // -1 reports a failed server socket
    emitter.instruction("add sp, sp, #160");                                    // release the frame
    emitter.instruction("ret");                                                 // return the failure result
}

/// Emits the AArch64 sockaddr_un construction at `[sp, #24]`: the family bytes
/// followed by the NUL-terminated path copied from `[sp, #8]`/`[sp, #16]`.
fn emit_sockaddr_un_aarch64(emitter: &mut Emitter, plat: Platform) {
    if matches!(plat, Platform::MacOS) {
        emitter.instruction("ldr x9, [sp, #16]");                               // reload the path length for sun_len
        emitter.instruction("add x9, x9, #3");                                  // sun_len = 2 family + path + NUL
        emitter.instruction("strb w9, [sp, #24]");                              // macOS sockaddr_un begins with sun_len
        emitter.instruction("mov w9, #1");                                      // AF_UNIX
        emitter.instruction("strb w9, [sp, #25]");                              // store sin_family
    } else {
        emitter.instruction("mov w9, #1");                                      // Linux sun_family is a 2-byte field
        emitter.instruction("strb w9, [sp, #24]");                              // store the family low byte
        emitter.instruction("strb wzr, [sp, #25]");                             // store the family high byte
    }
    emitter.instruction("ldr x9, [sp, #8]");                                    // path source pointer
    emitter.instruction("ldr x10, [sp, #16]");                                  // path length
    emitter.instruction("add x11, sp, #26");                                    // sun_path destination cursor
    emitter.instruction("mov x12, #0");                                         // copy index
    emitter.label("__rt_unix_socket_server_copy");
    emitter.instruction("cmp x12, x10");                                        // copied every path byte?
    emitter.instruction("b.hs __rt_unix_socket_server_copy_done");              // copy complete
    emitter.instruction("ldrb w13, [x9, x12]");                                 // load a path byte
    emitter.instruction("strb w13, [x11, x12]");                                // store it into sun_path
    emitter.instruction("add x12, x12, #1");                                    // advance the copy index
    emitter.instruction("b __rt_unix_socket_server_copy");                      // keep copying
    emitter.label("__rt_unix_socket_server_copy_done");
    emitter.instruction("strb wzr, [x11, x12]");                                // NUL-terminate sun_path
}

fn emit_unix_socket_server_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: unix_socket_server ---");
    emitter.label_global("__rt_unix_socket_server");

    // Frame: [rsp+0) fd, [rsp+8) path ptr, [rsp+16) path len,
    //        [rsp+24..152) sockaddr_un, [rsp+152) sock_type.
    emitter.instruction("sub rsp, 168");                                        // frame (168≡8 mod 16: keeps rsp 16-aligned at the __rt_socket_backlog call)
    emitter.instruction("mov QWORD PTR [rsp + 8], rdi");                        // save the filesystem path pointer (caller pre-shifted past the scheme)
    emitter.instruction("mov QWORD PTR [rsp + 16], rsi");                       // save the filesystem path length (caller pre-shifted past the scheme)
    emitter.instruction("mov QWORD PTR [rsp + 152], rdx");                      // save the sock_type for the socket() syscall and the listen() gate

    // -- socket(AF_UNIX, sock_type, 0) --
    emitter.instruction("mov edi, 1");                                          // AF_UNIX
    emitter.instruction("mov rsi, QWORD PTR [rsp + 152]");                      // SOCK_STREAM (unix://) or SOCK_DGRAM (udg://)
    emitter.instruction("xor edx, edx");                                        // default protocol
    emitter.instruction("mov eax, 41");                                         // Linux x86_64 syscall 41 = socket
    emitter.instruction("syscall");                                             // create the socket
    emitter.instruction("test rax, rax");                                       // did socket() fail?
    emitter.instruction("js __rt_unix_socket_server_fail_x86");                 // socket() failed
    emitter.instruction("mov QWORD PTR [rsp + 0], rax");                        // save the socket descriptor

    // -- build the sockaddr_un at [rsp + 24]: family then sun_path --
    emitter.instruction("mov WORD PTR [rsp + 24], 1");                          // Linux sun_family = AF_UNIX
    emitter.instruction("mov r8, QWORD PTR [rsp + 8]");                         // path source pointer
    emitter.instruction("mov r9, QWORD PTR [rsp + 16]");                        // path length
    emitter.instruction("lea r10, [rsp + 26]");                                 // sun_path destination cursor
    emitter.instruction("xor rcx, rcx");                                        // copy index
    emitter.label("__rt_unix_socket_server_copy_x86");
    emitter.instruction("cmp rcx, r9");                                         // copied every path byte?
    emitter.instruction("jae __rt_unix_socket_server_copy_done_x86");           // copy complete
    emitter.instruction("movzx eax, BYTE PTR [r8 + rcx]");                      // load a path byte
    emitter.instruction("mov BYTE PTR [r10 + rcx], al");                        // store it into sun_path
    emitter.instruction("inc rcx");                                             // advance the copy index
    emitter.instruction("jmp __rt_unix_socket_server_copy_x86");                // keep copying
    emitter.label("__rt_unix_socket_server_copy_done_x86");
    emitter.instruction("mov BYTE PTR [r10 + rcx], 0");                         // NUL-terminate sun_path

    // -- bind(fd, &sockaddr_un, 2 + pathlen + 1) --
    emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");                        // socket descriptor
    emitter.instruction("lea rsi, [rsp + 24]");                                 // pointer to the sockaddr_un
    emitter.instruction("mov rdx, QWORD PTR [rsp + 16]");                       // path length
    emitter.instruction("add rdx, 3");                                          // address length = 2 family + path + NUL
    emitter.instruction("mov eax, 49");                                         // Linux x86_64 syscall 49 = bind
    emitter.instruction("syscall");                                             // bind the socket
    emitter.instruction("test rax, rax");                                       // did bind() fail?
    emitter.instruction("js __rt_unix_socket_server_fail_close_x86");           // bind() failed

    // -- listen(fd, 128) for SOCK_STREAM only; SOCK_DGRAM has no accept() --
    emitter.instruction("mov r10, QWORD PTR [rsp + 152]");                      // reload the sock_type
    emitter.instruction("cmp r10, 1");                                          // is this a SOCK_STREAM (unix://) socket?
    emitter.instruction("jne __rt_unix_socket_server_done_x86");                // SOCK_DGRAM (udg://) skips listen(): bind alone suffices
    emitter.instruction("call __rt_socket_backlog");                            // resolve the configured backlog (default 128)
    emitter.instruction("mov esi, eax");                                        // backlog → listen() arg 1
    emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");                        // socket descriptor (reload after the call clobbers rax)
    emitter.instruction("mov eax, 50");                                         // Linux x86_64 syscall 50 = listen
    emitter.instruction("syscall");                                             // mark the socket as listening
    emitter.instruction("test rax, rax");                                       // did listen() fail?
    emitter.instruction("js __rt_unix_socket_server_fail_close_x86");           // listen() failed

    emitter.label("__rt_unix_socket_server_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rsp + 0]");                        // return the bound descriptor
    emitter.instruction("add rsp, 168");                                        // release the frame
    emitter.instruction("ret");                                                 // return the bound socket

    emitter.label("__rt_unix_socket_server_fail_close_x86");
    emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");                        // reload the socket descriptor
    emitter.instruction("mov eax, 3");                                          // Linux x86_64 syscall 3 = close
    emitter.instruction("syscall");                                             // close the failed socket

    emitter.label("__rt_unix_socket_server_fail_x86");
    emitter.instruction("mov rax, -1");                                         // -1 reports a failed server socket
    emitter.instruction("add rsp, 168");                                        // release the frame
    emitter.instruction("ret");                                                 // return the failure result
}
