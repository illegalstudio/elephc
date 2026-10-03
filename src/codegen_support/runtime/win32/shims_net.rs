//! Win32 shims for the socket/DNS/hostname family: gethostname,
//! gethostbyname, socket/winsock init/cleanup, accept4/setsockopt/getsockopt/
//! socketpair/pselect6/sendmsg/recvmsg, and the W3e-2 net/dns/inet + misc
//! msvcrt family (getaddrinfo, inet_pton/ntop, strtoll, atof, setlocale, chown failure).

use crate::codegen::emit::Emitter;

/// Legacy AF_UNIX loopback registry retained out of the Windows runtime build.
#[cfg(any())]
pub(super) fn emit_unix_loopback_registry(emitter: &mut Emitter) {
    emitter.label_global("__rt_win_unix_find_socket");
    emitter.instruction("lea rax, [rip + _win_unix_socket_records]");           // first socket record
    emitter.instruction("mov ecx, 32");                                         // bounded registry capacity
    emitter.label(".Lwin_unix_find_socket_loop");
    emitter.instruction("cmp QWORD PTR [rax], rdi");                            // record owns this SOCKET?
    emitter.instruction("je .Lwin_unix_find_socket_done");                      // return matching record
    emitter.instruction("add rax, 32");                                         // advance to next socket record
    emitter.instruction("dec ecx");                                             // one fewer record remains
    emitter.instruction("jnz .Lwin_unix_find_socket_loop");                     // scan remaining records
    emitter.instruction("xor eax, eax");                                        // no matching socket record
    emitter.label(".Lwin_unix_find_socket_done");
    emitter.instruction("ret");                                                 // return record or NULL
    emitter.blank();

    emitter.label_global("__rt_win_unix_alloc_socket");
    emitter.instruction("lea rax, [rip + _win_unix_socket_records]");           // first socket record
    emitter.instruction("mov ecx, 32");                                         // bounded registry capacity
    emitter.label(".Lwin_unix_alloc_socket_loop");
    emitter.instruction("cmp QWORD PTR [rax], 0");                              // free socket record?
    emitter.instruction("je .Lwin_unix_alloc_socket_store");                    // initialize this record
    emitter.instruction("add rax, 32");                                         // advance to next socket record
    emitter.instruction("dec ecx");                                             // one fewer record remains
    emitter.instruction("jnz .Lwin_unix_alloc_socket_loop");                    // scan remaining records
    emitter.instruction("xor eax, eax");                                        // registry exhausted
    emitter.instruction("ret");                                                 // return NULL
    emitter.label(".Lwin_unix_alloc_socket_store");
    emitter.instruction("mov QWORD PTR [rax], rdi");                            // store full-width SOCKET
    emitter.instruction("mov QWORD PTR [rax + 8], rsi");                        // preserve SOCK_STREAM/SOCK_DGRAM
    emitter.instruction("mov QWORD PTR [rax + 16], 0");                         // no bound local endpoint yet
    emitter.instruction("mov QWORD PTR [rax + 24], 0");                         // no connected peer endpoint yet
    emitter.instruction("ret");                                                 // return initialized record
    emitter.blank();

    emitter.label_global("__rt_win_unix_close_all");
    emitter.instruction("sub rsp, 72");                                         // shadow space and bounded registry-loop locals
    emitter.instruction("lea rax, [rip + _win_unix_socket_records]");           // first emulated socket record
    emitter.instruction("mov QWORD PTR [rsp + 32], rax");                       // retain current record across closesocket
    emitter.instruction("mov DWORD PTR [rsp + 40], 32");                        // bounded socket registry capacity
    emitter.label(".Lwin_unix_close_all_socket_loop");
    emitter.instruction("mov rax, QWORD PTR [rsp + 32]");                       // current socket record
    emitter.instruction("mov rcx, QWORD PTR [rax]");                            // registered full-width SOCKET
    emitter.instruction("test rcx, rcx");                                       // occupied record?
    emitter.instruction("jz .Lwin_unix_close_all_socket_next");                 // skip unused record
    emitter.instruction("call closesocket");                                    // release the native loopback socket
    emitter.instruction("mov rax, QWORD PTR [rsp + 32]");                       // recover record after the Win32 call
    emitter.instruction("mov QWORD PTR [rax], 0");                              // release socket metadata
    emitter.instruction("mov QWORD PTR [rax + 16], 0");                         // detach local endpoint metadata
    emitter.instruction("mov QWORD PTR [rax + 24], 0");                         // detach peer endpoint metadata
    emitter.label(".Lwin_unix_close_all_socket_next");
    emitter.instruction("add QWORD PTR [rsp + 32], 32");                        // advance to next socket record
    emitter.instruction("dec DWORD PTR [rsp + 40]");                            // one fewer record remains
    emitter.instruction("jnz .Lwin_unix_close_all_socket_loop");                // close every occupied record
    emitter.instruction("lea rax, [rip + _win_unix_endpoint_records]");         // first named endpoint record
    emitter.instruction("mov ecx, 32");                                         // bounded endpoint registry capacity
    emitter.label(".Lwin_unix_close_all_endpoint_loop");
    emitter.instruction("mov QWORD PTR [rax], 0");                              // unregister endpoint name
    emitter.instruction("mov QWORD PTR [rax + 8], 0");                          // detach endpoint owner socket
    emitter.instruction("add rax, 140");                                        // advance to next endpoint record
    emitter.instruction("dec ecx");                                             // one fewer endpoint remains
    emitter.instruction("jnz .Lwin_unix_close_all_endpoint_loop");              // clear every endpoint record
    emitter.instruction("add rsp, 72");                                         // restore stack
    emitter.instruction("ret");                                                 // return after deterministic teardown
    emitter.blank();

    emitter.label_global("__rt_win_unix_find_path");
    emitter.instruction("lea rax, [rip + _win_unix_endpoint_records]");         // first endpoint record
    emitter.instruction("mov r8d, 32");                                         // bounded registry capacity
    emitter.label(".Lwin_unix_find_path_record");
    emitter.instruction("cmp QWORD PTR [rax], 0");                              // active endpoint?
    emitter.instruction("je .Lwin_unix_find_path_next");                        // skip unused record
    emitter.instruction("lea r9, [rax + 32]");                                  // stored path bytes
    emitter.instruction("mov ecx, 108");                                        // sockaddr_un path capacity
    emitter.instruction("mov r10, rdi");                                        // requested NUL-terminated path
    emitter.label(".Lwin_unix_find_path_compare");
    emitter.instruction("mov dl, BYTE PTR [r10]");                              // requested byte
    emitter.instruction("cmp dl, BYTE PTR [r9]");                               // same as stored byte?
    emitter.instruction("jne .Lwin_unix_find_path_next");                       // path differs
    emitter.instruction("test dl, dl");                                         // reached matching terminator?
    emitter.instruction("jz .Lwin_unix_find_path_done");                        // return matching endpoint
    emitter.instruction("inc r10");                                             // next requested byte
    emitter.instruction("inc r9");                                              // next stored byte
    emitter.instruction("dec ecx");                                             // one fewer byte remains
    emitter.instruction("jnz .Lwin_unix_find_path_compare");                    // compare bounded path
    emitter.instruction("jmp .Lwin_unix_find_path_done");                       // 108-byte paths match
    emitter.label(".Lwin_unix_find_path_next");
    emitter.instruction("add rax, 140");                                        // advance to next endpoint record
    emitter.instruction("dec r8d");                                             // one fewer endpoint remains
    emitter.instruction("jnz .Lwin_unix_find_path_record");                     // scan remaining endpoints
    emitter.instruction("xor eax, eax");                                        // path is not registered
    emitter.label(".Lwin_unix_find_path_done");
    emitter.instruction("ret");                                                 // return endpoint or NULL
    emitter.blank();

    emitter.label_global("__rt_win_unix_find_port");
    emitter.instruction("lea rax, [rip + _win_unix_endpoint_records]");         // first endpoint record
    emitter.instruction("mov ecx, 32");                                         // bounded registry capacity
    emitter.label(".Lwin_unix_find_port_loop");
    emitter.instruction("cmp QWORD PTR [rax], 0");                              // active endpoint?
    emitter.instruction("je .Lwin_unix_find_port_next");                        // skip unused record
    emitter.instruction("cmp WORD PTR [rax + 24], di");                         // same network-order loopback port?
    emitter.instruction("je .Lwin_unix_find_port_done");                        // return matching endpoint
    emitter.label(".Lwin_unix_find_port_next");
    emitter.instruction("add rax, 140");                                        // advance to next endpoint record
    emitter.instruction("dec ecx");                                             // one fewer endpoint remains
    emitter.instruction("jnz .Lwin_unix_find_port_loop");                       // scan remaining endpoints
    emitter.instruction("xor eax, eax");                                        // port is not registered
    emitter.label(".Lwin_unix_find_port_done");
    emitter.instruction("ret");                                                 // return endpoint or NULL
    emitter.blank();

    emitter.label_global("__rt_win_unix_alloc_endpoint");
    emitter.instruction("lea rax, [rip + _win_unix_endpoint_records]");         // first endpoint record
    emitter.instruction("mov ecx, 32");                                         // bounded registry capacity
    emitter.label(".Lwin_unix_alloc_endpoint_loop");
    emitter.instruction("cmp QWORD PTR [rax], 0");                              // free endpoint record?
    emitter.instruction("je .Lwin_unix_alloc_endpoint_store");                  // initialize this record
    emitter.instruction("add rax, 140");                                        // advance to next endpoint record
    emitter.instruction("dec ecx");                                             // one fewer endpoint remains
    emitter.instruction("jnz .Lwin_unix_alloc_endpoint_loop");                  // scan remaining endpoints
    emitter.instruction("xor eax, eax");                                        // registry exhausted
    emitter.instruction("ret");                                                 // return NULL
    emitter.label(".Lwin_unix_alloc_endpoint_store");
    emitter.instruction("mov QWORD PTR [rax], 1");                              // mark endpoint active
    emitter.instruction("mov QWORD PTR [rax + 8], rdi");                        // owner SOCKET
    emitter.instruction("mov QWORD PTR [rax + 16], rsi");                       // socket type
    emitter.instruction("mov QWORD PTR [rax + 24], rdx");                       // network-order loopback port
    emitter.instruction("lea r8, [rax + 32]");                                  // endpoint path destination
    emitter.instruction("xor ecx, ecx");                                        // path byte index
    emitter.label(".Lwin_unix_alloc_endpoint_copy");
    emitter.instruction("mov r9b, BYTE PTR [r10 + rcx]");                       // source path byte
    emitter.instruction("mov BYTE PTR [r8 + rcx], r9b");                        // copy path byte
    emitter.instruction("test r9b, r9b");                                       // copied terminator?
    emitter.instruction("jz .Lwin_unix_alloc_endpoint_done");                   // endpoint is complete
    emitter.instruction("inc ecx");                                             // next path byte
    emitter.instruction("cmp ecx, 107");                                        // reserve final byte for NUL
    emitter.instruction("jb .Lwin_unix_alloc_endpoint_copy");                   // keep copying bounded path
    emitter.instruction("mov BYTE PTR [r8 + 107], 0");                          // terminate truncated path deterministically
    emitter.label(".Lwin_unix_alloc_endpoint_done");
    emitter.instruction("ret");                                                 // return endpoint record
    emitter.blank();

    emitter.label_global("__rt_win_unix_unlink_path");
    emitter.instruction("sub rsp, 8");                                          // align internal call
    emitter.instruction("call __rt_win_unix_find_path");                        // locate emulated endpoint by path
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("test rax, rax");                                       // registered endpoint?
    emitter.instruction("jz .Lwin_unix_unlink_path_missing");                   // let filesystem unlink handle it
    emitter.instruction("mov QWORD PTR [rax], 0");                              // release endpoint name
    emitter.instruction("mov eax, 1");                                          // handled successfully
    emitter.instruction("ret");                                                 // return true
    emitter.label(".Lwin_unix_unlink_path_missing");
    emitter.instruction("xor eax, eax");                                        // not an emulated endpoint
    emitter.instruction("ret");                                                 // return false
    emitter.blank();

    emitter.label_global("__rt_win_unix_write_sockaddr");
    emitter.instruction("mov r8, rdi");                                         // endpoint record, or NULL for anonymous peer
    emitter.instruction("mov r9, rsi");                                         // destination sockaddr_un
    emitter.instruction("mov r10, rdx");                                        // destination length pointer
    emitter.instruction("xor eax, eax");                                        // zero fill value
    emitter.instruction("mov ecx, 110");                                        // family plus sun_path bytes
    emitter.instruction("mov rdi, r9");                                         // memset destination
    emitter.instruction("rep stosb");                                           // clear synthesized sockaddr_un
    emitter.instruction("mov WORD PTR [r9], 1");                                // AF_UNIX in shared layout
    emitter.instruction("test r8, r8");                                         // named endpoint available?
    emitter.instruction("jz .Lwin_unix_write_sockaddr_length");                 // anonymous peer has empty path
    emitter.instruction("lea rsi, [r8 + 32]");                                  // stored endpoint path
    emitter.instruction("lea rdi, [r9 + 2]");                                   // sockaddr_un sun_path
    emitter.instruction("mov ecx, 108");                                        // bounded path capacity
    emitter.label(".Lwin_unix_write_sockaddr_copy");
    emitter.instruction("mov al, BYTE PTR [rsi]");                              // stored path byte
    emitter.instruction("mov BYTE PTR [rdi], al");                              // copy into sockaddr_un
    emitter.instruction("inc rsi");                                             // next source byte
    emitter.instruction("inc rdi");                                             // next destination byte
    emitter.instruction("test al, al");                                         // copied terminator?
    emitter.instruction("jz .Lwin_unix_write_sockaddr_length");                 // finish synthesized address
    emitter.instruction("dec ecx");                                             // one fewer byte remains
    emitter.instruction("jnz .Lwin_unix_write_sockaddr_copy");                  // copy bounded path
    emitter.label(".Lwin_unix_write_sockaddr_length");
    emitter.instruction("test r10, r10");                                       // caller supplied addrlen pointer?
    emitter.instruction("jz .Lwin_unix_write_sockaddr_done");                   // no length to publish
    emitter.instruction("mov DWORD PTR [r10], 110");                            // synthesized sockaddr_un size
    emitter.label(".Lwin_unix_write_sockaddr_done");
    emitter.instruction("xor eax, eax");                                        // successful status
    emitter.instruction("ret");                                                 // return success
    emitter.blank();
}

/// Emits stateless compatibility labels for obsolete AF_UNIX registry callers.
///
/// PHP does not register AF_UNIX transports on Windows. The socket creation
/// shim rejects the family before these labels can be reached; `unlink` and
/// process cleanup may still call their no-op entry points.
pub(super) fn emit_unix_loopback_registry(emitter: &mut Emitter) {
    for label in [
        "__rt_win_unix_find_socket",
        "__rt_win_unix_alloc_socket",
        "__rt_win_unix_find_path",
        "__rt_win_unix_find_port",
        "__rt_win_unix_alloc_endpoint",
        "__rt_win_unix_unlink_path",
    ] {
        emitter.label_global(label);
        emitter.instruction("xor eax, eax");                                    // no Windows AF_UNIX registry entry exists
        emitter.instruction("ret");                                             // report absent metadata or an unhandled path
        emitter.blank();
    }
    emitter.label_global("__rt_win_unix_close_all");
    emitter.instruction("ret");                                                 // no AF_UNIX compatibility sockets are registered
    emitter.blank();
    emitter.label_global("__rt_win_unix_write_sockaddr");
    emitter.instruction("mov rax, -1");                                         // AF_UNIX address synthesis is unavailable on Windows
    emitter.instruction("ret");                                                 // report unsupported address synthesis
    emitter.blank();
}

/// Emits a shim that wraps msvcrt `gethostname`.
pub(super) fn emit_shim_gethostname(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_gethostname");
    emitter.instruction("sub rsp, 40");                                         // shadow space
    emitter.instruction("mov rcx, rdi");                                        // buffer
    emitter.instruction("mov rdx, rsi");                                        // length
    emitter.instruction("call gethostname");                                    // msvcrt gethostname
    emitter.instruction("cmp eax, -1");                                         // SOCKET_ERROR?
    emitter.instruction("je .Lgethostname_capture_errno");                      // publish Winsock hostname failure
    emitter.instruction("cdqe");                                                // sign-extend status
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lgethostname_capture_errno");
    emitter.instruction("call __rt_wsa_capture_errno");                         // capture WSAGetLastError and return -1
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return SOCKET_ERROR
    emitter.blank();
}

/// Emits the `__rt_sys_gethostbyname` shim: converts SysV `gethostbyname(name)` to MSx64
/// and calls ws2_32 `gethostbyname`. SysV: rdi=name → MSx64: rcx=name. Mirrors
/// `emit_shim_gethostname` (1-arg case). The `struct hostent *` return stays in rax.
pub(super) fn emit_shim_gethostbyname(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_gethostbyname");
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + alignment(8)
    emitter.instruction("mov rcx, rdi");                                        // name → arg1 (rcx)
    emitter.instruction("call gethostbyname");                                  // ws2_32 gethostbyname (returns struct hostent* in rax)
    emitter.instruction("test rax, rax");                                       // lookup succeeded?
    emitter.instruction("jz .Lgethostbyname_capture_errno");                    // publish resolver failure
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lgethostbyname_capture_errno");
    emitter.instruction("call __rt_wsa_capture_errno");                         // capture WSAGetLastError
    emitter.instruction("xor eax, eax");                                        // pointer API failure sentinel is NULL
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return NULL
    emitter.blank();
}

/// Emits socket-related shims (socket, connect, bind, listen, accept, send, recv, etc.).
/// sendto/recvfrom have 6 args and are emitted separately with dedicated shims.
///
/// Class-3 sign-extension rule: EVERY Winsock shim that returns a 32-bit `int` STATUS in
/// `eax` (0 on success, `SOCKET_ERROR` = -1 = 0xFFFFFFFF on failure) whose consumer
/// sign-tests the 64-bit `rax` needs `cdqe` after the call. On x86_64 writing `eax` zeroes
/// the upper 32 bits of `rax`, so without sign-extension a failure leaves
/// `rax = 0x00000000_FFFFFFFF` (positive) and a `test rax,rax; js` / `cmp rax,0; jl`
/// consumer misses it. `bind`, `listen`, `connect`, `shutdown`, `getsockname`, and
/// `getpeername` all return int status and are therefore emitted OUTSIDE the shared loop as
/// dedicated `cdqe` blocks. Verified consumers:
/// - bind:        stream_socket_server.rs:394/406, stream_socket_server_v6.rs:375/387,
///                unix_socket_server.rs (`test rax,rax; js`)
/// - listen:      same server sites as bind
/// - connect:     stream_socket_client.rs:380-381 (`test rax,rax; js`)
/// - shutdown:    stream_socket_shutdown.rs:47 (`test rax,rax; js` — a failed shutdown
///                otherwise reports true)
/// - getsockname: stream_socket_get_name.rs:310 (`cmp rax,0; jl` — a failed getsockname
///                otherwise parses an uninitialized sockaddr)
/// - getpeername: same shared `__rt_ssgn_after_x86` check (routed via syscall 52)
///
/// ONLY `socket` and `accept` stay in the shared loop below: they return a `SOCKET` (a
/// 64-bit `UINT_PTR` handle, NOT an int status), where `INVALID_SOCKET` = ~0 is already a
/// 64-bit -1 and `cdqe` would CORRUPT a valid handle whose bit 31 is set.
pub(super) fn emit_shim_socket_shims(emitter: &mut Emitter) {
    // Windows assigns AF_INET6=23 while the shared runtime emits Linux's value 10.
    emitter.label_global("__rt_sys_socket");
    emitter.instruction("sub rsp, 40");                                         // shadow space and ABI alignment
    emitter.instruction("cmp edi, 1");                                          // shared AF_UNIX requested?
    emitter.instruction("je .Lsocket_unix_unsupported");                        // PHP does not register AF_UNIX transports on Windows
    emitter.instruction("mov r8, rdx");                                         // preserve protocol before loading MSx64 arg2
    emitter.instruction("mov ecx, edi");                                        // address family
    emitter.instruction("cmp ecx, 10");                                         // Linux AF_INET6?
    emitter.instruction("jne .Lsocket_family_ready");                           // other family values already agree
    emitter.instruction("mov ecx, 23");                                         // Winsock AF_INET6
    emitter.label(".Lsocket_family_ready");
    emitter.instruction("mov edx, esi");                                        // socket type
    emitter.instruction("call socket");                                         // create full-width Winsock SOCKET
    emitter.instruction("cmp rax, -1");                                         // INVALID_SOCKET?
    emitter.instruction("je .Lsocket_capture_errno");                           // publish ordinary socket failure
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return SOCKET
    emitter.label(".Lsocket_unix_unsupported");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 97");                // EAFNOSUPPORT, matching PHP's absent Windows transport
    emitter.instruction("mov rax, -1");                                         // INVALID_SOCKET
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // reject AF_UNIX before consulting Winsock
    emitter.label(".Lsocket_capture_errno");
    emitter.instruction("call __rt_wsa_capture_errno");                         // translate Winsock failure
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return INVALID_SOCKET
    emitter.blank();

    // The AF_UNIX registry branches in the generic accept/bind/connect/name/
    // send/receive shims below are legacy-unreachable: `__rt_sys_socket`
    // rejects family 1 before a Windows socket can acquire registry metadata.
    // They remain emitted for now because removing them from this combined
    // multi-shim function is a larger mechanical cleanup than the parity fix.
    let shims: &[(&str, &str)] = &[("__rt_sys_accept", "accept")];
    for (label, func) in shims {
        emitter.label_global(label);
        emitter.instruction("sub rsp, 88");                                     // shadow space plus Unix-emulation locals
        emitter.instruction("mov QWORD PTR [rsp + 40], rsi");                   // preserve caller address buffer
        emitter.instruction("mov QWORD PTR [rsp + 48], rdx");                   // preserve caller length pointer
        emitter.instruction("mov QWORD PTR [rsp + 56], rdi");                   // preserve listening socket
        emitter.instruction("call __rt_win_unix_find_socket");                  // detect emulated listener
        emitter.instruction("mov QWORD PTR [rsp + 32], rax");                   // retain listener metadata or NULL
        emitter.instruction("mov r8, rdx");                                     // save arg3 before rdx is overwritten
        emitter.instruction("mov rcx, QWORD PTR [rsp + 56]");                   // listening socket
        emitter.instruction("mov rdx, QWORD PTR [rsp + 40]");                   // caller address buffer
        emitter.instruction("mov r8, QWORD PTR [rsp + 48]");                    // caller address length pointer
        emitter.instruction(&format!("call {}", func));                         // call Win32 function (returns 64-bit SOCKET handle)
        emitter.instruction("cmp rax, -1");                                     // INVALID_SOCKET?
        emitter.instruction(&format!("je .L{func}_capture_errno"));             // publish the corresponding Winsock failure
        emitter.instruction("cmp QWORD PTR [rsp + 32], 0");                     // emulated Unix listener?
        emitter.instruction(&format!("je .L{func}_success"));                   // native accept is complete
        emitter.instruction("mov QWORD PTR [rsp + 64], rax");                   // preserve accepted socket
        emitter.instruction("mov rdi, rax");                                    // accepted socket for registry
        emitter.instruction("mov r10, QWORD PTR [rsp + 32]");                   // listener metadata
        emitter.instruction("mov rsi, QWORD PTR [r10 + 8]");                    // inherit socket type
        emitter.instruction("call __rt_win_unix_alloc_socket");                 // register accepted Unix stream
        emitter.instruction("test rax, rax");                                   // metadata allocation succeeded?
        emitter.instruction(&format!("jz .L{func}_registry_full"));             // close accepted socket on exhaustion
        emitter.instruction("mov r10, QWORD PTR [rsp + 32]");                   // listener metadata
        emitter.instruction("mov r11, QWORD PTR [r10 + 16]");                   // listener local endpoint
        emitter.instruction("mov QWORD PTR [rax + 16], r11");                   // accepted socket shares local name
        emitter.instruction("mov QWORD PTR [rax + 24], 0");                     // client peer is anonymous
        emitter.instruction("xor edi, edi");                                    // anonymous Unix peer endpoint
        emitter.instruction("mov rsi, QWORD PTR [rsp + 40]");                   // caller sockaddr buffer
        emitter.instruction("mov rdx, QWORD PTR [rsp + 48]");                   // caller length pointer
        emitter.instruction("test rsi, rsi");                                   // caller requested peer address?
        emitter.instruction(&format!("jz .L{func}_restore_socket"));            // no sockaddr to synthesize
        emitter.instruction("call __rt_win_unix_write_sockaddr");               // publish anonymous AF_UNIX peer
        emitter.label(&format!(".L{func}_restore_socket"));
        emitter.instruction("mov rax, QWORD PTR [rsp + 64]");                   // restore accepted socket
        emitter.label(&format!(".L{func}_success"));
        emitter.instruction("add rsp, 88");                                     // restore stack
        emitter.instruction("ret");                                             // return handle (no cdqe: 64-bit SOCKET)
        emitter.label(&format!(".L{func}_registry_full"));
        emitter.instruction("mov rcx, QWORD PTR [rsp + 64]");                   // untracked accepted socket
        emitter.instruction("call closesocket");                                // release native resource
        emitter.instruction("mov DWORD PTR [rip + __rt_errno], 24");            // EMFILE registry-capacity failure
        emitter.instruction("mov rax, -1");                                     // INVALID_SOCKET
        emitter.instruction("add rsp, 88");                                     // restore stack
        emitter.instruction("ret");                                             // return failure
        emitter.label(&format!(".L{func}_capture_errno"));
        emitter.instruction("call __rt_wsa_capture_errno");                     // capture WSAGetLastError and return -1
        emitter.instruction("add rsp, 88");                                     // restore stack
        emitter.instruction("ret");                                             // return INVALID_SOCKET
        emitter.blank();
    }
    // bind: sign-extend the Winsock int return into rax so SOCKET_ERROR (-1) reads as
    // a 64-bit negative and the `test rax,rax; js fail` consumer detects a failed bind.
    emitter.label_global("__rt_sys_bind");
    emitter.instruction("sub rsp, 120");                                        // shadow space, native sockaddr, and emulation locals
    emitter.instruction("mov QWORD PTR [rsp + 32], rsi");                       // preserve sockaddr for family restoration
    emitter.instruction("mov QWORD PTR [rsp + 40], 0");                         // family is untranslated by default
    emitter.instruction("mov QWORD PTR [rsp + 48], rdi");                       // preserve full-width socket
    emitter.instruction("mov QWORD PTR [rsp + 56], rdx");                       // preserve caller sockaddr length
    emitter.instruction("call __rt_win_unix_find_socket");                      // detect emulated Unix socket
    emitter.instruction("mov QWORD PTR [rsp + 88], rax");                       // retain socket metadata or NULL
    emitter.instruction("test rax, rax");                                       // emulated socket?
    emitter.instruction("jz .Lbind_native_family");                             // use ordinary bind path
    emitter.instruction("mov r10, QWORD PTR [rsp + 32]");                       // caller sockaddr_un
    emitter.instruction("cmp WORD PTR [r10], 1");                               // AF_UNIX address?
    emitter.instruction("jne .Lbind_invalid_unix_family");                      // reject incoherent emulated bind
    emitter.instruction("lea rdi, [r10 + 2]");                                  // requested Unix path
    emitter.instruction("call __rt_win_unix_find_path");                        // reject duplicate active endpoint names
    emitter.instruction("test rax, rax");                                       // path already registered?
    emitter.instruction("jnz .Lbind_address_in_use");                           // deterministic EADDRINUSE
    emitter.instruction("pxor xmm0, xmm0");                                     // zero native sockaddr_in
    emitter.instruction("movdqu XMMWORD PTR [rsp + 64], xmm0");                 // clear 16-byte address
    emitter.instruction("mov WORD PTR [rsp + 64], 2");                          // Winsock AF_INET
    emitter.instruction("mov DWORD PTR [rsp + 68], 0x0100007f");                // 127.0.0.1 in network byte order
    emitter.instruction("mov rcx, QWORD PTR [rsp + 48]");                       // socket
    emitter.instruction("lea rdx, [rsp + 64]");                                 // loopback sockaddr_in
    emitter.instruction("mov r8d, 16");                                         // sockaddr_in length
    emitter.instruction("call bind");                                           // bind ephemeral loopback port
    emitter.instruction("cmp eax, -1");                                         // SOCKET_ERROR?
    emitter.instruction("je .Lbind_capture_errno");                             // publish native bind failure
    emitter.instruction("mov DWORD PTR [rsp + 80], 16");                        // getsockname input capacity
    emitter.instruction("mov rcx, QWORD PTR [rsp + 48]");                       // bound socket
    emitter.instruction("lea rdx, [rsp + 64]");                                 // receive assigned loopback port
    emitter.instruction("lea r8, [rsp + 80]");                                  // sockaddr length pointer
    emitter.instruction("call getsockname");                                    // discover assigned ephemeral port
    emitter.instruction("cmp eax, -1");                                         // SOCKET_ERROR?
    emitter.instruction("je .Lbind_capture_errno");                             // publish discovery failure
    emitter.instruction("mov rax, QWORD PTR [rsp + 88]");                       // socket metadata
    emitter.instruction("mov rdi, QWORD PTR [rsp + 48]");                       // endpoint owner socket
    emitter.instruction("mov rsi, QWORD PTR [rax + 8]");                        // endpoint stream/datagram type
    emitter.instruction("movzx edx, WORD PTR [rsp + 66]");                      // assigned network-order port
    emitter.instruction("mov r10, QWORD PTR [rsp + 32]");                       // original sockaddr_un
    emitter.instruction("add r10, 2");                                          // Unix path bytes
    emitter.instruction("call __rt_win_unix_alloc_endpoint");                   // publish path-to-loopback mapping
    emitter.instruction("test rax, rax");                                       // endpoint capacity available?
    emitter.instruction("jz .Lbind_registry_full");                             // deterministic capacity failure
    emitter.instruction("mov r10, QWORD PTR [rsp + 88]");                       // socket metadata
    emitter.instruction("mov QWORD PTR [r10 + 16], rax");                       // attach local endpoint
    emitter.instruction("xor eax, eax");                                        // bind success
    emitter.instruction("add rsp, 120");                                        // restore stack
    emitter.instruction("ret");                                                 // return success
    emitter.label(".Lbind_native_family");
    emitter.instruction("mov rsi, QWORD PTR [rsp + 32]");                       // restore caller sockaddr
    emitter.instruction("cmp WORD PTR [rsi], 10");                              // Linux AF_INET6 sockaddr?
    emitter.instruction("jne .Lbind_family_ready");                             // no translation needed
    emitter.instruction("mov WORD PTR [rsi], 23");                              // Winsock AF_INET6
    emitter.instruction("mov QWORD PTR [rsp + 40], 1");                         // restore shared layout after the call
    emitter.label(".Lbind_family_ready");
    emitter.instruction("mov r8, QWORD PTR [rsp + 56]");                        // caller sockaddr length
    emitter.instruction("mov rcx, QWORD PTR [rsp + 48]");                       // socket
    emitter.instruction("mov rdx, QWORD PTR [rsp + 32]");                       // output sockaddr
    emitter.instruction("call bind");                                           // call Winsock bind (result in eax)
    emitter.instruction("cmp QWORD PTR [rsp + 40], 0");                         // translated sockaddr family?
    emitter.instruction("je .Lbind_family_restored");                           // keep original family
    emitter.instruction("mov r10, QWORD PTR [rsp + 32]");                       // shared sockaddr pointer
    emitter.instruction("mov WORD PTR [r10], 10");                              // restore Linux AF_INET6
    emitter.label(".Lbind_family_restored");
    emitter.instruction("cmp eax, -1");                                         // SOCKET_ERROR?
    emitter.instruction("je .Lbind_capture_errno");                             // publish Winsock failure
    emitter.instruction("cdqe");                                                // sign-extend eax → rax (SOCKET_ERROR=-1 negative)
    emitter.instruction("add rsp, 120");                                        // restore stack
    emitter.instruction("ret");                                                 // return sign-extended result
    emitter.label(".Lbind_invalid_unix_family");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 97");                // EAFNOSUPPORT
    emitter.instruction("jmp .Lbind_local_failure");                            // return failure
    emitter.label(".Lbind_address_in_use");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 98");                // EADDRINUSE
    emitter.instruction("jmp .Lbind_local_failure");                            // return failure
    emitter.label(".Lbind_registry_full");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 28");                // ENOSPC registry-capacity failure
    emitter.label(".Lbind_local_failure");
    emitter.instruction("mov rax, -1");                                         // SOCKET_ERROR
    emitter.instruction("add rsp, 120");                                        // restore stack
    emitter.instruction("ret");                                                 // return failure
    emitter.label(".Lbind_capture_errno");
    emitter.instruction("call __rt_wsa_capture_errno");                         // capture WSAGetLastError and return -1
    emitter.instruction("add rsp, 120");                                        // restore stack
    emitter.instruction("ret");                                                 // return SOCKET_ERROR
    emitter.blank();
    // listen: sign-extend the Winsock int return into rax so SOCKET_ERROR (-1) reads as
    // a 64-bit negative and the `test rax,rax; js fail` consumer detects a failed listen.
    emitter.label_global("__rt_sys_listen");
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + alignment(8)
    emitter.instruction("mov r8, rdx");                                         // save arg3 before rdx is overwritten
    emitter.instruction("mov rcx, rdi");                                        // arg1
    emitter.instruction("mov rdx, rsi");                                        // backlog
    emitter.instruction("call listen");                                         // call Winsock listen (result in eax)
    emitter.instruction("cmp eax, -1");                                         // SOCKET_ERROR?
    emitter.instruction("je .Llisten_capture_errno");                           // publish Winsock failure
    emitter.instruction("cdqe");                                                // sign-extend eax → rax (SOCKET_ERROR=-1 negative)
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return sign-extended result
    emitter.label(".Llisten_capture_errno");
    emitter.instruction("call __rt_wsa_capture_errno");                         // capture WSAGetLastError and return -1
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return SOCKET_ERROR
    emitter.blank();
    // connect: sign-extend the Winsock int return into rax so SOCKET_ERROR (-1) reads as
    // a 64-bit negative and the `test rax,rax; js fail` consumer detects a refused port.
    emitter.label_global("__rt_sys_connect");
    emitter.instruction("sub rsp, 120");                                        // shadow space, native sockaddr, and emulation locals
    emitter.instruction("mov QWORD PTR [rsp + 32], rsi");                       // preserve sockaddr for family restoration
    emitter.instruction("mov QWORD PTR [rsp + 40], 0");                         // family is untranslated by default
    emitter.instruction("mov QWORD PTR [rsp + 48], rdi");                       // preserve full-width socket
    emitter.instruction("mov QWORD PTR [rsp + 56], rdx");                       // preserve caller sockaddr length
    emitter.instruction("call __rt_win_unix_find_socket");                      // detect emulated Unix socket
    emitter.instruction("mov QWORD PTR [rsp + 88], rax");                       // retain socket metadata or NULL
    emitter.instruction("test rax, rax");                                       // emulated socket?
    emitter.instruction("jz .Lconnect_native_family");                          // use ordinary connect path
    emitter.instruction("mov r10, QWORD PTR [rsp + 32]");                       // caller sockaddr_un
    emitter.instruction("cmp WORD PTR [r10], 1");                               // AF_UNIX address?
    emitter.instruction("jne .Lconnect_invalid_unix_family");                   // reject incoherent emulated connect
    emitter.instruction("lea rdi, [r10 + 2]");                                  // requested Unix path
    emitter.instruction("call __rt_win_unix_find_path");                        // resolve path to loopback endpoint
    emitter.instruction("test rax, rax");                                       // endpoint exists?
    emitter.instruction("jz .Lconnect_path_missing");                           // deterministic ENOENT
    emitter.instruction("mov QWORD PTR [rsp + 96], rax");                       // retain peer endpoint
    emitter.instruction("mov r10, QWORD PTR [rsp + 88]");                       // socket metadata
    emitter.instruction("mov r11, QWORD PTR [r10 + 8]");                        // client socket type
    emitter.instruction("cmp r11, QWORD PTR [rax + 16]");                       // same as endpoint type?
    emitter.instruction("jne .Lconnect_type_mismatch");                         // deterministic EPROTOTYPE
    emitter.instruction("pxor xmm0, xmm0");                                     // zero native sockaddr_in
    emitter.instruction("movdqu XMMWORD PTR [rsp + 64], xmm0");                 // clear 16-byte address
    emitter.instruction("mov WORD PTR [rsp + 64], 2");                          // Winsock AF_INET
    emitter.instruction("mov r11w, WORD PTR [rax + 24]");                       // registered network-order port
    emitter.instruction("mov WORD PTR [rsp + 66], r11w");                       // sockaddr_in port
    emitter.instruction("mov DWORD PTR [rsp + 68], 0x0100007f");                // 127.0.0.1 in network byte order
    emitter.instruction("mov rcx, QWORD PTR [rsp + 48]");                       // socket
    emitter.instruction("lea rdx, [rsp + 64]");                                 // resolved loopback sockaddr_in
    emitter.instruction("mov r8d, 16");                                         // sockaddr_in length
    emitter.instruction("call connect");                                        // connect emulated Unix endpoint
    emitter.instruction("cmp eax, -1");                                         // SOCKET_ERROR?
    emitter.instruction("je .Lconnect_capture_errno");                          // publish native connect failure
    emitter.instruction("mov r10, QWORD PTR [rsp + 88]");                       // socket metadata
    emitter.instruction("mov r11, QWORD PTR [rsp + 96]");                       // resolved peer endpoint
    emitter.instruction("mov QWORD PTR [r10 + 24], r11");                       // attach peer name
    emitter.instruction("xor eax, eax");                                        // connect success
    emitter.instruction("add rsp, 120");                                        // restore stack
    emitter.instruction("ret");                                                 // return success
    emitter.label(".Lconnect_native_family");
    emitter.instruction("mov rsi, QWORD PTR [rsp + 32]");                       // restore caller sockaddr
    emitter.instruction("cmp WORD PTR [rsi], 10");                              // Linux AF_INET6 sockaddr?
    emitter.instruction("jne .Lconnect_family_ready");                          // no translation needed
    emitter.instruction("mov WORD PTR [rsi], 23");                              // Winsock AF_INET6
    emitter.instruction("mov QWORD PTR [rsp + 40], 1");                         // restore shared layout after the call
    emitter.label(".Lconnect_family_ready");
    emitter.instruction("mov r8, QWORD PTR [rsp + 56]");                        // caller sockaddr length
    emitter.instruction("mov rcx, QWORD PTR [rsp + 48]");                       // socket
    emitter.instruction("mov rdx, rsi");                                        // arg2
    emitter.instruction("call connect");                                        // call Winsock connect (result in eax)
    emitter.instruction("cmp QWORD PTR [rsp + 40], 0");                         // translated sockaddr family?
    emitter.instruction("je .Lconnect_family_restored");                        // keep original family
    emitter.instruction("mov r10, QWORD PTR [rsp + 32]");                       // shared sockaddr pointer
    emitter.instruction("mov WORD PTR [r10], 10");                              // restore Linux AF_INET6
    emitter.label(".Lconnect_family_restored");
    emitter.instruction("cmp eax, -1");                                         // SOCKET_ERROR?
    emitter.instruction("je .Lconnect_capture_errno");                          // publish refused/timeout errors
    emitter.instruction("cdqe");                                                // sign-extend eax → rax (SOCKET_ERROR=-1 negative)
    emitter.instruction("add rsp, 120");                                        // restore stack
    emitter.instruction("ret");                                                 // return sign-extended result
    emitter.label(".Lconnect_invalid_unix_family");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 97");                // EAFNOSUPPORT
    emitter.instruction("jmp .Lconnect_local_failure");                         // return failure
    emitter.label(".Lconnect_path_missing");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 2");                 // ENOENT
    emitter.instruction("jmp .Lconnect_local_failure");                         // return failure
    emitter.label(".Lconnect_type_mismatch");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 91");                // EPROTOTYPE
    emitter.label(".Lconnect_local_failure");
    emitter.instruction("mov rax, -1");                                         // SOCKET_ERROR
    emitter.instruction("add rsp, 120");                                        // restore stack
    emitter.instruction("ret");                                                 // return failure
    emitter.label(".Lconnect_capture_errno");
    emitter.instruction("call __rt_wsa_capture_errno");                         // capture WSAGetLastError and return -1
    emitter.instruction("add rsp, 120");                                        // restore stack
    emitter.instruction("ret");                                                 // return SOCKET_ERROR
    emitter.blank();
    // shutdown: sign-extend the Winsock int return into rax so SOCKET_ERROR (-1) reads as
    // a 64-bit negative and the `test rax,rax; js fail` consumer detects a failed shutdown.
    emitter.label_global("__rt_sys_shutdown");
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + alignment(8)
    emitter.instruction("mov r8, rdx");                                         // save arg3 before rdx is overwritten
    emitter.instruction("mov rcx, rdi");                                        // arg1
    emitter.instruction("mov rdx, rsi");                                        // arg2
    emitter.instruction("call shutdown");                                       // call Winsock shutdown (result in eax)
    emitter.instruction("cmp eax, -1");                                         // SOCKET_ERROR?
    emitter.instruction("je .Lshutdown_capture_errno");                         // publish Winsock failure
    emitter.instruction("cdqe");                                                // sign-extend eax → rax (SOCKET_ERROR=-1 negative)
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return sign-extended result
    emitter.label(".Lshutdown_capture_errno");
    emitter.instruction("call __rt_wsa_capture_errno");                         // capture WSAGetLastError and return -1
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return SOCKET_ERROR
    emitter.blank();
    // getsockname: sign-extend the Winsock int return into rax so SOCKET_ERROR (-1) reads as
    // a 64-bit negative and the `cmp rax,0; jl fail` consumer detects a failed getsockname.
    emitter.label_global("__rt_sys_getsockname");
    emitter.instruction("sub rsp, 72");                                         // shadow space plus sockaddr and emulation locals
    emitter.instruction("mov QWORD PTR [rsp + 32], rsi");                       // preserve output sockaddr
    emitter.instruction("mov QWORD PTR [rsp + 40], rdx");                       // preserve output length pointer
    emitter.instruction("mov QWORD PTR [rsp + 48], rdi");                       // preserve full-width socket
    emitter.instruction("call __rt_win_unix_find_socket");                      // detect emulated Unix socket
    emitter.instruction("test rax, rax");                                       // emulated socket metadata?
    emitter.instruction("jz .Lgetsockname_native");                             // query Winsock for ordinary socket
    emitter.instruction("mov rdi, QWORD PTR [rax + 16]");                       // local endpoint or NULL
    emitter.instruction("mov rsi, QWORD PTR [rsp + 32]");                       // caller sockaddr buffer
    emitter.instruction("mov rdx, QWORD PTR [rsp + 40]");                       // caller length pointer
    emitter.instruction("call __rt_win_unix_write_sockaddr");                   // synthesize AF_UNIX local name
    emitter.instruction("add rsp, 72");                                         // restore stack
    emitter.instruction("ret");                                                 // return success
    emitter.label(".Lgetsockname_native");
    emitter.instruction("mov r8, QWORD PTR [rsp + 40]");                        // output length pointer
    emitter.instruction("mov rcx, QWORD PTR [rsp + 48]");                       // socket
    emitter.instruction("mov rdx, QWORD PTR [rsp + 32]");                       // output sockaddr
    emitter.instruction("call getsockname");                                    // call Winsock getsockname (result in eax)
    emitter.instruction("cmp eax, -1");                                         // SOCKET_ERROR?
    emitter.instruction("je .Lgetsockname_capture_errno");                      // publish Winsock failure
    emitter.instruction("mov r10, QWORD PTR [rsp + 32]");                       // output sockaddr
    emitter.instruction("cmp WORD PTR [r10], 23");                              // Winsock AF_INET6?
    emitter.instruction("jne .Lgetsockname_family_ready");                      // other families already agree
    emitter.instruction("mov WORD PTR [r10], 10");                              // restore shared Linux AF_INET6 layout
    emitter.label(".Lgetsockname_family_ready");
    emitter.instruction("cdqe");                                                // sign-extend eax → rax (SOCKET_ERROR=-1 negative)
    emitter.instruction("add rsp, 72");                                         // restore stack
    emitter.instruction("ret");                                                 // return sign-extended result
    emitter.label(".Lgetsockname_capture_errno");
    emitter.instruction("call __rt_wsa_capture_errno");                         // capture WSAGetLastError and return -1
    emitter.instruction("add rsp, 72");                                         // restore stack
    emitter.instruction("ret");                                                 // return SOCKET_ERROR
    emitter.blank();
    // getpeername: sign-extend the Winsock int return into rax so SOCKET_ERROR (-1) reads as
    // a 64-bit negative and the `cmp rax,0; jl fail` consumer detects a failed getpeername.
    emitter.label_global("__rt_sys_getpeername");
    emitter.instruction("sub rsp, 72");                                         // shadow space plus sockaddr and emulation locals
    emitter.instruction("mov QWORD PTR [rsp + 32], rsi");                       // preserve output sockaddr
    emitter.instruction("mov QWORD PTR [rsp + 40], rdx");                       // preserve output length pointer
    emitter.instruction("mov QWORD PTR [rsp + 48], rdi");                       // preserve full-width socket
    emitter.instruction("call __rt_win_unix_find_socket");                      // detect emulated Unix socket
    emitter.instruction("test rax, rax");                                       // emulated socket metadata?
    emitter.instruction("jz .Lgetpeername_native");                             // query Winsock for ordinary socket
    emitter.instruction("mov rdi, QWORD PTR [rax + 24]");                       // peer endpoint or NULL for anonymous peer
    emitter.instruction("mov rsi, QWORD PTR [rsp + 32]");                       // caller sockaddr buffer
    emitter.instruction("mov rdx, QWORD PTR [rsp + 40]");                       // caller length pointer
    emitter.instruction("call __rt_win_unix_write_sockaddr");                   // synthesize AF_UNIX peer name
    emitter.instruction("add rsp, 72");                                         // restore stack
    emitter.instruction("ret");                                                 // return success
    emitter.label(".Lgetpeername_native");
    emitter.instruction("mov r8, QWORD PTR [rsp + 40]");                        // output length pointer
    emitter.instruction("mov rcx, QWORD PTR [rsp + 48]");                       // socket
    emitter.instruction("mov rdx, QWORD PTR [rsp + 32]");                       // output sockaddr
    emitter.instruction("call getpeername");                                    // call Winsock getpeername (result in eax)
    emitter.instruction("cmp eax, -1");                                         // SOCKET_ERROR?
    emitter.instruction("je .Lgetpeername_capture_errno");                      // publish Winsock failure
    emitter.instruction("mov r10, QWORD PTR [rsp + 32]");                       // output sockaddr
    emitter.instruction("cmp WORD PTR [r10], 23");                              // Winsock AF_INET6?
    emitter.instruction("jne .Lgetpeername_family_ready");                      // other families already agree
    emitter.instruction("mov WORD PTR [r10], 10");                              // restore shared Linux AF_INET6 layout
    emitter.label(".Lgetpeername_family_ready");
    emitter.instruction("cdqe");                                                // sign-extend eax → rax (SOCKET_ERROR=-1 negative)
    emitter.instruction("add rsp, 72");                                         // restore stack
    emitter.instruction("ret");                                                 // return sign-extended result
    emitter.label(".Lgetpeername_capture_errno");
    emitter.instruction("call __rt_wsa_capture_errno");                         // capture WSAGetLastError and return -1
    emitter.instruction("add rsp, 72");                                         // restore stack
    emitter.instruction("ret");                                                 // return SOCKET_ERROR
    emitter.blank();
    // closesocket: 1 arg
    emitter.label_global("__rt_sys_closesocket");
    emitter.instruction("sub rsp, 56");                                         // shadow space plus socket local
    emitter.instruction("mov QWORD PTR [rsp + 32], rdi");                       // preserve full-width socket
    emitter.instruction("mov rcx, rdi");                                        // socket
    emitter.instruction("call closesocket");                                    // close socket
    emitter.instruction("cmp eax, -1");                                         // SOCKET_ERROR?
    emitter.instruction("je .Lclosesocket_capture_errno");                      // publish Winsock failure
    emitter.instruction("mov rdi, QWORD PTR [rsp + 32]");                       // closed socket for metadata lookup
    emitter.instruction("call __rt_win_unix_find_socket");                      // locate emulation metadata
    emitter.instruction("test rax, rax");                                       // emulated socket record?
    emitter.instruction("jz .Lclosesocket_done");                               // no metadata to clear
    emitter.instruction("mov QWORD PTR [rax], 0");                              // release socket record
    emitter.label(".Lclosesocket_done");
    emitter.instruction("xor eax, eax");                                        // successful status
    emitter.instruction("add rsp, 56");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lclosesocket_capture_errno");
    emitter.instruction("call __rt_wsa_capture_errno");                         // capture WSAGetLastError and return -1
    emitter.instruction("add rsp, 56");                                         // restore stack
    emitter.instruction("ret");                                                 // return SOCKET_ERROR
    emitter.blank();

    // sendto: 6 args — socket, buf, len, flags, dest_addr, addrlen
    // SysV: rdi=socket, rsi=buf, rdx=len, r10=flags, r8=dest_addr, r9=addrlen
    // MSx64: rcx, rdx, r8, r9, [rsp+32], [rsp+40]
    // Winsock sendto returns the byte count (>= 0) or SOCKET_ERROR (-1) as a 32-bit int;
    // stream_socket_sendto.rs:415 sign-tests it (`cmp rax,0; jl`), so cdqe sign-extends a
    // -1 failure into a 64-bit negative instead of a bogus positive byte count.
    emitter.label_global("__rt_sys_sendto");
    emitter.instruction("sub rsp, 136");                                        // shadow space, stack args, sockaddr, and emulation locals
    emitter.instruction("mov QWORD PTR [rsp + 112], rdi");                      // preserve socket
    emitter.instruction("mov QWORD PTR [rsp + 120], rsi");                      // preserve buffer
    emitter.instruction("mov QWORD PTR [rsp + 48], rdx");                       // preserve byte count
    emitter.instruction("mov QWORD PTR [rsp + 56], r10");                       // preserve flags
    emitter.instruction("mov QWORD PTR [rsp + 88], r8");                        // preserve destination address
    emitter.instruction("mov QWORD PTR [rsp + 96], r9");                        // preserve destination length
    emitter.instruction("test r8, r8");                                         // destination supplied?
    emitter.instruction("jz .Lsendto_address_ready");                           // connected socket uses no explicit address
    emitter.instruction("cmp WORD PTR [r8], 1");                                // AF_UNIX destination?
    emitter.instruction("jne .Lsendto_address_ready");                          // native sockaddr passes through
    emitter.instruction("mov rdi, QWORD PTR [rsp + 112]");                      // sending socket
    emitter.instruction("call __rt_win_unix_find_socket");                      // detect loopback fallback metadata
    emitter.instruction("test rax, rax");                                       // emulated AF_UNIX sender?
    emitter.instruction("jz .Lsendto_address_ready");                           // native AF_UNIX resolves paths across processes
    emitter.instruction("mov QWORD PTR [rsp + 128], rax");                      // retain fallback sender metadata
    emitter.instruction("mov r10, QWORD PTR [rsp + 88]");                       // restore destination sockaddr_un
    emitter.instruction("lea rdi, [r10 + 2]");                                  // requested Unix path
    emitter.instruction("call __rt_win_unix_find_path");                        // resolve endpoint registry
    emitter.instruction("test rax, rax");                                       // endpoint exists?
    emitter.instruction("jz .Lsendto_path_missing");                            // deterministic ENOENT
    emitter.instruction("mov QWORD PTR [rsp + 104], rax");                      // retain endpoint record
    emitter.instruction("mov r10, QWORD PTR [rsp + 104]");                      // destination endpoint
    emitter.instruction("mov rax, QWORD PTR [rsp + 128]");                      // fallback sender metadata
    emitter.instruction("mov r11, QWORD PTR [rax + 8]");                        // sender socket type
    emitter.instruction("cmp r11, QWORD PTR [r10 + 16]");                       // compatible endpoint type?
    emitter.instruction("jne .Lsendto_type_mismatch");                          // deterministic EPROTOTYPE
    emitter.instruction("pxor xmm0, xmm0");                                     // zero native sockaddr_in
    emitter.instruction("movdqu XMMWORD PTR [rsp + 64], xmm0");                 // clear 16-byte address
    emitter.instruction("mov WORD PTR [rsp + 64], 2");                          // Winsock AF_INET
    emitter.instruction("mov r11w, WORD PTR [r10 + 24]");                       // registered network-order port
    emitter.instruction("mov WORD PTR [rsp + 66], r11w");                       // sockaddr_in port
    emitter.instruction("mov DWORD PTR [rsp + 68], 0x0100007f");                // 127.0.0.1 in network byte order
    emitter.instruction("lea rax, [rsp + 64]");                                 // translated destination address
    emitter.instruction("mov QWORD PTR [rsp + 88], rax");                       // use translated sockaddr
    emitter.instruction("mov QWORD PTR [rsp + 96], 16");                        // native sockaddr_in length
    emitter.label(".Lsendto_address_ready");
    emitter.instruction("mov rax, QWORD PTR [rsp + 88]");                       // destination address
    emitter.instruction("mov QWORD PTR [rsp + 32], rax");                       // destination → 5th MSx64 arg
    emitter.instruction("mov rax, QWORD PTR [rsp + 96]");                       // destination length
    emitter.instruction("mov QWORD PTR [rsp + 40], rax");                       // addrlen → 6th MSx64 arg
    emitter.instruction("mov r9, QWORD PTR [rsp + 56]");                        // flags → r9
    emitter.instruction("mov r8, QWORD PTR [rsp + 48]");                        // len → r8
    emitter.instruction("mov rdx, QWORD PTR [rsp + 120]");                      // buffer → rdx
    emitter.instruction("mov rcx, QWORD PTR [rsp + 112]");                      // socket → rcx
    emitter.instruction("call sendto");                                         // sendto(socket, buf, len, flags, dest_addr, addrlen)
    emitter.instruction("cmp eax, -1");                                         // SOCKET_ERROR?
    emitter.instruction("je .Lsendto_capture_errno");                           // publish Winsock failure
    emitter.instruction("cdqe");                                                // sign-extend eax → rax (SOCKET_ERROR=-1 negative)
    emitter.instruction("add rsp, 136");                                        // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lsendto_path_missing");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 2");                 // ENOENT
    emitter.instruction("jmp .Lsendto_local_failure");                          // return failure
    emitter.label(".Lsendto_type_mismatch");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 91");                // EPROTOTYPE
    emitter.label(".Lsendto_local_failure");
    emitter.instruction("mov rax, -1");                                         // SOCKET_ERROR
    emitter.instruction("add rsp, 136");                                        // restore stack
    emitter.instruction("ret");                                                 // return failure
    emitter.label(".Lsendto_capture_errno");
    emitter.instruction("call __rt_wsa_capture_errno");                         // capture WSAGetLastError and return -1
    emitter.instruction("add rsp, 136");                                        // restore stack
    emitter.instruction("ret");                                                 // return SOCKET_ERROR
    emitter.blank();

    // recvfrom: 6 args — socket, buf, len, flags, src_addr, &addrlen
    // SysV: rdi=socket, rsi=buf, rdx=len, r10=flags, r8=src_addr, r9=&addrlen
    // MSx64: rcx, rdx, r8, r9, [rsp+32], [rsp+40]
    // Winsock recvfrom returns the byte count (>= 0) or SOCKET_ERROR (-1) as a 32-bit int;
    // stream_socket_recvfrom.rs:204 sign-tests it (`cmp rax,0; jl`), so cdqe sign-extends a
    // -1 failure into a 64-bit negative instead of a bogus positive byte count. On failure,
    // also translates WSAGetLastError() to a POSIX errno and stores it into __rt_errno (see
    // __rt_win32_errno_from_code, shims_c_symbols.rs) — this is what the fgets/fread EAGAIN
    // check needs to distinguish a nonblocking would-block from a real error/EOF.
    emitter.label_global("__rt_sys_recvfrom");
    emitter.instruction("sub rsp, 152");                                        // shadow space, stack args, sockaddr, and emulation locals
    emitter.instruction("mov QWORD PTR [rsp + 112], rdi");                      // preserve socket
    emitter.instruction("mov QWORD PTR [rsp + 120], rsi");                      // preserve buffer
    emitter.instruction("mov QWORD PTR [rsp + 48], rdx");                       // preserve byte count
    emitter.instruction("mov QWORD PTR [rsp + 56], r10");                       // preserve flags
    emitter.instruction("mov QWORD PTR [rsp + 88], r8");                        // preserve caller source sockaddr
    emitter.instruction("mov QWORD PTR [rsp + 96], r9");                        // preserve caller source length pointer
    emitter.instruction("mov QWORD PTR [rsp + 136], r8");                       // retain caller sockaddr across redirection
    emitter.instruction("mov QWORD PTR [rsp + 144], r9");                       // retain caller length pointer across redirection
    emitter.instruction("mov QWORD PTR [rsp + 104], 0");                        // native address output by default
    emitter.instruction("test r8, r8");                                         // caller requested source address?
    emitter.instruction("jz .Lrecvfrom_address_ready");                         // no address translation needed
    emitter.instruction("call __rt_win_unix_find_socket");                      // detect emulated Unix socket
    emitter.instruction("test rax, rax");                                       // emulated socket metadata?
    emitter.instruction("jz .Lrecvfrom_address_ready");                         // ordinary socket writes caller buffer
    emitter.instruction("pxor xmm0, xmm0");                                     // zero native sockaddr_in
    emitter.instruction("movdqu XMMWORD PTR [rsp + 64], xmm0");                 // clear 16-byte address buffer
    emitter.instruction("mov DWORD PTR [rsp + 80], 16");                        // native address capacity
    emitter.instruction("lea rax, [rsp + 64]");                                 // temporary native source address
    emitter.instruction("mov QWORD PTR [rsp + 88], rax");                       // redirect Winsock output
    emitter.instruction("lea rax, [rsp + 80]");                                 // temporary native source length
    emitter.instruction("mov QWORD PTR [rsp + 96], rax");                       // redirect Winsock length output
    emitter.instruction("mov QWORD PTR [rsp + 104], 1");                        // synthesize AF_UNIX source afterward
    emitter.label(".Lrecvfrom_address_ready");
    emitter.instruction("mov rax, QWORD PTR [rsp + 88]");                       // native source address
    emitter.instruction("mov QWORD PTR [rsp + 32], rax");                       // source address → 5th MSx64 arg
    emitter.instruction("mov rax, QWORD PTR [rsp + 96]");                       // native source length pointer
    emitter.instruction("mov QWORD PTR [rsp + 40], rax");                       // source length → 6th MSx64 arg
    emitter.instruction("mov r9, QWORD PTR [rsp + 56]");                        // flags → r9
    emitter.instruction("mov r8, QWORD PTR [rsp + 48]");                        // len → r8
    emitter.instruction("mov rdx, QWORD PTR [rsp + 120]");                      // buffer → rdx
    emitter.instruction("mov rcx, QWORD PTR [rsp + 112]");                      // socket → rcx
    emitter.instruction("call recvfrom");                                       // recvfrom(socket, buf, len, flags, src_addr, &addrlen)
    emitter.instruction("cmp eax, -1");                                         // SOCKET_ERROR?
    emitter.instruction("je .Lrecvfrom_fail");                                  // failure: translate WSAGetLastError and return -1
    emitter.instruction("cdqe");                                                // sign-extend byte count
    emitter.instruction("mov QWORD PTR [rsp + 128], rax");                      // preserve result across address synthesis
    emitter.instruction("cmp QWORD PTR [rsp + 104], 0");                        // emulated Unix source requested?
    emitter.instruction("je .Lrecvfrom_success");                               // native output is already complete
    emitter.instruction("movzx edi, WORD PTR [rsp + 66]");                      // sender's network-order loopback port
    emitter.instruction("call __rt_win_unix_find_port");                        // resolve sender endpoint name
    emitter.instruction("mov rdi, rax");                                        // endpoint or NULL for anonymous sender
    emitter.instruction("mov rsi, QWORD PTR [rsp + 136]");                      // caller sockaddr buffer
    emitter.instruction("mov rdx, QWORD PTR [rsp + 144]");                      // caller length pointer
    emitter.instruction("call __rt_win_unix_write_sockaddr");                   // synthesize AF_UNIX source name
    emitter.label(".Lrecvfrom_success");
    emitter.instruction("mov rax, QWORD PTR [rsp + 128]");                      // restore byte count
    emitter.instruction("add rsp, 152");                                        // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lrecvfrom_fail");
    emitter.instruction("call __rt_wsa_capture_errno");                         // capture WSAGetLastError and return -1
    emitter.instruction("add rsp, 152");                                        // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.blank();
}

/// Emits the `__rt_winsock_init` helper that calls `WSAStartup(MAKEWORD(2,2), &wsadata)`.
///
/// Allocates a 400-byte `WSADATA` buffer on the stack, loads `MAKEWORD(2,2)` (0x0202)
/// into the SysV first arg (`edi`), zero-inits the WSADATA buffer, shuffles to MSx64
/// (`rcx`=version, `rdx`=&wsadata), and calls `WSAStartup`. The return value is ignored
/// (Winsock init is best-effort; socket calls will fail with a meaningful WSAGetLastError
/// if startup failed). Called from the Windows `main` wrapper before `__elephc_main`.
pub(super) fn emit_winsock_init(emitter: &mut Emitter) {
    emitter.label_global("__rt_winsock_init");
    // -- stack frame: shadow(32) + WSADATA(400) + version spill(8) + pad to 16-byte align --
    // 32 + 400 + 8 = 440; round up to 456 (456 ≡ 8 mod 16, re-aligning the entry rsp ≡ 8).
    emitter.instruction("sub rsp, 456");                                        // shadow(32) + WSADATA(400) + spill(8) + pad(16), 16-byte aligned
    // -- zero the 400-byte WSADATA buffer at [rsp+32..432) --
    emitter.instruction("cld");                                                 // forward direction for rep stosb
    emitter.instruction("lea rdi, [rsp + 32]");                                 // dest = WSADATA buffer start (above shadow space)
    emitter.instruction("xor eax, eax");                                        // zero fill byte
    emitter.instruction("mov ecx, 400");                                        // WSADATA size in bytes (Microsoft's MAXGETHOSTSTRUCT-equivalent for v2.2)
    emitter.instruction("rep stosb");                                           // zero the whole WSADATA buffer so unused fields are determinate
    // -- WSAStartup(MAKEWORD(2,2), &wsadata) --
    emitter.instruction("mov edi, 0x0202");                                     // MAKEWORD(2,2) = 0x0202 (minor=2, major=2) — SysV arg1
    emitter.instruction("lea rsi, [rsp + 32]");                                 // &wsadata — SysV arg2
    emitter.instruction("mov rcx, rdi");                                        // MSx64 arg1 = version (MAKEWORD(2,2))
    emitter.instruction("mov rdx, rsi");                                        // MSx64 arg2 = &wsadata
    emitter.instruction("call WSAStartup");                                     // initialize Winsock 2.2 (return in eax: 0 = success, ignored)
    emitter.instruction("add rsp, 456");                                        // restore stack
    emitter.instruction("ret");                                                 // return to caller
    emitter.blank();
}

/// Emits the `__rt_winsock_cleanup` helper that calls `WSACleanup()`.
///
/// Releases Winsock resources. Safe to call even when `WSAStartup` was never invoked
/// (Winsock returns an error in that case, which we ignore). Called from `__rt_sys_exit`
/// before `ExitProcess` so socket resources are released on process termination.
pub(super) fn emit_winsock_cleanup(emitter: &mut Emitter) {
    emitter.label_global("__rt_winsock_cleanup");
    emitter.instruction("sub rsp, 40");                                         // shadow space (16-byte aligned: entry ≡ 8, 40 ≡ 8 → 0)
    emitter.instruction("call WSACleanup");                                     // release Winsock resources (return ignored)
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return to caller
    emitter.blank();
}

/// Emits an accept4 shim (maps to accept on Windows).
pub(super) fn emit_shim_accept4(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_accept4");
    emitter.instruction("sub rsp, 8");                                          // align internal call
    emitter.instruction("call __rt_sys_accept");                                // accept and preserve Unix-emulation metadata
    emitter.instruction("add rsp, 8");                                          // restore stack
    emitter.instruction("ret");                                                 // return accepted socket or failure
    emitter.blank();
}

/// Emits setsockopt shim — 5 args: socket, level, optname, optval, optlen.
///
/// SysV: rdi=socket, rsi=level, rdx=optname, r10=optval, r8=optlen
/// MSx64: rcx, rdx, r8, r9, [rsp+32]
///
/// Winsock setsockopt returns a 32-bit `int` status (0 success, `SOCKET_ERROR` = -1 on
/// failure); `stream_set_timeout.rs:75` sign-tests it (`cmp rax,0; jl`), so `cdqe`
/// sign-extends a -1 failure into a 64-bit negative instead of a false-positive success.
///
/// F5 fix (Winsock sockopt-value translation): elephc's x86_64 socket-option emitters
/// (`stream_set_timeout.rs`, `apply_socket_opts.rs`, `stream_socket_server_v6.rs`) stage
/// raw POSIX/Linux `level`/`optname` numbers before the `syscall(54)` that
/// `windows_transform.rs` rewrites into `call __rt_sys_setsockopt`; those numbers do NOT
/// match Winsock's, so forwarding them unmodified silently sets the wrong (or an invalid)
/// option. This shim now translates the level+optname pairs elephc actually emits before
/// calling Winsock `setsockopt`:
/// - level: POSIX `SOL_SOCKET`(1) → Winsock `0xFFFF`. Any other level (e.g. POSIX
///   `IPPROTO_TCP`=6, used by `TCP_NODELAY`, or `IPPROTO_IPV6`=41) is already numerically
///   identical on Winsock, so it is left unchanged.
/// - optname (only when the ORIGINAL level was `SOL_SOCKET`): POSIX `SO_RCVTIMEO`(20) →
///   `0x1006`, `SO_SNDTIMEO`(21) → `0x1005`, `SO_REUSEADDR`(2) → `4`, `SO_KEEPALIVE`(9) →
///   `8`, `SO_BROADCAST`(6) → `0x20`, `SO_REUSEPORT`(15) → `4`. `SO_KEEPALIVE`/
///   `SO_SNDTIMEO` have no elephc consumer today but are translated anyway per the
///   audited mapping table, since a future consumer would otherwise silently inherit the
///   bug. `SO_REUSEPORT` has no Winsock equivalent (`WSAENOPROTOOPT` if forwarded raw), so
///   it is mapped to Winsock `SO_REUSEADDR`(4) — the exact substitution php-src uses on
///   Windows for equivalent address-reuse; double-setting `SO_REUSEADDR` (when elephc also
///   emits optname 2) is idempotent/harmless. Any other `SOL_SOCKET` optname passes
///   through UNCHANGED rather than being silently dropped.
/// - optname (only when the ORIGINAL level was `IPPROTO_IPV6`=41, itself identical on
///   Winsock): POSIX `IPV6_V6ONLY`(26) → Winsock `27`; the payload is a plain 4-byte int
///   (no conversion). Any other `IPPROTO_IPV6` optname passes through UNCHANGED.
/// - payload for `SO_RCVTIMEO`/`SO_SNDTIMEO` ONLY: php-src's Windows trap — Winsock takes
///   a plain `DWORD` millisecond count for these two options, not a `struct timeval`. The
///   shim reads the POSIX `timeval` elephc already built at `[r10]` (`tv_sec`@+0,
///   `tv_usec`@+8, 8 bytes each), computes `ms = tv_sec*1000 + tv_usec/1000` into a stack
///   DWORD (in the frame's already-reserved-but-unused `[rsp+40..56)` alignment padding),
///   and redirects `optval`/`optlen` to that DWORD (`optlen=4`) before the Winsock call.
///   Every other option's payload (a 4-byte `int`) is already binary-compatible and is
///   forwarded unchanged.
///
/// F5 follow-up fix (rdx clobber in the timeout payload): the ms-conversion above divides
/// `tv_usec` by 1000 via `cqo`/`idiv`, and both destructively overwrite `rdx` (`cqo`
/// sign-extends `rax` into `rdx:rax`; `idiv` then leaves the remainder in `rdx`) — but
/// `rdx` is exactly where the translated Winsock optname (`0x1006`/`0x1005`) was staged
/// just before the jump into this block. Left alone, the optname is silently replaced by
/// `tv_usec % 1000` and Winsock receives garbage. The shim now stashes `rdx` to the
/// unused `[rsp+40]` pad slot before `cqo` and restores it immediately after `idiv`, so
/// the optname survives to `.Lsetsockopt_after_optname`'s `mov r8, rdx`.
pub(super) fn emit_shim_setsockopt(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_setsockopt");
    emitter.instruction("sub rsp, 56");                                         // shadow(32) + stack arg(8) + alignment(16)
    // -- translate POSIX level/optname to Winsock values (SOL_SOCKET + IPPROTO_IPV6) --
    emitter.instruction("cmp rsi, 1");                                          // level == POSIX SOL_SOCKET?
    emitter.instruction("je .Lsetsockopt_sol_socket");                          // -> SOL_SOCKET optname table
    emitter.instruction("cmp rsi, 41");                                         // level == POSIX IPPROTO_IPV6?
    emitter.instruction("je .Lsetsockopt_ipv6");                                // -> IPPROTO_IPV6 optname table
    emitter.instruction("jmp .Lsetsockopt_after_optname");                      // other levels (e.g. IPPROTO_TCP=6) need no translation
    emitter.label(".Lsetsockopt_sol_socket");
    emitter.instruction("cmp rdx, 20");                                         // optname == POSIX SO_RCVTIMEO?
    emitter.instruction("je .Lsetsockopt_is_rcvtimeo");                         // -> Winsock SO_RCVTIMEO + ms payload
    emitter.instruction("cmp rdx, 21");                                         // optname == POSIX SO_SNDTIMEO?
    emitter.instruction("je .Lsetsockopt_is_sndtimeo");                         // -> Winsock SO_SNDTIMEO + ms payload
    emitter.instruction("cmp rdx, 2");                                          // optname == POSIX SO_REUSEADDR?
    emitter.instruction("je .Lsetsockopt_is_reuseaddr");                        // -> Winsock SO_REUSEADDR
    emitter.instruction("cmp rdx, 9");                                          // optname == POSIX SO_KEEPALIVE?
    emitter.instruction("je .Lsetsockopt_is_keepalive");                        // -> Winsock SO_KEEPALIVE
    emitter.instruction("cmp rdx, 6");                                          // optname == POSIX SO_BROADCAST?
    emitter.instruction("je .Lsetsockopt_is_broadcast");                        // -> Winsock SO_BROADCAST
    emitter.instruction("cmp rdx, 15");                                         // optname == POSIX SO_REUSEPORT?
    emitter.instruction("je .Lsetsockopt_is_reuseport");                        // -> Winsock SO_REUSEADDR (no SO_REUSEPORT on Win)
    emitter.instruction("jmp .Lsetsockopt_after_optname");                      // unknown SOL_SOCKET optname: pass through unchanged
    emitter.label(".Lsetsockopt_is_rcvtimeo");
    emitter.instruction("mov rdx, 0x1006");                                     // Winsock SO_RCVTIMEO
    emitter.instruction("jmp .Lsetsockopt_timeout_payload");                    // apply the shared ms-payload conversion
    emitter.label(".Lsetsockopt_is_sndtimeo");
    emitter.instruction("mov rdx, 0x1005");                                     // Winsock SO_SNDTIMEO
    emitter.instruction("jmp .Lsetsockopt_timeout_payload");                    // apply the shared ms-payload conversion
    emitter.label(".Lsetsockopt_is_reuseaddr");
    emitter.instruction("mov rdx, 4");                                          // Winsock SO_REUSEADDR
    emitter.instruction("jmp .Lsetsockopt_after_optname");                      // payload already a 4-byte int: no conversion
    emitter.label(".Lsetsockopt_is_keepalive");
    emitter.instruction("mov rdx, 8");                                          // Winsock SO_KEEPALIVE
    emitter.instruction("jmp .Lsetsockopt_after_optname");                      // payload already a 4-byte int: no conversion
    emitter.label(".Lsetsockopt_is_broadcast");
    emitter.instruction("mov rdx, 0x20");                                       // Winsock SO_BROADCAST
    emitter.instruction("jmp .Lsetsockopt_after_optname");                      // payload already a 4-byte int: no conversion
    emitter.label(".Lsetsockopt_is_reuseport");
    emitter.instruction("mov rdx, 4");                                          // Winsock SO_REUSEADDR: php-src maps SO_REUSEPORT here on Windows
    emitter.instruction("jmp .Lsetsockopt_after_optname");                      // payload already a 4-byte int: no conversion
    emitter.label(".Lsetsockopt_ipv6");
    emitter.instruction("cmp rdx, 26");                                         // optname == POSIX IPV6_V6ONLY?
    emitter.instruction("je .Lsetsockopt_is_v6only");                           // -> Winsock IPV6_V6ONLY (27)
    emitter.instruction("jmp .Lsetsockopt_after_optname");                      // other IPPROTO_IPV6 optnames pass through unchanged
    emitter.label(".Lsetsockopt_is_v6only");
    emitter.instruction("mov rdx, 27");                                         // Winsock IPV6_V6ONLY (Linux 26 -> Windows 27)
    emitter.instruction("jmp .Lsetsockopt_after_optname");                      // payload already a 4-byte int: no conversion
    // -- SO_RCVTIMEO/SO_SNDTIMEO payload: Windows wants a DWORD millisecond --
    // -- count, not a `struct timeval`; convert the POSIX timeval at [r10] --
    emitter.label(".Lsetsockopt_timeout_payload");
    emitter.instruction("mov rax, QWORD PTR [r10 + 8]");                        // tv_usec
    emitter.instruction("mov QWORD PTR [rsp + 40], rdx");                       // stash Winsock optname: cqo/idiv below clobber rdx
    emitter.instruction("cqo");                                                 // sign-extend rax into rdx:rax for idiv
    emitter.instruction("mov r11, 1000");                                       // divisor: microseconds per millisecond
    emitter.instruction("idiv r11");                                            // rax = tv_usec / 1000 (rdx = remainder, discarded)
    emitter.instruction("mov rdx, QWORD PTR [rsp + 40]");                       // restore optname clobbered by cqo/idiv
    emitter.instruction("mov r11, rax");                                        // stash the usec-derived ms
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // tv_sec
    emitter.instruction("imul rax, rax, 1000");                                 // tv_sec * 1000
    emitter.instruction("add rax, r11");                                        // ms = tv_sec*1000 + tv_usec/1000
    emitter.instruction("mov DWORD PTR [rsp + 48], eax");                       // stash ms DWORD in unused frame padding
    emitter.instruction("lea r10, [rsp + 48]");                                 // optval now points at the ms DWORD
    emitter.instruction("mov r8, 4");                                           // optlen = sizeof(DWORD)
    emitter.label(".Lsetsockopt_after_optname");
    emitter.instruction("cmp rsi, 1");                                          // level == POSIX SOL_SOCKET?
    emitter.instruction("jne .Lsetsockopt_after_level");                        // other levels pass through unchanged
    emitter.instruction("mov rsi, 0xffff");                                     // Winsock SOL_SOCKET
    emitter.label(".Lsetsockopt_after_level");
    emitter.instruction("mov QWORD PTR [rsp + 32], r8");                        // optlen → 5th arg (stack)
    emitter.instruction("mov r9, r10");                                         // optval → r9 (4th arg)
    emitter.instruction("mov r8, rdx");                                         // optname → r8 (3rd arg)
    emitter.instruction("mov rdx, rsi");                                        // level → rdx (2nd arg)
    emitter.instruction("mov rcx, rdi");                                        // socket → rcx (1st arg)
    emitter.instruction("call setsockopt");                                     // setsockopt(socket, level, optname, optval, optlen)
    emitter.instruction("cmp eax, -1");                                         // SOCKET_ERROR?
    emitter.instruction("je .Lsetsockopt_capture_errno");                       // publish Winsock option failure
    emitter.instruction("cdqe");                                                // sign-extend eax → rax (SOCKET_ERROR=-1 negative)
    emitter.instruction("add rsp, 56");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lsetsockopt_capture_errno");
    emitter.instruction("call __rt_wsa_capture_errno");                         // capture WSAGetLastError and return -1
    emitter.instruction("add rsp, 56");                                         // restore stack
    emitter.instruction("ret");                                                 // return SOCKET_ERROR
    emitter.blank();
}

/// Emits getsockopt shim — 5 args: socket, level, optname, optval, &optlen.
///
/// SysV: rdi=socket, rsi=level, rdx=optname, r10=optval, r8=&optlen
/// MSx64: rcx, rdx, r8, r9, [rsp+32]
///
/// INVESTIGATED (sign-extension audit): Winsock getsockopt has the same 32-bit int-status
/// shape as setsockopt (0 success, `SOCKET_ERROR` = -1 on failure), but as of this audit
/// there is NO consumer anywhere in the codebase — no PHP builtin lowers to syscall 55
/// (`grep -rn "getsockopt\|socket_get_option" src/` outside this file and the
/// windows_transform.rs syscall-number table returns nothing). The sign-extension triplet
/// (int-status return ∧ a consumer sign-tests it ∧ cdqe absent) fails on the second
/// conjunct: there is no consumer to sign-test it, so `cdqe` is deliberately NOT added
/// here. If a `socket_get_option`/`getsockopt` consumer is ever wired up on the syscall-55
/// path, apply the same `cdqe` fix as `emit_shim_setsockopt` above at that time.
///
/// F5 audit (Winsock sockopt-value translation): `emit_shim_setsockopt` above now
/// translates POSIX `level`/`optname`/timeout-payload values to their Winsock
/// equivalents (elephc staged raw POSIX numbers, which do not match Winsock's). This
/// getsockopt shim is DELIBERATELY left untranslated: it is latent (same "no consumer"
/// finding as the sign-extension audit above), and translating a path nothing calls
/// would be speculative/unverifiable. If a `socket_get_option`/`getsockopt` consumer is
/// ever wired up, apply the same level/optname/payload translation as
/// `emit_shim_setsockopt` at that time — do not assume this shim is Winsock-correct
/// until then.
pub(super) fn emit_shim_getsockopt(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_getsockopt");
    emitter.instruction("sub rsp, 56");                                         // shadow(32) + stack arg(8) + alignment(16)
    emitter.instruction("mov QWORD PTR [rsp + 32], r8");                        // &optlen → 5th arg (stack)
    emitter.instruction("mov r9, r10");                                         // optval → r9 (4th arg)
    emitter.instruction("mov r8, rdx");                                         // optname → r8 (3rd arg)
    emitter.instruction("mov rdx, rsi");                                        // level → rdx (2nd arg)
    emitter.instruction("mov rcx, rdi");                                        // socket → rcx (1st arg)
    emitter.instruction("call getsockopt");                                     // getsockopt(socket, level, optname, optval, &optlen)
    emitter.instruction("cmp eax, -1");                                         // SOCKET_ERROR?
    emitter.instruction("je .Lgetsockopt_capture_errno");                       // publish Winsock option failure
    emitter.instruction("cdqe");                                                // sign-extend 32-bit status
    emitter.instruction("add rsp, 56");                                         // restore stack
    emitter.instruction("ret");                                                 // return
    emitter.label(".Lgetsockopt_capture_errno");
    emitter.instruction("call __rt_wsa_capture_errno");                         // capture WSAGetLastError and return -1
    emitter.instruction("add rsp, 56");                                         // restore stack
    emitter.instruction("ret");                                                 // return SOCKET_ERROR
    emitter.blank();
}

/// Emits the PHP-compatible Windows `socketpair` loopback implementation.
///
/// php-src accepts only `AF_INET` on Windows and emulates the pair with a
/// loopback listener, client connection, and accepted server socket. The
/// runtime preserves both opaque 64-bit `SOCKET` values in the caller's
/// output pair.
pub(super) fn emit_shim_socketpair(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_socketpair");
    emitter.instruction("sub rsp, 104");                                        // shadow space, sockaddr_in, handles, output, error locals
    emitter.instruction("mov QWORD PTR [rsp + 80], r10");                       // preserve int sv[2] output pointer
    emitter.instruction("mov DWORD PTR [rsp + 92], edx");                       // preserve caller protocol across Winsock calls
    emitter.instruction("cmp edi, 2");                                          // PHP_WIN32 socketpair accepts AF_INET only
    emitter.instruction("jne .Lsocketpair_domain_fail");                        // reject AF_UNIX and every non-IPv4 family
    emitter.instruction("mov rax, rsi");                                        // socket type
    emitter.instruction("and eax, 0xf");                                        // isolate SOCK_TYPE_MASK
    emitter.instruction("cmp eax, 1");                                          // SOCK_STREAM?
    emitter.instruction("jne .Lsocketpair_domain_fail");                        // only stream pairs are emulated
    emitter.instruction("mov QWORD PTR [rsp + 56], -1");                        // listener = INVALID_SOCKET
    emitter.instruction("mov QWORD PTR [rsp + 64], -1");                        // client = INVALID_SOCKET
    emitter.instruction("mov QWORD PTR [rsp + 72], -1");                        // accepted = INVALID_SOCKET
    emitter.instruction("mov WORD PTR [rsp + 32], 2");                          // sockaddr_in.sin_family = AF_INET
    emitter.instruction("mov WORD PTR [rsp + 34], 0");                          // ephemeral port
    emitter.instruction("mov DWORD PTR [rsp + 36], 0x0100007f");                // sin_addr = htonl(INADDR_LOOPBACK)
    emitter.instruction("mov QWORD PTR [rsp + 40], 0");                         // zero sin_zero[8]
    emitter.instruction("mov DWORD PTR [rsp + 48], 16");                        // sockaddr length
    emitter.instruction("mov ecx, 2");                                          // AF_INET
    emitter.instruction("mov edx, 1");                                          // SOCK_STREAM
    emitter.instruction("mov r8d, DWORD PTR [rsp + 92]");                       // caller-selected protocol
    emitter.instruction("call socket");                                         // create loopback listener
    emitter.instruction("cmp rax, -1");                                         // INVALID_SOCKET?
    emitter.instruction("je .Lsocketpair_native_fail");                         // capture creation failure
    emitter.instruction("mov QWORD PTR [rsp + 56], rax");                       // retain listener
    emitter.instruction("mov rcx, rax");                                        // listener socket
    emitter.instruction("lea rdx, [rsp + 32]");                                 // loopback sockaddr
    emitter.instruction("mov r8d, 16");                                         // sockaddr length
    emitter.instruction("call bind");                                           // bind ephemeral loopback port
    emitter.instruction("cmp eax, -1");                                         // SOCKET_ERROR?
    emitter.instruction("je .Lsocketpair_native_fail");                         // cleanup listener
    emitter.instruction("mov rcx, QWORD PTR [rsp + 56]");                       // listener
    emitter.instruction("mov edx, 1");                                          // backlog one
    emitter.instruction("call listen");                                         // begin accepting
    emitter.instruction("cmp eax, -1");                                         // SOCKET_ERROR?
    emitter.instruction("je .Lsocketpair_native_fail");                         // cleanup listener
    emitter.instruction("mov rcx, QWORD PTR [rsp + 56]");                       // listener
    emitter.instruction("lea rdx, [rsp + 32]");                                 // receive bound address
    emitter.instruction("lea r8, [rsp + 48]");                                  // socklen pointer
    emitter.instruction("call getsockname");                                    // discover ephemeral port
    emitter.instruction("cmp eax, -1");                                         // SOCKET_ERROR?
    emitter.instruction("je .Lsocketpair_native_fail");                         // cleanup listener
    emitter.instruction("mov ecx, 2");                                          // AF_INET
    emitter.instruction("mov edx, 1");                                          // SOCK_STREAM
    emitter.instruction("mov r8d, DWORD PTR [rsp + 92]");                       // caller-selected protocol
    emitter.instruction("call socket");                                         // create client endpoint
    emitter.instruction("cmp rax, -1");                                         // INVALID_SOCKET?
    emitter.instruction("je .Lsocketpair_native_fail");                         // cleanup listener
    emitter.instruction("mov QWORD PTR [rsp + 64], rax");                       // retain client
    emitter.instruction("mov rcx, rax");                                        // client socket
    emitter.instruction("lea rdx, [rsp + 32]");                                 // listener loopback address
    emitter.instruction("mov r8d, 16");                                         // sockaddr length
    emitter.instruction("call connect");                                        // connect client to listener
    emitter.instruction("cmp eax, -1");                                         // SOCKET_ERROR?
    emitter.instruction("je .Lsocketpair_native_fail");                         // cleanup both sockets
    emitter.instruction("mov rcx, QWORD PTR [rsp + 56]");                       // listener socket
    emitter.instruction("xor edx, edx");                                        // no peer address requested
    emitter.instruction("xor r8d, r8d");                                        // no peer address length
    emitter.instruction("call accept");                                         // accept connected server endpoint
    emitter.instruction("cmp rax, -1");                                         // INVALID_SOCKET?
    emitter.instruction("je .Lsocketpair_native_fail");                         // cleanup client/listener
    emitter.instruction("mov QWORD PTR [rsp + 72], rax");                       // retain accepted endpoint
    emitter.instruction("mov rcx, QWORD PTR [rsp + 56]");                       // listener no longer needed
    emitter.instruction("call closesocket");                                    // close listener
    emitter.instruction("mov r10, QWORD PTR [rsp + 80]");                       // int sv[2] output
    emitter.instruction("mov rax, QWORD PTR [rsp + 64]");                       // full-width client SOCKET
    emitter.instruction("mov QWORD PTR [r10], rax");                            // sv[0] = client
    emitter.instruction("mov rax, QWORD PTR [rsp + 72]");                       // full-width accepted SOCKET
    emitter.instruction("mov QWORD PTR [r10 + 8], rax");                        // sv[1] = server
    emitter.instruction("xor eax, eax");                                        // POSIX success
    emitter.instruction("add rsp, 104");                                        // restore stack
    emitter.instruction("ret");                                                 // return 0
    emitter.label(".Lsocketpair_native_fail");
    emitter.instruction("call WSAGetLastError");                                // capture failure before cleanup calls
    emitter.instruction("mov DWORD PTR [rsp + 88], eax");                       // preserve native error
    for (offset, skip) in [(72, "server"), (64, "client"), (56, "listener")] {
        emitter.instruction(&format!("mov rcx, QWORD PTR [rsp + {offset}]"));   // socket candidate
        emitter.instruction("cmp rcx, -1");                                     // allocated endpoint?
        emitter.instruction(&format!("je .Lsocketpair_skip_{skip}"));           // skip invalid socket
        emitter.instruction("call closesocket");                                // release endpoint
        emitter.label(&format!(".Lsocketpair_skip_{skip}"));
    }
    emitter.instruction("mov eax, DWORD PTR [rsp + 88]");                       // restore WSA error
    emitter.instruction("mov DWORD PTR [rip + __rt_wsa_last_error], eax");      // retain native Winsock state
    emitter.instruction("call __rt_win32_errno_from_code");                     // translate POSIX errno
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], eax");               // publish errno
    emitter.instruction("mov rax, -1");                                         // POSIX failure
    emitter.instruction("add rsp, 104");                                        // restore stack
    emitter.instruction("ret");                                                 // return -1
    emitter.label(".Lsocketpair_domain_fail");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 92");                // ENOPROTOOPT, matching socketpair_win32
    emitter.instruction("mov rax, -1");                                         // reject unsupported pair family/type
    emitter.instruction("add rsp, 104");                                        // restore stack
    emitter.instruction("ret");                                                 // return -1
    emitter.blank();
}

/// Emits the `__rt_sys_pselect6` shim: converts the Linux `pselect6` syscall
/// (syscall 270) to ws2_32 `select`. elephc's `__rt_stream_select` x86_64 path
/// builds Linux fd_set bitmaps (one qword per set, nfds=64) and emits
/// `syscall(270)` with SysV args, which the Windows syscall transform routes
/// here as a normal `call`.
///
/// SysV args received (passed in SysV registers by the transform):
/// - `edi` = nfds (int; elephc always passes 64 — the bitmap is one qword)
/// - `rsi` = readfds (Linux fd_set* bitmap; NULL allowed)
/// - `rdx` = writefds (Linux fd_set* bitmap; NULL allowed)
/// - `r10` = exceptfds (Linux fd_set* bitmap; NULL allowed)
/// - `r8`  = timeout (struct timespec* : sec@0 i64, nsec@8 i64; NULL = block)
/// - `r9`  = sigmask (NULL — ignored entirely)
///
/// Conversion:
/// - Each non-NULL Linux fd_set bitmap (qword) is converted into a Windows
///   `fd_set` (winsock2, 520 bytes: `u_int fd_count` @0, 4-byte pad, `SOCKET
///   fd_array[64]` @8). For each set bit `b` (0..nfds-1), `b` is appended to
///   `fd_array` and `fd_count` incremented. NULL sets pass NULL to `select`.
/// - The Linux `struct timespec` (sec i64, nsec i64) is converted to the
///   Windows `struct timeval` (tv_sec i32 @0, tv_usec i32 @4): tv_sec = sec
///   (low 32 bits), tv_usec = nsec / 1000. A NULL timeout passes NULL (block).
/// - After `select` returns: on SOCKET_ERROR (-1), the three Linux bitmaps are
///   zeroed (only the non-NULL ones) and -1 is returned. On success, each
///   non-NULL Linux bitmap is zeroed and rebuilt from the post-select Windows
///   `fd_set` (which `select` rewrites in place to contain only ready
///   descriptors): for each fd in `fd_array[0..fd_count)`, bit `fd` is set in
///   the Linux bitmap qword (`or [linux_fds], 1 << fd`).
///
/// `select` returns its ready count/`SOCKET_ERROR` as a 32-bit `int` in `eax`.
/// The shim sign-extends that result before storing it and compares `eax` with
/// the 32-bit `SOCKET_ERROR` sentinel, so failures enter the errno/bitmap reset
/// path while successful counts remain suitable for the runtime's 64-bit tests.
///
/// Frame: 1688 bytes (`sub rsp, 1688`; 1688 ≡ 8 mod 16, so rsp ≡ 0 at the
/// `call select` since the shim is entered via `call` with rsp ≡ 8). Layout:
/// - [rsp+0..32)    shadow space (MSx64)
/// - [rsp+32]       nfds spill (edi, zero-extended to 64 bits)
/// - [rsp+40]       readfds ptr (rsi)
/// - [rsp+48]       writefds ptr (rdx)
/// - [rsp+56]       exceptfds ptr (r10)
/// - [rsp+64]       timeout ptr (r8)
/// - [rsp+72]       saved select result
/// - [rsp+80]       win_read ptr  (select arg2)
/// - [rsp+88]       win_write ptr (select arg3)
/// - [rsp+96]       win_except ptr (select arg4)
/// - [rsp+104]      win_timeval ptr (select arg5)
/// - [rsp+112]      win_read fd_set (520 bytes)
/// - [rsp+632]      win_write fd_set (520 bytes)
/// - [rsp+1152]     win_except fd_set (520 bytes)
/// - [rsp+1672]     win_timeval (8 bytes: tv_sec i32 @0, tv_usec i32 @4)
/// - [rsp+1680]     padding (8 bytes)
pub(super) fn emit_shim_pselect6(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_pselect6");
    // -- frame: 1688 bytes; 1688 ≡ 8 mod 16 keeps rsp ≡ 0 at the ws2_32 select call --
    emitter.instruction("sub rsp, 1688");                                       // allocate frame (1688 ≡ 8 mod 16)
    // -- spill incoming SysV args (volatile across fd_set build and select call) --
    emitter.instruction("mov eax, edi");                                        // zero-extend nfds (edi, 32-bit) into rax
    emitter.instruction("mov QWORD PTR [rsp + 32], rax");                       // spill nfds
    emitter.instruction("mov QWORD PTR [rsp + 40], rsi");                       // spill readfds ptr
    emitter.instruction("mov QWORD PTR [rsp + 48], rdx");                       // spill writefds ptr
    emitter.instruction("mov QWORD PTR [rsp + 56], r10");                       // spill exceptfds ptr
    emitter.instruction("mov QWORD PTR [rsp + 64], r8");                        // spill timeout ptr
    // -- build win_read fd_set from Linux readfds bitmap (rsi) --
    emitter.instruction("mov rax, QWORD PTR [rsp + 40]");                       // readfds ptr
    emitter.instruction("test rax, rax");                                       // NULL?
    emitter.instruction("jz .Lpselect6_read_null");                             // → pass NULL to select
    emitter.instruction("lea rdi, [rsp + 112]");                                // win_read fd_set base
    emitter.instruction("xor eax, eax");                                        // fill value = 0
    emitter.instruction("mov rcx, 65");                                         // 65 qwords = 520 bytes
    emitter.instruction("rep stosq");                                           // zero win_read fd_set (fd_count + array)
    emitter.instruction("mov rax, QWORD PTR [rsp + 40]");                       // reload readfds ptr (clobbered by rep stosq)
    emitter.instruction("mov r11, QWORD PTR [rax]");                            // Linux bitmap qword (bits 0..63)
    emitter.instruction("lea rdi, [rsp + 112]");                                // win_read fd_set base (for array writes)
    emitter.instruction("xor r10d, r10d");                                      // bit counter b = 0
    emitter.label(".Lpselect6_read_loop");
    emitter.instruction("cmp r10d, 64");                                        // b < 64?
    emitter.instruction("jge .Lpselect6_read_done");                            // → done scanning bitmap
    emitter.instruction("mov rdx, 1");                                          // rdx = 1
    emitter.instruction("mov ecx, r10d");                                       // shift count = b (32-bit mov zero-extends to rcx)
    emitter.instruction("shl rdx, cl");                                         // rdx = 1 << b
    emitter.instruction("test r11, rdx");                                       // bit b set in Linux bitmap?
    emitter.instruction("jz .Lpselect6_read_next");                             // → skip
    emitter.instruction("mov ecx, DWORD PTR [rdi]");                            // fd_count (u_int @0)
    emitter.instruction("lea rax, [rip + _win_select_read_handles]");           // bit-to-SOCKET bridge table
    emitter.instruction("mov rax, QWORD PTR [rax + r10 * 8]");                  // recover full-width opaque SOCKET
    emitter.instruction("mov QWORD PTR [rdi + 8 + rcx*8], rax");                // fd_array[fd_count] = SOCKET
    emitter.instruction("inc ecx");                                             // fd_count++
    emitter.instruction("mov DWORD PTR [rdi], ecx");                            // store updated fd_count
    emitter.label(".Lpselect6_read_next");
    emitter.instruction("inc r10d");                                            // b++
    emitter.instruction("jmp .Lpselect6_read_loop");                            // next bit
    emitter.label(".Lpselect6_read_done");
    emitter.instruction("lea rax, [rsp + 112]");                                // win_read ptr for select
    emitter.instruction("mov QWORD PTR [rsp + 80], rax");                       // store select arg2
    emitter.instruction("jmp .Lpselect6_read_end");                             // skip NULL path
    emitter.label(".Lpselect6_read_null");
    emitter.instruction("mov QWORD PTR [rsp + 80], 0");                         // pass NULL for readfds
    emitter.label(".Lpselect6_read_end");
    // -- build win_write fd_set from Linux writefds bitmap (rdx) --
    emitter.instruction("mov rax, QWORD PTR [rsp + 48]");                       // writefds ptr
    emitter.instruction("test rax, rax");                                       // NULL?
    emitter.instruction("jz .Lpselect6_write_null");                            // → pass NULL to select
    emitter.instruction("lea rdi, [rsp + 632]");                                // win_write fd_set base
    emitter.instruction("xor eax, eax");                                        // fill value = 0
    emitter.instruction("mov rcx, 65");                                         // 65 qwords = 520 bytes
    emitter.instruction("rep stosq");                                           // zero win_write fd_set
    emitter.instruction("mov rax, QWORD PTR [rsp + 48]");                       // reload writefds ptr
    emitter.instruction("mov r11, QWORD PTR [rax]");                            // Linux bitmap qword
    emitter.instruction("lea rdi, [rsp + 632]");                                // win_write fd_set base
    emitter.instruction("xor r10d, r10d");                                      // bit counter b = 0
    emitter.label(".Lpselect6_write_loop");
    emitter.instruction("cmp r10d, 64");                                        // b < 64?
    emitter.instruction("jge .Lpselect6_write_done");                           // → done
    emitter.instruction("mov rdx, 1");                                          // rdx = 1
    emitter.instruction("mov ecx, r10d");                                       // shift count = b (32-bit mov zero-extends to rcx)
    emitter.instruction("shl rdx, cl");                                         // rdx = 1 << b
    emitter.instruction("test r11, rdx");                                       // bit b set?
    emitter.instruction("jz .Lpselect6_write_next");                            // → skip
    emitter.instruction("mov ecx, DWORD PTR [rdi]");                            // fd_count
    emitter.instruction("lea rax, [rip + _win_select_write_handles]");          // bit-to-SOCKET bridge table
    emitter.instruction("mov rax, QWORD PTR [rax + r10 * 8]");                  // recover full-width opaque SOCKET
    emitter.instruction("mov QWORD PTR [rdi + 8 + rcx*8], rax");                // fd_array[fd_count] = SOCKET
    emitter.instruction("inc ecx");                                             // fd_count++
    emitter.instruction("mov DWORD PTR [rdi], ecx");                            // store fd_count
    emitter.label(".Lpselect6_write_next");
    emitter.instruction("inc r10d");                                            // b++
    emitter.instruction("jmp .Lpselect6_write_loop");                           // next bit
    emitter.label(".Lpselect6_write_done");
    emitter.instruction("lea rax, [rsp + 632]");                                // win_write ptr for select
    emitter.instruction("mov QWORD PTR [rsp + 88], rax");                       // store select arg3
    emitter.instruction("jmp .Lpselect6_write_end");                            // skip NULL path
    emitter.label(".Lpselect6_write_null");
    emitter.instruction("mov QWORD PTR [rsp + 88], 0");                         // pass NULL for writefds
    emitter.label(".Lpselect6_write_end");
    // -- build win_except fd_set from Linux exceptfds bitmap (r10) --
    emitter.instruction("mov rax, QWORD PTR [rsp + 56]");                       // exceptfds ptr
    emitter.instruction("test rax, rax");                                       // NULL?
    emitter.instruction("jz .Lpselect6_except_null");                           // → pass NULL to select
    emitter.instruction("lea rdi, [rsp + 1152]");                               // win_except fd_set base
    emitter.instruction("xor eax, eax");                                        // fill value = 0
    emitter.instruction("mov rcx, 65");                                         // 65 qwords = 520 bytes
    emitter.instruction("rep stosq");                                           // zero win_except fd_set
    emitter.instruction("mov rax, QWORD PTR [rsp + 56]");                       // reload exceptfds ptr
    emitter.instruction("mov r11, QWORD PTR [rax]");                            // Linux bitmap qword
    emitter.instruction("lea rdi, [rsp + 1152]");                               // win_except fd_set base
    emitter.instruction("xor r10d, r10d");                                      // bit counter b = 0
    emitter.label(".Lpselect6_except_loop");
    emitter.instruction("cmp r10d, 64");                                        // b < 64?
    emitter.instruction("jge .Lpselect6_except_done");                          // → done
    emitter.instruction("mov rdx, 1");                                          // rdx = 1
    emitter.instruction("mov ecx, r10d");                                       // shift count = b (32-bit mov zero-extends to rcx)
    emitter.instruction("shl rdx, cl");                                         // rdx = 1 << b
    emitter.instruction("test r11, rdx");                                       // bit b set?
    emitter.instruction("jz .Lpselect6_except_next");                           // → skip
    emitter.instruction("mov ecx, DWORD PTR [rdi]");                            // fd_count
    emitter.instruction("lea rax, [rip + _win_select_except_handles]");         // bit-to-SOCKET bridge table
    emitter.instruction("mov rax, QWORD PTR [rax + r10 * 8]");                  // recover full-width opaque SOCKET
    emitter.instruction("mov QWORD PTR [rdi + 8 + rcx*8], rax");                // fd_array[fd_count] = SOCKET
    emitter.instruction("inc ecx");                                             // fd_count++
    emitter.instruction("mov DWORD PTR [rdi], ecx");                            // store fd_count
    emitter.label(".Lpselect6_except_next");
    emitter.instruction("inc r10d");                                            // b++
    emitter.instruction("jmp .Lpselect6_except_loop");                          // next bit
    emitter.label(".Lpselect6_except_done");
    emitter.instruction("lea rax, [rsp + 1152]");                               // win_except ptr for select
    emitter.instruction("mov QWORD PTR [rsp + 96], rax");                       // store select arg4
    emitter.instruction("jmp .Lpselect6_except_end");                           // skip NULL path
    emitter.label(".Lpselect6_except_null");
    emitter.instruction("mov QWORD PTR [rsp + 96], 0");                         // pass NULL for exceptfds
    emitter.label(".Lpselect6_except_end");
    // -- build win_timeval from Linux struct timespec (r8) --
    emitter.instruction("mov rax, QWORD PTR [rsp + 64]");                       // timeout ptr
    emitter.instruction("test rax, rax");                                       // NULL?
    emitter.instruction("jz .Lpselect6_tv_null");                               // → pass NULL (block indefinitely)
    emitter.instruction("mov rcx, QWORD PTR [rax]");                            // sec (i64 @0)
    emitter.instruction("mov DWORD PTR [rsp + 1672], ecx");                     // tv_sec (low 32 bits @0)
    emitter.instruction("mov rax, QWORD PTR [rax + 8]");                        // nsec (i64 @8)
    emitter.instruction("xor rdx, rdx");                                        // clear high half of dividend
    emitter.instruction("mov ecx, 1000");                                       // divisor: 1000 (nsec → usec)
    emitter.instruction("div rcx");                                             // rax = nsec / 1000 = tv_usec
    emitter.instruction("mov DWORD PTR [rsp + 1676], eax");                     // tv_usec (32-bit @4)
    emitter.instruction("lea rax, [rsp + 1672]");                               // win_timeval ptr
    emitter.instruction("mov QWORD PTR [rsp + 104], rax");                      // store select arg5
    emitter.instruction("jmp .Lpselect6_tv_end");                               // skip NULL path
    emitter.label(".Lpselect6_tv_null");
    emitter.instruction("mov QWORD PTR [rsp + 104], 0");                        // pass NULL timeout (block)
    emitter.label(".Lpselect6_tv_end");
    // -- materialize MSx64 args and call ws2_32 select --
    emitter.instruction("mov rcx, QWORD PTR [rsp + 32]");                       // nfds (select arg1)
    emitter.instruction("mov rdx, QWORD PTR [rsp + 80]");                       // readfds (select arg2)
    emitter.instruction("mov r8, QWORD PTR [rsp + 88]");                        // writefds (select arg3)
    emitter.instruction("mov r9, QWORD PTR [rsp + 96]");                        // exceptfds (select arg4)
    emitter.instruction("mov rax, QWORD PTR [rsp + 104]");                      // win_timeval ptr
    emitter.instruction("mov QWORD PTR [rsp + 32], rax");                       // select arg5 (5th arg at [rsp+32])
    emitter.instruction("call select");                                         // ws2_32 select → eax (ready count or SOCKET_ERROR)
    emitter.instruction("movsxd rax, eax");                                     // widen the signed Win32 int result before saving it
    emitter.instruction("mov QWORD PTR [rsp + 72], rax");                       // save result
    emitter.instruction("cmp eax, -1");                                         // SOCKET_ERROR is a 32-bit int sentinel
    emitter.instruction("je .Lpselect6_error");                                 // → zero Linux bitmaps, return -1
    // -- success: writeback ready fds into the three Linux bitmaps --
    // -- read writeback: zero Linux bitmap, then set bits from win_read fd_array --
    emitter.instruction("mov rax, QWORD PTR [rsp + 40]");                       // readfds ptr
    emitter.instruction("test rax, rax");                                       // NULL (was not built)?
    emitter.instruction("jz .Lpselect6_wb_read_skip");                          // → nothing to write back
    emitter.instruction("mov QWORD PTR [rax], 0");                              // zero Linux read bitmap
    emitter.instruction("lea rdi, [rsp + 112]");                                // win_read fd_set base
    emitter.instruction("mov r11d, DWORD PTR [rdi]");                           // post-select fd_count
    emitter.instruction("xor r10d, r10d");                                      // loop index i = 0
    emitter.label(".Lpselect6_wb_read_loop");
    emitter.instruction("cmp r10d, r11d");                                      // i < fd_count?
    emitter.instruction("jge .Lpselect6_wb_read_done");                         // → done
    emitter.instruction("mov r9, QWORD PTR [rdi + 8 + r10*8]");                 // fd = fd_array[i] (SOCKET, 8 bytes)
    emitter.instruction("xor ecx, ecx");                                        // bridge slot search index
    emitter.label(".Lpselect6_wb_read_find");
    emitter.instruction("cmp ecx, 64");                                         // searched every registered input slot?
    emitter.instruction("jge .Lpselect6_wb_read_advance");                      // ignore an unknown Winsock result
    emitter.instruction("lea rax, [rip + _win_select_read_handles]");           // read-set SOCKET table
    emitter.instruction("cmp QWORD PTR [rax + rcx * 8], r9");                   // this slot owns the returned SOCKET?
    emitter.instruction("je .Lpselect6_wb_read_found");                         // set its original bitmap position
    emitter.instruction("inc ecx");                                             // try next slot
    emitter.instruction("jmp .Lpselect6_wb_read_find");                         // continue reverse lookup
    emitter.label(".Lpselect6_wb_read_found");
    emitter.instruction("mov rax, 1");                                          // rax = 1
    emitter.instruction("shl rax, cl");                                         // rax = 1 << fd
    emitter.instruction("mov rdx, QWORD PTR [rsp + 40]");                       // readfds ptr
    emitter.instruction("or QWORD PTR [rdx], rax");                             // set bit fd in Linux read bitmap
    emitter.label(".Lpselect6_wb_read_advance");
    emitter.instruction("inc r10d");                                            // i++
    emitter.instruction("jmp .Lpselect6_wb_read_loop");                         // next fd
    emitter.label(".Lpselect6_wb_read_done");
    emitter.label(".Lpselect6_wb_read_skip");
    // -- write writeback: zero Linux bitmap, then set bits from win_write fd_array --
    emitter.instruction("mov rax, QWORD PTR [rsp + 48]");                       // writefds ptr
    emitter.instruction("test rax, rax");                                       // NULL?
    emitter.instruction("jz .Lpselect6_wb_write_skip");                         // → nothing to write back
    emitter.instruction("mov QWORD PTR [rax], 0");                              // zero Linux write bitmap
    emitter.instruction("lea rdi, [rsp + 632]");                                // win_write fd_set base
    emitter.instruction("mov r11d, DWORD PTR [rdi]");                           // post-select fd_count
    emitter.instruction("xor r10d, r10d");                                      // loop index i = 0
    emitter.label(".Lpselect6_wb_write_loop");
    emitter.instruction("cmp r10d, r11d");                                      // i < fd_count?
    emitter.instruction("jge .Lpselect6_wb_write_done");                        // → done
    emitter.instruction("mov r9, QWORD PTR [rdi + 8 + r10*8]");                 // fd = fd_array[i]
    emitter.instruction("xor ecx, ecx");                                        // bridge slot search index
    emitter.label(".Lpselect6_wb_write_find");
    emitter.instruction("cmp ecx, 64");                                         // searched every registered input slot?
    emitter.instruction("jge .Lpselect6_wb_write_advance");                     // ignore an unknown Winsock result
    emitter.instruction("lea rax, [rip + _win_select_write_handles]");          // write-set SOCKET table
    emitter.instruction("cmp QWORD PTR [rax + rcx * 8], r9");                   // this slot owns the returned SOCKET?
    emitter.instruction("je .Lpselect6_wb_write_found");                        // set its original bitmap position
    emitter.instruction("inc ecx");                                             // try next slot
    emitter.instruction("jmp .Lpselect6_wb_write_find");                        // continue reverse lookup
    emitter.label(".Lpselect6_wb_write_found");
    emitter.instruction("mov rax, 1");                                          // rax = 1
    emitter.instruction("shl rax, cl");                                         // rax = 1 << fd
    emitter.instruction("mov rdx, QWORD PTR [rsp + 48]");                       // writefds ptr
    emitter.instruction("or QWORD PTR [rdx], rax");                             // set bit fd in Linux write bitmap
    emitter.label(".Lpselect6_wb_write_advance");
    emitter.instruction("inc r10d");                                            // i++
    emitter.instruction("jmp .Lpselect6_wb_write_loop");                        // next fd
    emitter.label(".Lpselect6_wb_write_done");
    emitter.label(".Lpselect6_wb_write_skip");
    // -- except writeback: zero Linux bitmap, then set bits from win_except fd_array --
    emitter.instruction("mov rax, QWORD PTR [rsp + 56]");                       // exceptfds ptr
    emitter.instruction("test rax, rax");                                       // NULL?
    emitter.instruction("jz .Lpselect6_wb_except_skip");                        // → nothing to write back
    emitter.instruction("mov QWORD PTR [rax], 0");                              // zero Linux except bitmap
    emitter.instruction("lea rdi, [rsp + 1152]");                               // win_except fd_set base
    emitter.instruction("mov r11d, DWORD PTR [rdi]");                           // post-select fd_count
    emitter.instruction("xor r10d, r10d");                                      // loop index i = 0
    emitter.label(".Lpselect6_wb_except_loop");
    emitter.instruction("cmp r10d, r11d");                                      // i < fd_count?
    emitter.instruction("jge .Lpselect6_wb_except_done");                       // → done
    emitter.instruction("mov r9, QWORD PTR [rdi + 8 + r10*8]");                 // fd = fd_array[i]
    emitter.instruction("xor ecx, ecx");                                        // bridge slot search index
    emitter.label(".Lpselect6_wb_except_find");
    emitter.instruction("cmp ecx, 64");                                         // searched every registered input slot?
    emitter.instruction("jge .Lpselect6_wb_except_advance");                    // ignore an unknown Winsock result
    emitter.instruction("lea rax, [rip + _win_select_except_handles]");         // exception-set SOCKET table
    emitter.instruction("cmp QWORD PTR [rax + rcx * 8], r9");                   // this slot owns the returned SOCKET?
    emitter.instruction("je .Lpselect6_wb_except_found");                       // set its original bitmap position
    emitter.instruction("inc ecx");                                             // try next slot
    emitter.instruction("jmp .Lpselect6_wb_except_find");                       // continue reverse lookup
    emitter.label(".Lpselect6_wb_except_found");
    emitter.instruction("mov rax, 1");                                          // rax = 1
    emitter.instruction("shl rax, cl");                                         // rax = 1 << fd
    emitter.instruction("mov rdx, QWORD PTR [rsp + 56]");                       // exceptfds ptr
    emitter.instruction("or QWORD PTR [rdx], rax");                             // set bit fd in Linux except bitmap
    emitter.label(".Lpselect6_wb_except_advance");
    emitter.instruction("inc r10d");                                            // i++
    emitter.instruction("jmp .Lpselect6_wb_except_loop");                       // next fd
    emitter.label(".Lpselect6_wb_except_done");
    emitter.label(".Lpselect6_wb_except_skip");
    // -- common success return: rax = saved select result --
    emitter.instruction("mov rax, QWORD PTR [rsp + 72]");                       // reload saved result
    emitter.instruction("cdqe");                                                // sign-extend eax → rax (SOCKET_ERROR=-1 negative, see doc)
    emitter.instruction("add rsp, 1688");                                       // restore stack
    emitter.instruction("ret");                                                 // return ready count (≥ 0) or -1
    // -- error path (SOCKET_ERROR): zero all non-NULL Linux bitmaps, return -1 --
    emitter.label(".Lpselect6_error");
    emitter.instruction("call __rt_wsa_capture_errno");                         // preserve select's Winsock error before any other calls
    emitter.instruction("mov rax, QWORD PTR [rsp + 40]");                       // readfds ptr
    emitter.instruction("test rax, rax");                                       // NULL?
    emitter.instruction("jz .Lpselect6_err_w_skip");                            // → skip
    emitter.instruction("mov QWORD PTR [rax], 0");                              // zero Linux read bitmap
    emitter.label(".Lpselect6_err_w_skip");
    emitter.instruction("mov rax, QWORD PTR [rsp + 48]");                       // writefds ptr
    emitter.instruction("test rax, rax");                                       // NULL?
    emitter.instruction("jz .Lpselect6_err_x_skip");                            // → skip
    emitter.instruction("mov QWORD PTR [rax], 0");                              // zero Linux write bitmap
    emitter.label(".Lpselect6_err_x_skip");
    emitter.instruction("mov rax, QWORD PTR [rsp + 56]");                       // exceptfds ptr
    emitter.instruction("test rax, rax");                                       // NULL?
    emitter.instruction("jz .Lpselect6_err_ret");                               // → skip
    emitter.instruction("mov QWORD PTR [rax], 0");                              // zero Linux except bitmap
    emitter.label(".Lpselect6_err_ret");
    emitter.instruction("mov rax, -1");                                         // return -1 (SOCKET_ERROR)
    emitter.instruction("add rsp, 1688");                                       // restore stack
    emitter.instruction("ret");                                                 // return -1
    emitter.blank();
}

/// Emits Linux-layout `sendmsg` scatter/gather lowering through `WSASend`.
pub(super) fn emit_shim_sendmsg(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_sendmsg");
    emitter.instruction("sub rsp, 120");                                        // WSASend stack args, WSABUF translation, and error locals
    emitter.instruction("mov QWORD PTR [rsp + 56], rdi");                       // SOCKET
    emitter.instruction("mov QWORD PTR [rsp + 64], rsi");                       // Linux msghdr pointer
    emitter.instruction("mov QWORD PTR [rsp + 72], rdx");                       // send flags
    emitter.instruction("test rsi, rsi");                                       // msghdr present?
    emitter.instruction("jz .Lsendmsg_invalid");                                // EINVAL
    emitter.instruction("cmp QWORD PTR [rsi], 0");                              // msg_name unsupported by WSASend path
    emitter.instruction("jne .Lsendmsg_unsupported");                           // use sendto surface for addressed sends
    emitter.instruction("cmp QWORD PTR [rsi + 32], 0");                         // ancillary control data present?
    emitter.instruction("jne .Lsendmsg_unsupported");                           // Winsock extension path is not exposed here
    emitter.instruction("mov rax, QWORD PTR [rsi + 24]");                       // Linux msg_iovlen
    emitter.instruction("test rax, rax");                                       // empty vector?
    emitter.instruction("jz .Lsendmsg_empty");                                  // zero bytes sent
    emitter.instruction("mov r9d, 0xffffffff");                                 // zero-extended DWORD maximum
    emitter.instruction("cmp rax, r9");                                         // WSASend DWORD buffer count limit
    emitter.instruction("ja .Lsendmsg_invalid");                                // reject narrowing overflow
    emitter.instruction("mov QWORD PTR [rsp + 88], rax");                       // preserve buffer count
    emitter.instruction("shl rax, 4");                                          // sizeof(WSABUF) = 16
    emitter.instruction("call __rt_heap_alloc");                                // allocate Windows-specific WSABUF array
    emitter.instruction("test rax, rax");                                       // allocation succeeded?
    emitter.instruction("jz .Lsendmsg_nomem");                                  // ENOMEM
    emitter.instruction("mov QWORD PTR [rsp + 80], rax");                       // owned WSABUF array
    emitter.instruction("mov QWORD PTR [rsp + 104], 0");                        // translation index
    emitter.label(".Lsendmsg_translate");
    emitter.instruction("mov rcx, QWORD PTR [rsp + 104]");                      // index
    emitter.instruction("cmp rcx, QWORD PTR [rsp + 88]");                       // translated every iovec?
    emitter.instruction("jae .Lsendmsg_call");                                  // invoke Winsock
    emitter.instruction("mov r10, QWORD PTR [rsp + 64]");                       // msghdr
    emitter.instruction("mov r10, QWORD PTR [r10 + 16]");                       // Linux iovec array
    emitter.instruction("mov r9, rcx");                                         // element index
    emitter.instruction("shl r9, 4");                                           // byte offset = index * 16
    emitter.instruction("mov r11, QWORD PTR [r10 + r9 + 8]");                   // iov_len
    emitter.instruction("mov edx, 0xffffffff");                                 // zero-extended ULONG maximum
    emitter.instruction("cmp r11, rdx");                                        // WSABUF.len is ULONG
    emitter.instruction("ja .Lsendmsg_translate_invalid");                      // reject narrowing overflow
    emitter.instruction("mov rax, QWORD PTR [rsp + 80]");                       // WSABUF base
    emitter.instruction("mov DWORD PTR [rax + r9], r11d");                      // WSABUF.len @ +0
    emitter.instruction("mov DWORD PTR [rax + r9 + 4], 0");                     // explicit alignment padding
    emitter.instruction("mov r11, QWORD PTR [r10 + r9]");                       // iov_base
    emitter.instruction("mov QWORD PTR [rax + r9 + 8], r11");                   // WSABUF.buf @ +8
    emitter.instruction("add QWORD PTR [rsp + 104], 1");                        // next element
    emitter.instruction("jmp .Lsendmsg_translate");                             // continue translation
    emitter.label(".Lsendmsg_call");
    emitter.instruction("mov rcx, QWORD PTR [rsp + 56]");                       // SOCKET
    emitter.instruction("mov rdx, QWORD PTR [rsp + 80]");                       // WSABUF array
    emitter.instruction("mov r8d, DWORD PTR [rsp + 88]");                       // buffer count
    emitter.instruction("lea r9, [rsp + 96]");                                  // lpNumberOfBytesSent
    emitter.instruction("mov rax, QWORD PTR [rsp + 72]");                       // flags
    emitter.instruction("mov QWORD PTR [rsp + 32], rax");                       // dwFlags (arg5)
    emitter.instruction("mov QWORD PTR [rsp + 40], 0");                         // lpOverlapped = NULL
    emitter.instruction("mov QWORD PTR [rsp + 48], 0");                         // completion routine = NULL
    emitter.instruction("call WSASend");                                        // synchronous scatter/gather send
    emitter.instruction("cmp eax, -1");                                         // SOCKET_ERROR?
    emitter.instruction("je .Lsendmsg_native_fail");                            // capture before cleanup
    emitter.instruction("mov rax, QWORD PTR [rsp + 80]");                       // owned WSABUF array
    emitter.instruction("call __rt_heap_free");                                 // release translation
    emitter.instruction("mov eax, DWORD PTR [rsp + 96]");                       // bytes sent
    emitter.instruction("add rsp, 120");                                        // restore stack
    emitter.instruction("ret");                                                 // return byte count
    emitter.label(".Lsendmsg_native_fail");
    emitter.instruction("call WSAGetLastError");                                // preserve would-block/timeout/native error
    emitter.instruction("mov DWORD PTR [rsp + 112], eax");                      // native error across cleanup
    emitter.instruction("mov rax, QWORD PTR [rsp + 80]");                       // owned WSABUF array
    emitter.instruction("call __rt_heap_free");                                 // release translation
    emitter.instruction("mov eax, DWORD PTR [rsp + 112]");                      // restore WSA error
    emitter.instruction("mov DWORD PTR [rip + __rt_wsa_last_error], eax");      // retain native state
    emitter.instruction("call __rt_win32_errno_from_code");                     // map timeout/nonblocking errors
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], eax");               // publish errno
    emitter.instruction("mov rax, -1");                                         // POSIX failure
    emitter.instruction("add rsp, 120");                                        // restore stack
    emitter.instruction("ret");                                                 // return -1
    emitter.label(".Lsendmsg_translate_invalid");
    emitter.instruction("mov rax, QWORD PTR [rsp + 80]");                       // owned WSABUF array
    emitter.instruction("call __rt_heap_free");                                 // release partial translation
    emitter.label(".Lsendmsg_invalid");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 22");                // EINVAL
    emitter.instruction("jmp .Lsendmsg_direct_fail");                           // return failure
    emitter.label(".Lsendmsg_unsupported");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 95");                // EOPNOTSUPP for name/control surfaces
    emitter.instruction("jmp .Lsendmsg_direct_fail");                           // return failure
    emitter.label(".Lsendmsg_nomem");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 12");                // ENOMEM
    emitter.label(".Lsendmsg_direct_fail");
    emitter.instruction("mov rax, -1");                                         // POSIX failure
    emitter.instruction("add rsp, 120");                                        // restore stack
    emitter.instruction("ret");                                                 // return -1
    emitter.label(".Lsendmsg_empty");
    emitter.instruction("xor eax, eax");                                        // empty vector sends zero bytes
    emitter.instruction("add rsp, 120");                                        // restore stack
    emitter.instruction("ret");                                                 // return 0
    emitter.blank();
}

/// Emits Linux-layout `recvmsg` scatter/gather lowering through `WSARecv`.
pub(super) fn emit_shim_recvmsg(emitter: &mut Emitter) {
    emit_shim_recvmsg_wsarecv(emitter);
}

/// Translates Linux x86-64 `msghdr`/`iovec` storage into Winsock `WSABUF`
/// storage and performs a synchronous `WSARecv`, preserving Winsock errors
/// across temporary-buffer cleanup before publishing their POSIX equivalent.
fn emit_shim_recvmsg_wsarecv(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_recvmsg");
    emitter.instruction("sub rsp, 120");                                        // WSARecv stack args, WSABUF translation, and error locals
    emitter.instruction("mov QWORD PTR [rsp + 56], rdi");                       // SOCKET
    emitter.instruction("mov QWORD PTR [rsp + 64], rsi");                       // Linux msghdr pointer
    emitter.instruction("mov DWORD PTR [rsp + 100], edx");                      // input/output receive flags
    emitter.instruction("test rsi, rsi");                                       // msghdr present?
    emitter.instruction("jz .Lrecvmsg_invalid");                                // EINVAL
    emitter.instruction("cmp QWORD PTR [rsi], 0");                              // msg_name unsupported by this connected-socket path
    emitter.instruction("jne .Lrecvmsg_unsupported");                           // use recvfrom surface for addressed receives
    emitter.instruction("cmp QWORD PTR [rsi + 32], 0");                         // ancillary control data present?
    emitter.instruction("jne .Lrecvmsg_unsupported");                           // Winsock extension path is not exposed here
    emitter.instruction("mov rax, QWORD PTR [rsi + 24]");                       // Linux msg_iovlen
    emitter.instruction("test rax, rax");                                       // empty vector?
    emitter.instruction("jz .Lrecvmsg_empty");                                  // zero bytes received
    emitter.instruction("mov r9d, 0xffffffff");                                 // zero-extended DWORD maximum
    emitter.instruction("cmp rax, r9");                                         // WSARecv DWORD buffer count limit
    emitter.instruction("ja .Lrecvmsg_invalid");                                // reject narrowing overflow
    emitter.instruction("mov QWORD PTR [rsp + 88], rax");                       // preserve buffer count
    emitter.instruction("shl rax, 4");                                          // sizeof(WSABUF) = 16
    emitter.instruction("call __rt_heap_alloc");                                // allocate Windows-specific WSABUF array
    emitter.instruction("test rax, rax");                                       // allocation succeeded?
    emitter.instruction("jz .Lrecvmsg_nomem");                                  // ENOMEM
    emitter.instruction("mov QWORD PTR [rsp + 80], rax");                       // owned WSABUF array
    emitter.instruction("mov QWORD PTR [rsp + 104], 0");                        // translation index
    emitter.label(".Lrecvmsg_translate");
    emitter.instruction("mov rcx, QWORD PTR [rsp + 104]");                      // index
    emitter.instruction("cmp rcx, QWORD PTR [rsp + 88]");                       // translated every iovec?
    emitter.instruction("jae .Lrecvmsg_call");                                  // invoke Winsock
    emitter.instruction("mov r10, QWORD PTR [rsp + 64]");                       // msghdr
    emitter.instruction("mov r10, QWORD PTR [r10 + 16]");                       // Linux iovec array
    emitter.instruction("mov r9, rcx");                                         // element index
    emitter.instruction("shl r9, 4");                                           // byte offset = index * 16
    emitter.instruction("mov r11, QWORD PTR [r10 + r9 + 8]");                   // iov_len
    emitter.instruction("mov edx, 0xffffffff");                                 // zero-extended ULONG maximum
    emitter.instruction("cmp r11, rdx");                                        // WSABUF.len is ULONG
    emitter.instruction("ja .Lrecvmsg_translate_invalid");                      // reject narrowing overflow
    emitter.instruction("mov rax, QWORD PTR [rsp + 80]");                       // WSABUF base
    emitter.instruction("mov DWORD PTR [rax + r9], r11d");                      // WSABUF.len @ +0
    emitter.instruction("mov DWORD PTR [rax + r9 + 4], 0");                     // explicit alignment padding
    emitter.instruction("mov r11, QWORD PTR [r10 + r9]");                       // iov_base
    emitter.instruction("mov QWORD PTR [rax + r9 + 8], r11");                   // WSABUF.buf @ +8
    emitter.instruction("add QWORD PTR [rsp + 104], 1");                        // next element
    emitter.instruction("jmp .Lrecvmsg_translate");                             // continue translation
    emitter.label(".Lrecvmsg_call");
    emitter.instruction("mov rcx, QWORD PTR [rsp + 56]");                       // SOCKET
    emitter.instruction("mov rdx, QWORD PTR [rsp + 80]");                       // WSABUF array
    emitter.instruction("mov r8d, DWORD PTR [rsp + 88]");                       // buffer count
    emitter.instruction("lea r9, [rsp + 96]");                                  // lpNumberOfBytesRecvd
    emitter.instruction("lea rax, [rsp + 100]");                                // lpFlags
    emitter.instruction("mov QWORD PTR [rsp + 32], rax");                       // flags pointer (arg5)
    emitter.instruction("mov QWORD PTR [rsp + 40], 0");                         // lpOverlapped = NULL
    emitter.instruction("mov QWORD PTR [rsp + 48], 0");                         // completion routine = NULL
    emitter.instruction("call WSARecv");                                        // synchronous scatter/gather receive
    emitter.instruction("cmp eax, -1");                                         // SOCKET_ERROR?
    emitter.instruction("je .Lrecvmsg_native_fail");                            // capture before cleanup
    emitter.instruction("mov r10, QWORD PTR [rsp + 64]");                       // Linux msghdr
    emitter.instruction("mov eax, DWORD PTR [rsp + 100]");                      // Winsock output flags
    emitter.instruction("mov DWORD PTR [r10 + 48], eax");                       // Linux msg_flags @ +48
    emitter.instruction("mov rax, QWORD PTR [rsp + 80]");                       // owned WSABUF array
    emitter.instruction("call __rt_heap_free");                                 // release translation
    emitter.instruction("mov eax, DWORD PTR [rsp + 96]");                       // bytes received
    emitter.instruction("add rsp, 120");                                        // restore stack
    emitter.instruction("ret");                                                 // return byte count
    emitter.label(".Lrecvmsg_native_fail");
    emitter.instruction("call WSAGetLastError");                                // preserve would-block/timeout/native error
    emitter.instruction("mov DWORD PTR [rsp + 112], eax");                      // native error across cleanup
    emitter.instruction("mov rax, QWORD PTR [rsp + 80]");                       // owned WSABUF array
    emitter.instruction("call __rt_heap_free");                                 // release translation
    emitter.instruction("mov eax, DWORD PTR [rsp + 112]");                      // restore WSA error
    emitter.instruction("mov DWORD PTR [rip + __rt_wsa_last_error], eax");      // retain native state
    emitter.instruction("call __rt_win32_errno_from_code");                     // map timeout/nonblocking errors
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], eax");               // publish errno
    emitter.instruction("mov rax, -1");                                         // POSIX failure
    emitter.instruction("add rsp, 120");                                        // restore stack
    emitter.instruction("ret");                                                 // return -1
    emitter.label(".Lrecvmsg_translate_invalid");
    emitter.instruction("mov rax, QWORD PTR [rsp + 80]");                       // owned WSABUF array
    emitter.instruction("call __rt_heap_free");                                 // release partial translation
    emitter.label(".Lrecvmsg_invalid");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 22");                // EINVAL
    emitter.instruction("jmp .Lrecvmsg_direct_fail");                           // return failure
    emitter.label(".Lrecvmsg_unsupported");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 95");                // EOPNOTSUPP for name/control surfaces
    emitter.instruction("jmp .Lrecvmsg_direct_fail");                           // return failure
    emitter.label(".Lrecvmsg_nomem");
    emitter.instruction("mov DWORD PTR [rip + __rt_errno], 12");                // ENOMEM
    emitter.label(".Lrecvmsg_direct_fail");
    emitter.instruction("mov rax, -1");                                         // POSIX failure
    emitter.instruction("add rsp, 120");                                        // restore stack
    emitter.instruction("ret");                                                 // return -1
    emitter.label(".Lrecvmsg_empty");
    emitter.instruction("mov DWORD PTR [rsi + 48], 0");                         // no output flags for an empty vector
    emitter.instruction("xor eax, eax");                                        // empty vector receives zero bytes
    emitter.instruction("add rsp, 120");                                        // restore stack
    emitter.instruction("ret");                                                 // return 0
    emitter.blank();
}

/// Emits the W3e-2 net/dns/inet (ws2_32) + misc (msvcrt) `__rt_sys_*` shim
/// family: `getaddrinfo`, `freeaddrinfo`, `inet_pton`, `inet_ntop`,
/// `gethostbyaddr`, the protocol/service database lookups, `strtoll` (→ `_strtoi64`), `atof` (FP-return), `setlocale`,
/// and the unsupported Windows `chown`/`lchown` shims (see the `windows_c_shim_name`
/// doc-comment for why these are named `__rt_sys_libc_chown`/
/// `__rt_sys_libc_lchown` rather than reusing the pre-existing
/// `__rt_sys_chown`/`__rt_sys_lchown` ENOSYS labels). `dup` reuses the
/// existing `emit_shim_dup_shims` `__rt_sys_dup` shim (no new shim needed).
pub(super) fn emit_shim_net_dns(emitter: &mut Emitter) {
    emit_shim_getaddrinfo(emitter);
    emit_shim_freeaddrinfo(emitter);
    emit_shim_inet_pton(emitter);
    emit_shim_inet_ntop(emitter);
    emit_shim_gethostbyaddr_win(emitter);
    emit_shim_getprotobyname(emitter);
    emit_shim_getprotobynumber(emitter);
    emit_shim_getservbyname(emitter);
    emit_shim_getservbyport(emitter);
    emit_shim_strtoll(emitter);
    emit_shim_atof(emitter);
    emit_shim_setlocale(emitter);
    emit_shim_libc_chown_unsupported(emitter);
}

/// Emits the `__rt_sys_getprotobyname` shim for Winsock's protocol database.
///
/// SysV supplies the NUL-terminated name in `rdi`; Winsock expects it in `rcx`
/// and returns a transient `protoent*` in `rax`.
fn emit_shim_getprotobyname(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_getprotobyname");
    emitter.instruction("sub rsp, 40");                                         // reserve shadow space with MSx64 call alignment
    emitter.instruction("mov rcx, rdi");                                        // move the protocol C string into MSx64 arg1
    emitter.instruction("call getprotobyname");                                 // query Winsock's protocol database
    emitter.instruction("add rsp, 40");                                         // release the call frame
    emitter.instruction("ret");                                                 // return the transient protoent pointer or null
    emitter.blank();
}

/// Emits the `__rt_sys_getprotobynumber` shim for Winsock's protocol database.
///
/// The protocol id remains a 32-bit C `int`, so only `edi` is moved into the
/// first MSx64 integer argument register.
fn emit_shim_getprotobynumber(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_getprotobynumber");
    emitter.instruction("sub rsp, 40");                                         // reserve shadow space with MSx64 call alignment
    emitter.instruction("mov ecx, edi");                                        // move the protocol number into MSx64 arg1
    emitter.instruction("call getprotobynumber");                               // query Winsock's protocol database
    emitter.instruction("add rsp, 40");                                         // release the call frame
    emitter.instruction("ret");                                                 // return the transient protoent pointer or null
    emitter.blank();
}

/// Emits the `__rt_sys_getservbyname` shim for Winsock's services database.
///
/// The service and protocol are independently NUL-terminated by the caller and
/// are shuffled from SysV `rdi`/`rsi` to MSx64 `rcx`/`rdx`.
fn emit_shim_getservbyname(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_getservbyname");
    emitter.instruction("sub rsp, 40");                                         // reserve shadow space with MSx64 call alignment
    emitter.instruction("mov rdx, rsi");                                        // move the protocol C string into MSx64 arg2
    emitter.instruction("mov rcx, rdi");                                        // move the service C string into MSx64 arg1
    emitter.instruction("call getservbyname");                                  // query Winsock's services database
    emitter.instruction("add rsp, 40");                                         // release the call frame
    emitter.instruction("ret");                                                 // return the transient servent pointer or null
    emitter.blank();
}

/// Emits the `__rt_sys_getservbyport` shim for Winsock's services database.
///
/// PHP exposes a host-order port while Winsock expects the low 16 bits in
/// network byte order, matching php-src's `htons((unsigned short) port)` call.
fn emit_shim_getservbyport(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_getservbyport");
    emitter.instruction("sub rsp, 40");                                         // reserve shadow space with MSx64 call alignment
    emitter.instruction("mov eax, edi");                                        // retain the PHP host-order port in a 32-bit scratch register
    emitter.instruction("xchg al, ah");                                         // convert the low 16 bits to network byte order without a CRT dependency
    emitter.instruction("movzx ecx, ax");                                       // pass the widened network-order port as MSx64 arg1
    emitter.instruction("mov rdx, rsi");                                        // move the protocol C string into MSx64 arg2
    emitter.instruction("call getservbyport");                                  // query Winsock's services database
    emitter.instruction("add rsp, 40");                                         // release the call frame
    emitter.instruction("ret");                                                 // return the transient servent pointer or null
    emitter.blank();
}

/// Emits the `__rt_sys_getaddrinfo` shim: converts SysV `getaddrinfo(node,
/// service, *hints, **res)` (rdi, rsi, rdx, rcx) to MSx64 `getaddrinfo`
/// (rcx=node, rdx=service, r8=hints, r9=res). Register-shuffle hazard: SysV
/// arg4 (res) is in `rcx`, which is ALSO the MSx64 arg1 target, so it is
/// saved to `r9` BEFORE `rcx` is overwritten by the arg1 shuffle. SysV arg3
/// (hints) is in `rdx`, which is ALSO MSx64 arg2, so it is saved to `r8`
/// BEFORE `rdx` is overwritten by the arg2 shuffle. `rdx`←`rsi` (service) and
/// `rcx`←`rdi` (node) move last. Returns an `int` status (0 success) in
/// `eax`; every consumer (`resolve_host_v6.rs:152`) does `test rax,rax; jnz`
/// — no cdqe (a nonzero error code stays nonzero whether or not it is
/// sign-extended; the check never distinguishes sign).
fn emit_shim_getaddrinfo(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_getaddrinfo");
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + alignment(8)
    emitter.instruction("mov r9, rcx");                                         // SAVE res (SysV arg4) before rcx is overwritten
    emitter.instruction("mov r8, rdx");                                         // SAVE hints (SysV arg3) before rdx is overwritten
    emitter.instruction("mov rdx, rsi");                                        // service → arg2 (rdx)
    emitter.instruction("mov rcx, rdi");                                        // node → arg1 (rcx)
    emitter.instruction("call getaddrinfo");                                    // ws2_32 getaddrinfo (returns int status in eax)
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return (no cdqe — status only test-for-nonzero)
    emitter.blank();
}

/// Emits the `__rt_sys_freeaddrinfo` shim: converts SysV `freeaddrinfo(*res)`
/// (rdi) to MSx64 `freeaddrinfo` (rcx=res). Mirrors `emit_shim_zlib_trivial_1arg`
/// (1-arg case). `void` return — no cdqe. Sites: `resolve_host_v6.rs:171,180`.
fn emit_shim_freeaddrinfo(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_freeaddrinfo");
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + alignment(8)
    emitter.instruction("mov rcx, rdi");                                        // res → arg1 (rcx)
    emitter.instruction("call freeaddrinfo");                                   // ws2_32 freeaddrinfo (void return)
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return (void — no cdqe)
    emitter.blank();
}

/// Emits the `__rt_sys_inet_pton` shim: converts SysV `inet_pton(af, src,
/// dst)` (edi, rsi, rdx) to MSx64 `inet_pton` (ecx=af, rdx=src, r8=dst).
/// Register-shuffle hazard: SysV arg3 (dst) is in `rdx`, which is ALSO the
/// MSx64 arg2 (src) target, so it is saved to `r8` BEFORE `rdx` is
/// overwritten by the arg2 shuffle; `rdx`←`rsi` (src) and `ecx`←`edi` (af)
/// move last. Returns an `int` in `eax` (1 success, 0 fail, -1
/// EAFNOSUPPORT); the consumer (`inet6_pton.rs:85`) does `cmp eax,1; sete
/// al` — collapses to a 0/1 predicate without ever sign-testing `rax`, so no
/// cdqe.
fn emit_shim_inet_pton(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_inet_pton");
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + alignment(8)
    emitter.instruction("mov r8, rdx");                                         // SAVE dst (SysV arg3) before rdx is overwritten
    emitter.instruction("mov rdx, rsi");                                        // src → arg2 (rdx)
    emitter.instruction("mov ecx, edi");                                        // af → arg1 (ecx)
    emitter.instruction("call inet_pton");                                      // ws2_32 inet_pton (returns int in eax)
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return (no cdqe — cmp eax,1;sete never sign-tests)
    emitter.blank();
}

/// Emits the `__rt_sys_inet_ntop` shim: converts SysV `inet_ntop(af, src,
/// dst, size)` (edi, rsi, rdx, ecx) to MSx64 `inet_ntop` (ecx=af, rdx=src,
/// r8=dst, r9d=size). Register-shuffle hazard: SysV arg4 (size) is in `ecx`,
/// which is ALSO the MSx64 arg1 (af) target, so it is saved to `r9d` BEFORE
/// `ecx` is overwritten by the arg1 shuffle. SysV arg3 (dst) is in `rdx`,
/// which is ALSO MSx64 arg2 (src), so it is saved to `r8` BEFORE `rdx` is
/// overwritten by the arg2 shuffle. `rdx`←`rsi` (src) and `ecx`←`edi` (af,
/// 32-bit family value) move last. Returns `const char*` (buf pointer or
/// NULL) in `rax`; the consumer (`format_sockaddr.rs:432`) does `test
/// rax,rax; jz` — pointer, never sign-tested, so no cdqe.
fn emit_shim_inet_ntop(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_inet_ntop");
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + alignment(8)
    emitter.instruction("mov r9d, ecx");                                        // SAVE size (SysV arg4) before ecx is overwritten
    emitter.instruction("mov r8, rdx");                                         // SAVE dst (SysV arg3) before rdx is overwritten
    emitter.instruction("mov rdx, rsi");                                        // src → arg2 (rdx)
    emitter.instruction("mov ecx, edi");                                        // af → arg1 (ecx, 32-bit family value)
    emitter.instruction("call inet_ntop");                                      // ws2_32 inet_ntop (returns const char* buf or NULL in rax)
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return pointer (no cdqe: never sign-tested)
    emitter.blank();
}

/// Emits the `__rt_sys_gethostbyaddr` shim: converts SysV `gethostbyaddr(addr,
/// len, type)` (rdi, rsi, rdx) to MSx64 `gethostbyaddr` (rcx=addr, rdx=len,
/// r8=type). Register-shuffle hazard: SysV arg3 (type) is in `rdx`, which is
/// ALSO the MSx64 arg2 (len) target, so it is saved to `r8` BEFORE `rdx` is
/// overwritten by the arg2 shuffle; `rdx`←`rsi` (len) and `rcx`←`rdi` (addr)
/// move last. Returns `struct hostent*` (or NULL) in `rax`; the consumer
/// (`gethostbyaddr.rs:115`) does `test rax,rax; jz` — pointer, never
/// sign-tested, so no cdqe. Named `_win` to avoid colliding with the SysV
/// `emit_gethostbyaddr_linux_x86_64` runtime-helper function of a similar
/// name in `runtime::io::gethostbyaddr`.
fn emit_shim_gethostbyaddr_win(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_gethostbyaddr");
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + alignment(8)
    emitter.instruction("mov r8, rdx");                                         // SAVE type (SysV arg3) before rdx is overwritten
    emitter.instruction("mov rdx, rsi");                                        // len → arg2 (rdx)
    emitter.instruction("mov rcx, rdi");                                        // addr → arg1 (rcx)
    emitter.instruction("call gethostbyaddr");                                  // ws2_32 gethostbyaddr (returns struct hostent* or NULL in rax)
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return pointer (no cdqe: never sign-tested)
    emitter.blank();
}

/// Emits the `__rt_sys_strtoll` shim: converts SysV `strtoll(s, endptr,
/// base)` (rdi, rsi, edx) to the MSx64 msvcrt equivalent `_strtoi64` (rcx=s,
/// rdx=endptr, r8d=base) — msvcrt has no symbol literally named `strtoll`,
/// so this shim calls the differently-named `_strtoi64` import (no
/// self-recursion risk, unlike `atof`/`setlocale`/etc. which share their
/// SysV name with the msvcrt import). Register-shuffle hazard: SysV arg3
/// (base) is in `edx`, which is ALSO the MSx64 arg2 (endptr) target, so it
/// is saved to `r8d` BEFORE `edx` is overwritten by the arg2 shuffle.
/// `rdx`←`rsi` (endptr) and `rcx`←`rdi` (s) move last. Returns a 64-bit
/// `long long` in `rax` (LLONG_MAX/MIN on overflow) — full 64-bit value,
/// never a 32-bit status needing sign-extension, so no cdqe. Site:
/// `str_to_int.rs:94`.
fn emit_shim_strtoll(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_strtoll");
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + alignment(8)
    emitter.instruction("mov r8, rdx");                                         // SAVE base (SysV arg3) before rdx is overwritten
    emitter.instruction("mov rdx, rsi");                                        // endptr → arg2 (rdx)
    emitter.instruction("mov rcx, rdi");                                        // s → arg1 (rcx)
    emitter.instruction("call _strtoi64");                                      // msvcrt _strtoi64 (returns 64-bit long long in rax)
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return (no cdqe: full 64-bit value, not a status)
    emitter.blank();
}

/// Emits the `__rt_sys_atof` shim: converts SysV `atof(s)` (rdi) to MSx64
/// `atof` (rcx=s). Unlike the uniform [`emit_fp_shadow_shim`] family (`pow`,
/// `sin`, ... — all-FP-register arguments, identical in SysV and MSx64), `atof`
/// takes a POINTER argument, which SysV passes in `rdi` but MSx64 expects in
/// `rcx` — so this shim needs its own `rdi`→`rcx` move before the call
/// (`emit_fp_shadow_shim` cannot be reused here). The `double` result stays in
/// `xmm0` in both ABIs — xmm0 is NEVER touched by this shim. Sites:
/// `mixed_cast_float.rs:110`, `json_decode_mixed/x86_64.rs:455`
/// (`mixed_cast_float.rs:57` is the AArch64 branch of the same function —
/// left untouched).
fn emit_shim_atof(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_atof");
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + alignment(8)
    emitter.instruction("mov rcx, rdi");                                        // s → arg1 (rcx); xmm0 untouched
    emitter.instruction("call atof");                                           // msvcrt atof (returns double in xmm0)
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return (double stays in xmm0 — no cdqe, not an int result)
    emitter.blank();
}

/// Emits the `__rt_sys_setlocale` shim: converts SysV `setlocale(category,
/// locale)` (edi, rsi) to MSx64 `setlocale` (ecx=category, rdx=locale).
/// `rdx`←`rsi` (locale) and `ecx`←`edi` (category) never collide with an
/// earlier MSx64 write, so the shuffle order does not matter. Returns
/// `char*` (or NULL) in `rax`; the consumer (`regex_locale.rs:50,55`) does
/// `test rax,rax; jnz` — pointer, never sign-tested, so no cdqe.
fn emit_shim_setlocale(emitter: &mut Emitter) {
    emitter.label_global("__rt_sys_setlocale");
    emitter.instruction("sub rsp, 40");                                         // shadow(32) + alignment(8)
    emitter.instruction("mov rdx, rsi");                                        // locale → arg2 (rdx)
    emitter.instruction("mov ecx, edi");                                        // category → arg1 (ecx)
    emitter.instruction("call setlocale");                                      // msvcrt setlocale (returns char* or NULL in rax)
    emitter.instruction("add rsp, 40");                                         // restore stack
    emitter.instruction("ret");                                                 // return pointer (no cdqe: never sign-tested)
    emitter.blank();
}

/// Emits the `__rt_sys_libc_chown`/`__rt_sys_libc_lchown` shims.
///
/// Windows has no POSIX ownership change. php-src omits `lchown` and `lchgrp`
/// there, while elephc still needs internal failure shims so its shared runtime
/// assembly contains no nonexistent CRT imports. `chown` and `chgrp` remain
/// PHP-visible and lower through the same `-1` failure path. These shims are
/// separate from the pre-existing `__rt_sys_chown`/`__rt_sys_lchown`
/// (`emit_shim_c_symbol_delegates`), which serve the unrelated transformed
/// Linux-syscall-number 92/94 path.
pub(super) fn emit_shim_libc_chown_unsupported(emitter: &mut Emitter) {
    for label in ["__rt_sys_libc_chown", "__rt_sys_libc_lchown"] {
        emitter.label_global(label);
        emitter.instruction("mov rax, -1");                                     // PHP_WIN32 rejects ownership changes on plain filesystem paths
        emitter.instruction("ret");                                             // return failure without touching the path or errno
        emitter.blank();
    }
}
