//! Purpose:
//! Emits `__rt_stash_connect_host`, which records the transport host string of a
//! freshly connected `stream_socket_client` socket into the per-fd
//! `_stream_connect_host` table so `stream_socket_enable_crypto` can default the
//! TLS SNI / peer-name to it when no `ssl.peer_name` context option is set.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::io`.
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
//!   freed — one slot per live descriptor, overwritten on reconnect) and stored
//!   in a bounded associative full-width-descriptor map.
//! - The fd passes through unchanged (in on arg0, out in the result register) so
//!   the builtin can wrap the call transparently.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits `__rt_stash_connect_host(fd, addr_ptr, addr_len) -> fd`.
///
/// Inputs (AArch64): x0 = fd, x1 = address pointer, x2 = address length.
/// (x86_64): rdi = fd, rsi = address pointer, rdx = address length. Output:
/// x0 / rax = fd (unchanged). Negative descriptors are passthrough failures;
/// full-width Winsock handles are accepted and matched as keys rather than indexes.
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

    // -- reject only failed connects; valid descriptors are full-width keys --
    emitter.instruction("cmp x0, #0");                                          // is the fd negative (failed connect)?
    emitter.instruction("b.lt __rt_sch_done");                                  // negative fd → passthrough, no stash

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

    // -- persist the host bytes and associate them with the full-width fd --
    abi::emit_call_label(emitter, "__rt_str_persist");                          // x1 = persisted host ptr, x2 = len (unchanged)
    emitter.instruction("str x1, [sp, #8]");                                    // preserve the persisted host pointer during the slot scan
    emitter.instruction("str x2, [sp, #16]");                                   // preserve the persisted host length during the slot scan
    abi::emit_symbol_address(emitter, "x13", "_stream_connect_host_fds");
    abi::emit_symbol_address(emitter, "x10", "_stream_connect_host");
    emitter.instruction("ldr x12, [sp, #0]");                                   // reload the full-width descriptor key
    emitter.instruction("mov x9, #0");                                          // begin with an existing-key scan
    emitter.label("__rt_sch_slot_match");
    emitter.instruction("cmp x9, #256");                                        // has every existing host slot been inspected?
    emitter.instruction("b.ge __rt_sch_slot_empty_start");                      // no match requires a free-slot scan
    emitter.instruction("add x11, x10, x9, lsl #4");                            // address this candidate host pointer/length pair
    emitter.instruction("ldr x14, [x11, #8]");                                  // load the candidate host length marker
    emitter.instruction("cbz x14, __rt_sch_slot_match_next");                   // unused slots cannot match a descriptor
    emitter.instruction("ldr x14, [x13, x9, lsl #3]");                          // load this slot's full-width descriptor key
    emitter.instruction("cmp x14, x12");                                        // is this the descriptor being updated?
    emitter.instruction("b.eq __rt_sch_slot_store");                            // replace the existing host value in place
    emitter.label("__rt_sch_slot_match_next");
    emitter.instruction("add x9, x9, #1");                                      // advance through occupied slots
    emitter.instruction("b __rt_sch_slot_match");                               // continue the existing-key scan
    emitter.label("__rt_sch_slot_empty_start");
    emitter.instruction("mov x9, #0");                                          // restart at slot zero to find free capacity
    emitter.label("__rt_sch_slot_empty");
    emitter.instruction("cmp x9, #256");                                        // has every bounded slot proved occupied?
    emitter.instruction("b.ge __rt_sch_done");                                  // full host metadata table leaves the descriptor unstashed
    emitter.instruction("add x11, x10, x9, lsl #4");                            // address this candidate host pointer/length pair
    emitter.instruction("ldr x14, [x11, #8]");                                  // inspect the candidate host length marker
    emitter.instruction("cbz x14, __rt_sch_slot_store");                        // zero length identifies an available slot
    emitter.instruction("add x9, x9, #1");                                      // advance to the next candidate free slot
    emitter.instruction("b __rt_sch_slot_empty");                               // continue the bounded free-slot scan
    emitter.label("__rt_sch_slot_store");
    emitter.instruction("str x12, [x13, x9, lsl #3]");                          // store the full-width descriptor key
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the persisted host pointer
    emitter.instruction("ldr x2, [sp, #16]");                                   // reload the persisted host length
    emitter.instruction("str x1, [x11, #0]");                                   // store the persisted host pointer
    emitter.instruction("str x2, [x11, #8]");                                   // publish the non-zero host length last

    emitter.label("__rt_sch_done");
    emitter.instruction("ldr x0, [sp, #0]");                                    // return the fd unchanged
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return to the stream_socket_client emitter

    emit_get_stashed_connect_host_aarch64(emitter);
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

    // -- reject only failed connects; valid descriptors are full-width keys --
    emitter.instruction("cmp rdi, 0");                                          // is the fd negative (failed connect)?
    emitter.instruction("jl __rt_sch_done_x86");                                // negative fd → passthrough, no stash

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
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the persisted host pointer during the slot scan
    abi::emit_symbol_address(emitter, "r8", "_stream_connect_host_fds");
    abi::emit_symbol_address(emitter, "r9", "_stream_connect_host");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the full-width descriptor key
    emitter.instruction("xor r10d, r10d");                                      // begin with an existing-key scan
    emitter.label("__rt_sch_slot_match_x86");
    emitter.instruction("cmp r10, 256");                                        // has every existing host slot been inspected?
    emitter.instruction("jge __rt_sch_slot_empty_start_x86");                   // no match requires a free-slot scan
    emitter.instruction("mov r11, r10");                                        // copy the slot index for the 16-byte host-pair stride
    emitter.instruction("shl r11, 4");                                          // convert the slot index to a host-pair byte offset
    emitter.instruction("cmp QWORD PTR [r9 + r11 + 8], 0");                     // is this host slot unused?
    emitter.instruction("je __rt_sch_slot_match_next_x86");                     // unused slots cannot match a descriptor
    emitter.instruction("cmp QWORD PTR [r8 + r10 * 8], rdi");                   // is this the descriptor being updated?
    emitter.instruction("je __rt_sch_slot_store_x86");                          // replace the existing host value in place
    emitter.label("__rt_sch_slot_match_next_x86");
    emitter.instruction("add r10, 1");                                          // advance through occupied slots
    emitter.instruction("jmp __rt_sch_slot_match_x86");                         // continue the existing-key scan
    emitter.label("__rt_sch_slot_empty_start_x86");
    emitter.instruction("xor r10d, r10d");                                      // restart at slot zero to find free capacity
    emitter.label("__rt_sch_slot_empty_x86");
    emitter.instruction("cmp r10, 256");                                        // has every bounded slot proved occupied?
    emitter.instruction("jge __rt_sch_done_x86");                               // full host metadata table leaves the descriptor unstashed
    emitter.instruction("mov r11, r10");                                        // copy the slot index for the 16-byte host-pair stride
    emitter.instruction("shl r11, 4");                                          // convert the slot index to a host-pair byte offset
    emitter.instruction("cmp QWORD PTR [r9 + r11 + 8], 0");                     // inspect the candidate host length marker
    emitter.instruction("je __rt_sch_slot_store_x86");                          // zero length identifies an available slot
    emitter.instruction("add r10, 1");                                          // advance to the next candidate free slot
    emitter.instruction("jmp __rt_sch_slot_empty_x86");                         // continue the bounded free-slot scan
    emitter.label("__rt_sch_slot_store_x86");
    emitter.instruction("mov QWORD PTR [r8 + r10 * 8], rdi");                   // store the full-width descriptor key
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the persisted host pointer
    emitter.instruction("mov QWORD PTR [r9 + r11 + 0], rax");                   // store the persisted host pointer
    emitter.instruction("mov QWORD PTR [r9 + r11 + 8], rdx");                   // publish the non-zero host length last

    emitter.label("__rt_sch_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // return the fd unchanged
    emitter.instruction("add rsp, 16");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the stream_socket_client emitter

    emit_get_stashed_connect_host_x86_64(emitter);
}

/// Emits the AArch64 full-width descriptor lookup for persisted SNI host metadata.
fn emit_get_stashed_connect_host_aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_get_stashed_connect_host");
    abi::emit_symbol_address(emitter, "x9", "_stream_connect_host_fds");
    abi::emit_symbol_address(emitter, "x10", "_stream_connect_host");
    emitter.instruction("mov x11, #0");                                         // begin at the first bounded host slot
    emitter.label("__rt_gsch_loop");
    emitter.instruction("cmp x11, #256");                                       // have all persisted host slots been searched?
    emitter.instruction("b.ge __rt_gsch_miss");                                 // no matching descriptor means no default SNI host
    emitter.instruction("add x12, x10, x11, lsl #4");                           // address this candidate host pointer/length pair
    emitter.instruction("ldr x2, [x12, #8]");                                   // load the candidate host length marker
    emitter.instruction("cbz x2, __rt_gsch_next");                              // unused slots cannot match a descriptor
    emitter.instruction("ldr x13, [x9, x11, lsl #3]");                          // load this slot's full-width descriptor key
    emitter.instruction("cmp x13, x0");                                         // does the descriptor match the requested socket?
    emitter.instruction("b.eq __rt_gsch_found");                                // return this persisted host pair
    emitter.label("__rt_gsch_next");
    emitter.instruction("add x11, x11, #1");                                    // advance to the next bounded host slot
    emitter.instruction("b __rt_gsch_loop");                                    // continue the associative lookup
    emitter.label("__rt_gsch_found");
    emitter.instruction("ldr x1, [x12, #0]");                                   // return the persisted host pointer
    emitter.instruction("mov x0, #1");                                          // report a successful metadata lookup
    emitter.instruction("ret");                                                 // return the host pointer/length pair
    emitter.label("__rt_gsch_miss");
    emitter.instruction("mov x0, #0");                                          // report that no persisted host is available
    emitter.instruction("mov x1, #0");                                          // clear the absent host pointer
    emitter.instruction("mov x2, #0");                                          // clear the absent host length
    emitter.instruction("ret");                                                 // return the lookup miss
}

/// Emits the x86_64 full-width descriptor lookup for persisted SNI host metadata.
fn emit_get_stashed_connect_host_x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_get_stashed_connect_host");
    abi::emit_symbol_address(emitter, "r8", "_stream_connect_host_fds");
    abi::emit_symbol_address(emitter, "r9", "_stream_connect_host");
    emitter.instruction("xor r10d, r10d");                                      // begin at the first bounded host slot
    emitter.label("__rt_gsch_loop_x86");
    emitter.instruction("cmp r10, 256");                                        // have all persisted host slots been searched?
    emitter.instruction("jge __rt_gsch_miss_x86");                              // no matching descriptor means no default SNI host
    emitter.instruction("mov r11, r10");                                        // copy the slot index for the 16-byte host-pair stride
    emitter.instruction("shl r11, 4");                                          // convert the slot index to a host-pair byte offset
    emitter.instruction("mov rdx, QWORD PTR [r9 + r11 + 8]");                   // load the candidate host length marker
    emitter.instruction("test rdx, rdx");                                       // is this host slot live?
    emitter.instruction("jz __rt_gsch_next_x86");                               // unused slots cannot match a descriptor
    emitter.instruction("cmp QWORD PTR [r8 + r10 * 8], rdi");                   // does the descriptor match the requested socket?
    emitter.instruction("je __rt_gsch_found_x86");                              // return this persisted host pair
    emitter.label("__rt_gsch_next_x86");
    emitter.instruction("add r10, 1");                                          // advance to the next bounded host slot
    emitter.instruction("jmp __rt_gsch_loop_x86");                              // continue the associative lookup
    emitter.label("__rt_gsch_found_x86");
    emitter.instruction("mov rax, QWORD PTR [r9 + r11 + 0]");                   // return the persisted host pointer
    emitter.instruction("ret");                                                 // return the host pointer/length pair
    emitter.label("__rt_gsch_miss_x86");
    emitter.instruction("xor eax, eax");                                        // null pointer reports absent persisted host metadata
    emitter.instruction("xor edx, edx");                                        // clear the absent host length
    emitter.instruction("ret");                                                 // return the lookup miss
}

#[cfg(test)]
mod tests {
    use crate::codegen_support::platform::{Platform, Target};

    use super::*;

    /// Verifies Windows SNI metadata accepts full-width socket keys and scans
    /// bounded slots instead of rejecting or directly indexing handles above 255.
    #[test]
    fn windows_connect_host_map_accepts_full_width_socket_keys() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_stash_connect_host(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("cmp QWORD PTR [r8 + r10 * 8], rdi"));
        assert!(asm.contains("cmp r10, 256"));
        assert!(!asm.contains("cmp rdi, 256"));
        assert!(!asm.contains("shl rdi, 4"));
    }
}
