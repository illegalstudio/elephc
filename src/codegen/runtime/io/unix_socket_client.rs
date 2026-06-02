//! Purpose:
//! Emits the `__rt_unix_socket_client` runtime helper, which opens a connected
//! Unix-domain socket to a filesystem path. The socket type (SOCK_STREAM /
//! SOCK_DGRAM) is passed in by the caller so this one helper backs both the
//! `unix://` (stream) and `udg://` (datagram) transports.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::io`.
//! - `__rt_stream_socket_client` tail-branches here for `unix://` and
//!   `udg://` addresses; the caller is responsible for advancing the address
//!   pointer past the scheme prefix before the branch.
//!
//! Key details:
//! - The incoming pointer/length already point at the filesystem path. The
//!   path is copied into a `sockaddr_un` whose `sun_path` starts at offset 2
//!   on both macOS and Linux.
//! - Returns the connected descriptor, or -1 on failure.

use crate::codegen::{emit::Emitter, platform::Arch, platform::Platform};

/// unix_socket_client: open a connected Unix-domain socket.
/// Input:  AArch64 x0 = path pointer, x1 = path length, x2 = sock_type
///         x86_64  rdi = path pointer, rsi = path length, rdx = sock_type
///         where sock_type is 1 (SOCK_STREAM, for `unix://`) or 2
///         (SOCK_DGRAM, for `udg://`).
/// Output: connected descriptor, or -1 on failure
pub fn emit_unix_socket_client(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_unix_socket_client_linux_x86_64(emitter);
        return;
    }

    let plat = emitter.platform;
    emitter.blank();
    emitter.comment("--- runtime: unix_socket_client ---");
    emitter.label_global("__rt_unix_socket_client");

    // Frame: [0) fd, [8) path ptr, [16) path len, [24..152) sockaddr_un,
    //        [152) sock_type carried across the syscalls.
    emitter.instruction("sub sp, sp, #160");                                    // frame for fd, path state, sockaddr_un and sock_type
    emitter.instruction("str x0, [sp, #8]");                                    // save the filesystem path pointer (caller pre-shifted past the scheme)
    emitter.instruction("str x1, [sp, #16]");                                   // save the filesystem path length (caller pre-shifted past the scheme)
    emitter.instruction("str x2, [sp, #152]");                                  // save the sock_type for the socket() syscall

    // -- socket(AF_UNIX, sock_type, 0) --
    emitter.instruction("mov x0, #1");                                          // AF_UNIX
    emitter.instruction("ldr x1, [sp, #152]");                                  // SOCK_STREAM (unix://) or SOCK_DGRAM (udg://)
    emitter.instruction("mov x2, #0");                                          // default protocol
    emitter.syscall(97);
    if plat.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: a negative descriptor means failure
    }
    emitter.instruction(&plat.branch_on_syscall_success("__rt_unix_socket_client_sock_ok")); // continue when socket succeeded
    emitter.instruction("b __rt_unix_socket_client_fail");                      // socket() failed
    emitter.label("__rt_unix_socket_client_sock_ok");
    emitter.instruction("str x0, [sp, #0]");                                    // save the socket descriptor

    // -- build the sockaddr_un at [sp, #24]: family then sun_path --
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
    emitter.label("__rt_unix_socket_client_copy");
    emitter.instruction("cmp x12, x10");                                        // copied every path byte?
    emitter.instruction("b.hs __rt_unix_socket_client_copy_done");              // copy complete
    emitter.instruction("ldrb w13, [x9, x12]");                                 // load a path byte
    emitter.instruction("strb w13, [x11, x12]");                                // store it into sun_path
    emitter.instruction("add x12, x12, #1");                                    // advance the copy index
    emitter.instruction("b __rt_unix_socket_client_copy");                      // keep copying
    emitter.label("__rt_unix_socket_client_copy_done");
    emitter.instruction("strb wzr, [x11, x12]");                                // NUL-terminate sun_path

    // -- connect(fd, &sockaddr_un, 2 + pathlen + 1) --
    emitter.instruction("ldr x0, [sp, #0]");                                    // socket descriptor
    emitter.instruction("add x1, sp, #24");                                     // pointer to the sockaddr_un
    emitter.instruction("ldr x2, [sp, #16]");                                   // path length
    emitter.instruction("add x2, x2, #3");                                      // address length = 2 family + path + NUL
    emitter.syscall(98);
    if plat.needs_cmp_before_error_branch() {
        emitter.instruction("cmp x0, #0");                                      // Linux: a negative result means failure
    }
    emitter.instruction(&plat.branch_on_syscall_success("__rt_unix_socket_client_ok")); // continue when connect succeeded
    emitter.instruction("b __rt_unix_socket_client_fail_close");                // connect() failed

    emitter.label("__rt_unix_socket_client_ok");
    emitter.instruction("ldr x0, [sp, #0]");                                    // return the connected descriptor
    emitter.instruction("add sp, sp, #160");                                    // release the frame
    emitter.instruction("ret");                                                 // return the connected socket

    emitter.label("__rt_unix_socket_client_fail_close");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the socket descriptor
    emitter.syscall(6);

    emitter.label("__rt_unix_socket_client_fail");
    emitter.instruction("mov x0, #-1");                                         // -1 reports a failed connection
    emitter.instruction("add sp, sp, #160");                                    // release the frame
    emitter.instruction("ret");                                                 // return the failure result
}

fn emit_unix_socket_client_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: unix_socket_client ---");
    emitter.label_global("__rt_unix_socket_client");

    // Frame: [rsp+0) fd, [rsp+8) path ptr, [rsp+16) path len,
    //        [rsp+24..152) sockaddr_un, [rsp+152) sock_type.
    emitter.instruction("sub rsp, 160");                                        // frame for fd, path state, sockaddr_un and sock_type
    emitter.instruction("mov QWORD PTR [rsp + 8], rdi");                        // save the filesystem path pointer (caller pre-shifted past the scheme)
    emitter.instruction("mov QWORD PTR [rsp + 16], rsi");                       // save the filesystem path length (caller pre-shifted past the scheme)
    emitter.instruction("mov QWORD PTR [rsp + 152], rdx");                      // save the sock_type for the socket() syscall

    // -- socket(AF_UNIX, sock_type, 0) --
    emitter.instruction("mov edi, 1");                                          // AF_UNIX
    emitter.instruction("mov rsi, QWORD PTR [rsp + 152]");                      // SOCK_STREAM (unix://) or SOCK_DGRAM (udg://)
    emitter.instruction("xor edx, edx");                                        // default protocol
    emitter.instruction("mov eax, 41");                                         // Linux x86_64 syscall 41 = socket
    emitter.instruction("syscall");                                             // create the socket
    emitter.instruction("test rax, rax");                                       // did socket() fail?
    emitter.instruction("js __rt_unix_socket_client_fail_x86");                 // socket() failed
    emitter.instruction("mov QWORD PTR [rsp + 0], rax");                        // save the socket descriptor

    // -- build the sockaddr_un at [rsp + 24]: family then sun_path --
    emitter.instruction("mov WORD PTR [rsp + 24], 1");                          // Linux sun_family = AF_UNIX
    emitter.instruction("mov r8, QWORD PTR [rsp + 8]");                         // path source pointer
    emitter.instruction("mov r9, QWORD PTR [rsp + 16]");                        // path length
    emitter.instruction("lea r10, [rsp + 26]");                                 // sun_path destination cursor
    emitter.instruction("xor rcx, rcx");                                        // copy index
    emitter.label("__rt_unix_socket_client_copy_x86");
    emitter.instruction("cmp rcx, r9");                                         // copied every path byte?
    emitter.instruction("jae __rt_unix_socket_client_copy_done_x86");           // copy complete
    emitter.instruction("movzx eax, BYTE PTR [r8 + rcx]");                      // load a path byte
    emitter.instruction("mov BYTE PTR [r10 + rcx], al");                        // store it into sun_path
    emitter.instruction("inc rcx");                                             // advance the copy index
    emitter.instruction("jmp __rt_unix_socket_client_copy_x86");                // keep copying
    emitter.label("__rt_unix_socket_client_copy_done_x86");
    emitter.instruction("mov BYTE PTR [r10 + rcx], 0");                         // NUL-terminate sun_path

    // -- connect(fd, &sockaddr_un, 2 + pathlen + 1) --
    emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");                        // socket descriptor
    emitter.instruction("lea rsi, [rsp + 24]");                                 // pointer to the sockaddr_un
    emitter.instruction("mov rdx, QWORD PTR [rsp + 16]");                       // path length
    emitter.instruction("add rdx, 3");                                          // address length = 2 family + path + NUL
    emitter.instruction("mov eax, 42");                                         // Linux x86_64 syscall 42 = connect
    emitter.instruction("syscall");                                             // connect the socket
    emitter.instruction("test rax, rax");                                       // did connect() fail?
    emitter.instruction("js __rt_unix_socket_client_fail_close_x86");           // connect() failed

    emitter.instruction("mov rax, QWORD PTR [rsp + 0]");                        // return the connected descriptor
    emitter.instruction("add rsp, 160");                                        // release the frame
    emitter.instruction("ret");                                                 // return the connected socket

    emitter.label("__rt_unix_socket_client_fail_close_x86");
    emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");                        // reload the socket descriptor
    emitter.instruction("mov eax, 3");                                          // Linux x86_64 syscall 3 = close
    emitter.instruction("syscall");                                             // close the failed socket

    emitter.label("__rt_unix_socket_client_fail_x86");
    emitter.instruction("mov rax, -1");                                         // -1 reports a failed connection
    emitter.instruction("add rsp, 160");                                        // release the frame
    emitter.instruction("ret");                                                 // return the failure result
}
