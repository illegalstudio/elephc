//! Purpose:
//! Emits `__rt_stash_connect_host`, which records the transport host string of a
//! freshly connected `stream_socket_client` socket into the per-fd
//! `_stream_connect_host` table so `stream_socket_enable_crypto` can default the
//! TLS SNI / peer-name to it when no `ssl.peer_name` context option is set.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via
//!   `crate::codegen::runtime::io`.
//! - The `stream_socket_client` builtin emitter, after a successful connect.
//!
//! Key details:
//! - The address is `[scheme://]host:port` (PHP's `tcp://example.com:443`); this
//!   strips a leading `scheme://`, then takes the host up to the LAST `:` (the
//!   port separator). A `[…]` IPv6 literal keeps its bracket bytes verbatim — the
//!   bracket payload is what `__rt_inet_addr_parse` resolved, and rustls accepts
//!   a bracketless form via the peer-name path; an IP SNI is simply ignored by a
//!   server, which is the safe PHP-compatible outcome.
//! - The host bytes are persisted with `__rt_str_persist` (heap-owned, never
//!   freed — one slot per fd, overwritten on the next connect to the same fd) and
//!   stored as `(ptr, len)` at `_stream_connect_host[fd * 16]`.
//! - The fd passes through unchanged (in on arg0, out in the result register) so
//!   the builtin can wrap the call transparently.

use crate::codegen::{abi, emit::Emitter, platform::Arch};

/// Emits `__rt_stash_connect_host(fd, addr_ptr, addr_len) -> fd`.
///
/// Inputs (AArch64): x0 = fd, x1 = address pointer, x2 = address length.
/// (x86_64): rdi = fd, rsi = address pointer, rdx = address length. Output:
/// x0 / rax = fd (unchanged). When `fd < 0` or `fd >= 256` the helper is a no-op
/// passthrough so failed connects and out-of-range descriptors never touch the
/// table.
pub fn emit_stash_connect_host(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stash_connect_host_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: stash_connect_host ---");
    emitter.label_global("__rt_stash_connect_host");

    // Frame: 48 bytes. [sp,#0] fd, [sp,#8] host ptr, [sp,#16] host len,
    //   [sp,#32..48] saved x29/x30.
    emitter.instruction("sub sp, sp, #48");                                     // helper frame for the connect-host stash
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the fd for the table index and return value

    // -- range-check the fd: only descriptors 0..256 get a table slot --
    emitter.instruction("cmp x0, #0");                                          // is the fd negative (failed connect)?
    emitter.instruction("b.lt __rt_sch_done");                                  // negative fd → passthrough, no stash
    emitter.instruction("cmp x0, #256");                                        // is the fd within the table bound?
    emitter.instruction("b.ge __rt_sch_done");                                  // out-of-range fd → passthrough, no stash

    // -- strip a leading "scheme://" by scanning for "://" --
    emitter.instruction("mov x9, #0");                                          // host start offset within the address
    emitter.instruction("mov x10, #0");                                         // scheme scan index
    emitter.label("__rt_sch_scheme");
    emitter.instruction("add x11, x10, #3");                                    // need three bytes for "://"
    emitter.instruction("cmp x11, x2");                                         // do enough bytes remain?
    emitter.instruction("b.gt __rt_sch_have_start");                            // no "://" found → host starts at offset 0
    emitter.instruction("ldrb w12, [x1, x10]");                                 // load candidate ':' byte
    emitter.instruction("cmp w12, #58");                                        // is it ':'?
    emitter.instruction("b.ne __rt_sch_scheme_next");                           // not the scheme marker
    emitter.instruction("add x13, x10, #1");                                    // index of the first '/'
    emitter.instruction("ldrb w12, [x1, x13]");                                 // load candidate first '/'
    emitter.instruction("cmp w12, #47");                                        // is it '/'?
    emitter.instruction("b.ne __rt_sch_scheme_next");                           // not the scheme marker
    emitter.instruction("add x13, x10, #2");                                    // index of the second '/'
    emitter.instruction("ldrb w12, [x1, x13]");                                 // load candidate second '/'
    emitter.instruction("cmp w12, #47");                                        // is it '/'?
    emitter.instruction("b.ne __rt_sch_scheme_next");                           // not the scheme marker
    emitter.instruction("add x9, x10, #3");                                     // host begins just past "://"
    emitter.instruction("b __rt_sch_have_start");                               // scheme stripped — find the port separator
    emitter.label("__rt_sch_scheme_next");
    emitter.instruction("add x10, x10, #1");                                    // advance the scheme scan index
    emitter.instruction("b __rt_sch_scheme");                                   // keep scanning for "://"
    emitter.label("__rt_sch_have_start");

    // -- host pointer = addr + start; remaining length = addr_len - start --
    emitter.instruction("add x1, x1, x9");                                      // x1 = host pointer (past any scheme)
    emitter.instruction("sub x2, x2, x9");                                      // x2 = bytes from host start to end of address

    // -- find the LAST ':' in the remaining bytes — the port separator. A '['
    //    IPv6 literal has no bare ':' after its ']' unless a port follows, so a
    //    last-colon scan correctly trims an optional :port and keeps the
    //    bracketed body. --
    emitter.instruction("mov x10, #0");                                         // scan index
    emitter.instruction("mov x11, x2");                                         // host length defaults to the whole remainder
    emitter.label("__rt_sch_port");
    emitter.instruction("cmp x10, x2");                                         // scanned every byte?
    emitter.instruction("b.ge __rt_sch_persist");                               // no ':' → host length stays the full remainder
    emitter.instruction("ldrb w12, [x1, x10]");                                 // load the candidate byte
    emitter.instruction("cmp w12, #58");                                        // is it ':'?
    emitter.instruction("b.ne __rt_sch_port_next");                             // not a colon — keep scanning
    emitter.instruction("mov x11, x10");                                        // remember the colon offset as the host length
    emitter.label("__rt_sch_port_next");
    emitter.instruction("add x10, x10, #1");                                    // advance the colon scan index
    emitter.instruction("b __rt_sch_port");                                     // keep scanning for the last ':'
    emitter.label("__rt_sch_persist");
    emitter.instruction("mov x2, x11");                                         // x2 = host length (trimmed of any :port)

    // -- persist the host bytes (heap-owned) and store (ptr, len) in the table --
    abi::emit_call_label(emitter, "__rt_str_persist");                          // x1 = persisted host ptr, x2 = len (unchanged)
    abi::emit_symbol_address(emitter, "x10", "_stream_connect_host");
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the fd for the table index
    emitter.instruction("add x10, x10, x9, lsl #4");                            // slot = table + fd * 16 (ptr, len pair)
    emitter.instruction("str x1, [x10, #0]");                                   // _stream_connect_host[fd].ptr = persisted host
    emitter.instruction("str x2, [x10, #8]");                                   // _stream_connect_host[fd].len = host length

    emitter.label("__rt_sch_done");
    emitter.instruction("ldr x0, [sp, #0]");                                    // return the fd unchanged
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return to the stream_socket_client emitter
}

/// x86_64 implementation of `__rt_stash_connect_host`.
fn emit_stash_connect_host_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stash_connect_host ---");
    emitter.label_global("__rt_stash_connect_host");

    // Frame: [rbp-8] fd, [rbp-16] host ptr (scratch). push rbp + sub 16 stays
    //   16-aligned for the inner __rt_str_persist call.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 16");                                         // spill slot for the fd across str_persist
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the fd for the table index and return value

    // -- range-check the fd: only descriptors 0..256 get a table slot --
    emitter.instruction("cmp rdi, 0");                                          // is the fd negative (failed connect)?
    emitter.instruction("jl __rt_sch_done_x86");                                // negative fd → passthrough, no stash
    emitter.instruction("cmp rdi, 256");                                        // is the fd within the table bound?
    emitter.instruction("jge __rt_sch_done_x86");                               // out-of-range fd → passthrough, no stash

    // -- strip a leading "scheme://" by scanning for "://" (rsi=ptr, rdx=len) --
    emitter.instruction("xor r9, r9");                                          // host start offset within the address
    emitter.instruction("xor r10, r10");                                        // scheme scan index
    emitter.label("__rt_sch_scheme_x86");
    emitter.instruction("lea r11, [r10 + 3]");                                  // need three bytes for "://"
    emitter.instruction("cmp r11, rdx");                                        // do enough bytes remain?
    emitter.instruction("jg __rt_sch_have_start_x86");                          // no "://" found → host starts at offset 0
    emitter.instruction("movzx ecx, BYTE PTR [rsi + r10]");                     // load candidate ':' byte
    emitter.instruction("cmp ecx, 58");                                         // is it ':'?
    emitter.instruction("jne __rt_sch_scheme_next_x86");                        // not the scheme marker
    emitter.instruction("movzx ecx, BYTE PTR [rsi + r10 + 1]");                 // load candidate first '/'
    emitter.instruction("cmp ecx, 47");                                         // is it '/'?
    emitter.instruction("jne __rt_sch_scheme_next_x86");                        // not the scheme marker
    emitter.instruction("movzx ecx, BYTE PTR [rsi + r10 + 2]");                 // load candidate second '/'
    emitter.instruction("cmp ecx, 47");                                         // is it '/'?
    emitter.instruction("jne __rt_sch_scheme_next_x86");                        // not the scheme marker
    emitter.instruction("lea r9, [r10 + 3]");                                   // host begins just past "://"
    emitter.instruction("jmp __rt_sch_have_start_x86");                         // scheme stripped — find the port separator
    emitter.label("__rt_sch_scheme_next_x86");
    emitter.instruction("inc r10");                                             // advance the scheme scan index
    emitter.instruction("jmp __rt_sch_scheme_x86");                             // keep scanning for "://"
    emitter.label("__rt_sch_have_start_x86");

    // -- host pointer = addr + start; remaining length = addr_len - start --
    emitter.instruction("add rsi, r9");                                         // rsi = host pointer (past any scheme)
    emitter.instruction("sub rdx, r9");                                         // rdx = bytes from host start to end of address

    // -- find the LAST ':' in the remaining bytes — the port separator --
    emitter.instruction("xor r10, r10");                                        // scan index
    emitter.instruction("mov r11, rdx");                                        // host length defaults to the whole remainder
    emitter.label("__rt_sch_port_x86");
    emitter.instruction("cmp r10, rdx");                                        // scanned every byte?
    emitter.instruction("jge __rt_sch_persist_x86");                            // no ':' → host length stays the full remainder
    emitter.instruction("movzx ecx, BYTE PTR [rsi + r10]");                     // load the candidate byte
    emitter.instruction("cmp ecx, 58");                                         // is it ':'?
    emitter.instruction("jne __rt_sch_port_next_x86");                          // not a colon — keep scanning
    emitter.instruction("mov r11, r10");                                        // remember the colon offset as the host length
    emitter.label("__rt_sch_port_next_x86");
    emitter.instruction("inc r10");                                             // advance the colon scan index
    emitter.instruction("jmp __rt_sch_port_x86");                               // keep scanning for the last ':'
    emitter.label("__rt_sch_persist_x86");

    // -- persist the host bytes; __rt_str_persist takes rax=ptr, rdx=len --
    emitter.instruction("mov rax, rsi");                                        // str_persist source pointer = host pointer
    emitter.instruction("mov rdx, r11");                                        // str_persist length = trimmed host length
    abi::emit_call_label(emitter, "__rt_str_persist");                          // rax = persisted host ptr, rdx = len (unchanged)
    emitter.instruction("lea r10, [rip + _stream_connect_host]");               // base of the per-fd connect-host table
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload the fd for the table index
    emitter.instruction("shl r9, 4");                                           // fd * 16 (ptr, len pair stride)
    emitter.instruction("add r10, r9");                                         // slot address = table + fd * 16
    emitter.instruction("mov QWORD PTR [r10 + 0], rax");                        // _stream_connect_host[fd].ptr = persisted host
    emitter.instruction("mov QWORD PTR [r10 + 8], rdx");                        // _stream_connect_host[fd].len = host length

    emitter.label("__rt_sch_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the fd unchanged
    emitter.instruction("add rsp, 16");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the stream_socket_client emitter
}
