//! Purpose:
//! Emits the bounded raw-descriptor-to-TLS-session map used by stream I/O.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` through the I/O runtime module.
//!
//! Key details:
//! - Raw Winsock `SOCKET` values are 64-bit handles and must never index a fixed array directly.
//! - The table holds 256 `(fd, session)` pairs; session zero marks an empty slot.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::abi;

/// Emits lookup, insertion, and removal helpers for the bounded TLS session table.
pub(crate) fn emit_tls_session_table(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Emits the AArch64 TLS session-table helpers.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: bounded TLS session table ---");
    emitter.label_global("__rt_tls_session_get");
    abi::emit_symbol_address(emitter, "x9", "_tls_session_fds");
    abi::emit_symbol_address(emitter, "x10", "_tls_sessions");
    emitter.instruction("mov x11, #0");                                         // begin at the first bounded table slot
    emitter.label("__rt_tls_get_loop");
    emitter.instruction("cmp x11, #256");                                       // have all TLS session slots been searched?
    emitter.instruction("b.ge __rt_tls_get_miss");                              // a full scan without a matching descriptor is a miss
    emitter.instruction("ldr x12, [x10, x11, lsl #3]");                         // load the candidate session handle
    emitter.instruction("cbz x12, __rt_tls_get_next");                          // empty slots cannot match a live descriptor
    emitter.instruction("ldr x13, [x9, x11, lsl #3]");                          // load the raw descriptor paired with this session
    emitter.instruction("cmp x13, x0");                                         // does this slot belong to the requested descriptor?
    emitter.instruction("b.eq __rt_tls_get_found");                             // return the matching live TLS session
    emitter.label("__rt_tls_get_next");
    emitter.instruction("add x11, x11, #1");                                    // advance to the next bounded table slot
    emitter.instruction("b __rt_tls_get_loop");                                 // continue the linear lookup
    emitter.label("__rt_tls_get_found");
    emitter.instruction("mov x0, x12");                                         // return the matched TLS session handle
    emitter.instruction("ret");                                                 // return to the stream I/O caller
    emitter.label("__rt_tls_get_miss");
    emitter.instruction("mov x0, #0");                                          // zero means the descriptor is plain, not TLS-backed
    emitter.instruction("ret");                                                 // return the lookup miss

    emitter.label_global("__rt_tls_session_set");
    abi::emit_symbol_address(emitter, "x9", "_tls_session_fds");
    abi::emit_symbol_address(emitter, "x10", "_tls_sessions");
    emitter.instruction("mov x11, #0");                                         // begin by searching for an existing descriptor slot
    emitter.label("__rt_tls_set_match_loop");
    emitter.instruction("cmp x11, #256");                                       // has the existing-slot scan completed?
    emitter.instruction("b.ge __rt_tls_set_empty_start");                       // no match requires an empty-slot scan
    emitter.instruction("ldr x12, [x10, x11, lsl #3]");                         // load the candidate session handle
    emitter.instruction("cbz x12, __rt_tls_set_match_next");                    // ignore unused slots during the match scan
    emitter.instruction("ldr x13, [x9, x11, lsl #3]");                          // load the descriptor paired with the live session
    emitter.instruction("cmp x13, x0");                                         // is this the descriptor being installed?
    emitter.instruction("b.eq __rt_tls_set_store");                             // replace the existing session in place
    emitter.label("__rt_tls_set_match_next");
    emitter.instruction("add x11, x11, #1");                                    // advance through live slots
    emitter.instruction("b __rt_tls_set_match_loop");                           // continue searching for a descriptor match
    emitter.label("__rt_tls_set_empty_start");
    emitter.instruction("mov x11, #0");                                         // restart at slot zero to find free capacity
    emitter.label("__rt_tls_set_empty_loop");
    emitter.instruction("cmp x11, #256");                                       // has every bounded slot proved occupied?
    emitter.instruction("b.ge __rt_tls_set_full");                              // fail cleanly instead of indexing beyond the table
    emitter.instruction("ldr x12, [x10, x11, lsl #3]");                         // inspect this slot's session marker
    emitter.instruction("cbz x12, __rt_tls_set_store");                         // session zero identifies an available slot
    emitter.instruction("add x11, x11, #1");                                    // advance to the next candidate free slot
    emitter.instruction("b __rt_tls_set_empty_loop");                           // continue the bounded free-slot scan
    emitter.label("__rt_tls_set_store");
    emitter.instruction("str x0, [x9, x11, lsl #3]");                           // store the full-width raw descriptor key
    emitter.instruction("str x1, [x10, x11, lsl #3]");                          // store its TLS session handle
    emitter.instruction("mov x0, #1");                                          // report successful insertion
    emitter.instruction("ret");                                                 // return to the attach path
    emitter.label("__rt_tls_set_full");
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // preserve the caller link while closing the untracked session
    emitter.instruction("mov x0, x1");                                          // pass the untracked TLS session to elephc_tls_close
    abi::emit_symbol_address(emitter, "x9", "_elephc_tls_close_fn");
    emitter.instruction("ldr x9, [x9]");                                        // load the published TLS close entry pointer
    emitter.instruction("cbz x9, __rt_tls_set_full_closed");                    // tolerate a missing bridge during defensive cleanup
    emitter.emit_published_bridge_call("x9");                                  // close the unrepresentable session through the published TLS entry
    emitter.label("__rt_tls_set_full_closed");
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore the caller frame and return address
    emitter.instruction("mov x0, #0");                                          // report bounded table exhaustion
    emitter.instruction("ret");                                                 // let the caller close the untracked TLS session

    emitter.label_global("__rt_tls_session_clear");
    abi::emit_symbol_address(emitter, "x9", "_tls_session_fds");
    abi::emit_symbol_address(emitter, "x10", "_tls_sessions");
    emitter.instruction("mov x11, #0");                                         // begin at the first bounded table slot
    emitter.label("__rt_tls_clear_loop");
    emitter.instruction("cmp x11, #256");                                       // have all slots been searched?
    emitter.instruction("b.ge __rt_tls_clear_miss");                            // no matching descriptor means no session to clear
    emitter.instruction("ldr x12, [x10, x11, lsl #3]");                         // load the candidate session handle
    emitter.instruction("cbz x12, __rt_tls_clear_next");                        // skip unused slots
    emitter.instruction("ldr x13, [x9, x11, lsl #3]");                          // load the candidate raw descriptor
    emitter.instruction("cmp x13, x0");                                         // does this slot match the requested descriptor?
    emitter.instruction("b.eq __rt_tls_clear_found");                           // clear and return the matching session
    emitter.label("__rt_tls_clear_next");
    emitter.instruction("add x11, x11, #1");                                    // advance to the next bounded slot
    emitter.instruction("b __rt_tls_clear_loop");                               // continue the lookup
    emitter.label("__rt_tls_clear_found");
    emitter.instruction("str xzr, [x10, x11, lsl #3]");                         // mark the session slot empty first
    emitter.instruction("str xzr, [x9, x11, lsl #3]");                          // clear the stale descriptor key
    emitter.instruction("mov x0, x12");                                         // return the removed session for close_notify
    emitter.instruction("ret");                                                 // return the removed session
    emitter.label("__rt_tls_clear_miss");
    emitter.instruction("mov x0, #0");                                          // no live session was attached to this descriptor
    emitter.instruction("ret");                                                 // return the clear miss
}

/// Emits the x86_64 TLS session-table helpers.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: bounded TLS session table ---");
    emitter.label_global("__rt_tls_session_get");
    abi::emit_symbol_address(emitter, "r8", "_tls_session_fds");
    abi::emit_symbol_address(emitter, "r9", "_tls_sessions");
    emitter.instruction("xor r10d, r10d");                                      // begin at the first bounded table slot
    emitter.label("__rt_tls_get_loop_x86_64");
    emitter.instruction("cmp r10, 256");                                        // have all TLS session slots been searched?
    emitter.instruction("jge __rt_tls_get_miss_x86_64");                        // a full scan without a matching descriptor is a miss
    emitter.instruction("mov r11, QWORD PTR [r9 + r10 * 8]");                   // load the candidate session handle
    emitter.instruction("test r11, r11");                                       // is this slot live?
    emitter.instruction("jz __rt_tls_get_next_x86_64");                         // empty slots cannot match a live descriptor
    emitter.instruction("cmp QWORD PTR [r8 + r10 * 8], rdi");                   // does this slot belong to the requested descriptor?
    emitter.instruction("je __rt_tls_get_found_x86_64");                        // return the matching live TLS session
    emitter.label("__rt_tls_get_next_x86_64");
    emitter.instruction("add r10, 1");                                          // advance to the next bounded table slot
    emitter.instruction("jmp __rt_tls_get_loop_x86_64");                        // continue the linear lookup
    emitter.label("__rt_tls_get_found_x86_64");
    emitter.instruction("mov rax, r11");                                        // return the matched TLS session handle
    emitter.instruction("ret");                                                 // return to the stream I/O caller
    emitter.label("__rt_tls_get_miss_x86_64");
    emitter.instruction("xor eax, eax");                                        // zero means the descriptor is plain, not TLS-backed
    emitter.instruction("ret");                                                 // return the lookup miss

    emitter.label_global("__rt_tls_session_set");
    abi::emit_symbol_address(emitter, "r8", "_tls_session_fds");
    abi::emit_symbol_address(emitter, "r9", "_tls_sessions");
    emitter.instruction("xor r10d, r10d");                                      // begin by searching for an existing descriptor slot
    emitter.label("__rt_tls_set_match_loop_x86_64");
    emitter.instruction("cmp r10, 256");                                        // has the existing-slot scan completed?
    emitter.instruction("jge __rt_tls_set_empty_start_x86_64");                 // no match requires an empty-slot scan
    emitter.instruction("cmp QWORD PTR [r9 + r10 * 8], 0");                     // is this slot unused?
    emitter.instruction("je __rt_tls_set_match_next_x86_64");                   // ignore unused slots during the match scan
    emitter.instruction("cmp QWORD PTR [r8 + r10 * 8], rdi");                   // is this the descriptor being installed?
    emitter.instruction("je __rt_tls_set_store_x86_64");                        // replace the existing session in place
    emitter.label("__rt_tls_set_match_next_x86_64");
    emitter.instruction("add r10, 1");                                          // advance through live slots
    emitter.instruction("jmp __rt_tls_set_match_loop_x86_64");                  // continue searching for a descriptor match
    emitter.label("__rt_tls_set_empty_start_x86_64");
    emitter.instruction("xor r10d, r10d");                                      // restart at slot zero to find free capacity
    emitter.label("__rt_tls_set_empty_loop_x86_64");
    emitter.instruction("cmp r10, 256");                                        // has every bounded slot proved occupied?
    emitter.instruction("jge __rt_tls_set_full_x86_64");                        // fail cleanly instead of indexing beyond the table
    emitter.instruction("cmp QWORD PTR [r9 + r10 * 8], 0");                     // inspect this slot's session marker
    emitter.instruction("je __rt_tls_set_store_x86_64");                        // session zero identifies an available slot
    emitter.instruction("add r10, 1");                                          // advance to the next candidate free slot
    emitter.instruction("jmp __rt_tls_set_empty_loop_x86_64");                  // continue the bounded free-slot scan
    emitter.label("__rt_tls_set_store_x86_64");
    emitter.instruction("mov QWORD PTR [r8 + r10 * 8], rdi");                   // store the full-width raw descriptor key
    emitter.instruction("mov QWORD PTR [r9 + r10 * 8], rsi");                   // store its TLS session handle
    emitter.instruction("mov eax, 1");                                          // report successful insertion
    emitter.instruction("ret");                                                 // return to the attach path
    emitter.label("__rt_tls_set_full_x86_64");
    emitter.instruction("sub rsp, 8");                                          // align the stack before closing the untracked session
    emitter.instruction("mov rdi, rsi");                                        // pass the untracked TLS session to elephc_tls_close
    abi::emit_load_symbol_to_reg(emitter, "r9", "_elephc_tls_close_fn", 0);     // load the published TLS close entry pointer
    emitter.instruction("test r9, r9");                                         // is defensive cleanup available?
    emitter.instruction("jz __rt_tls_set_full_closed_x86_64");                  // tolerate a missing bridge during defensive cleanup
    emitter.emit_published_bridge_call("r9");                                  // close through the published TLS adapter
    emitter.label("__rt_tls_set_full_closed_x86_64");
    emitter.instruction("add rsp, 8");                                          // restore the internal ABI stack position
    emitter.instruction("xor eax, eax");                                        // report bounded table exhaustion
    emitter.instruction("ret");                                                 // let the caller close the untracked TLS session

    emitter.label_global("__rt_tls_session_clear");
    abi::emit_symbol_address(emitter, "r8", "_tls_session_fds");
    abi::emit_symbol_address(emitter, "r9", "_tls_sessions");
    emitter.instruction("xor r10d, r10d");                                      // begin at the first bounded table slot
    emitter.label("__rt_tls_clear_loop_x86_64");
    emitter.instruction("cmp r10, 256");                                        // have all slots been searched?
    emitter.instruction("jge __rt_tls_clear_miss_x86_64");                      // no matching descriptor means no session to clear
    emitter.instruction("mov r11, QWORD PTR [r9 + r10 * 8]");                   // load the candidate session handle
    emitter.instruction("test r11, r11");                                       // is this slot live?
    emitter.instruction("jz __rt_tls_clear_next_x86_64");                       // skip unused slots
    emitter.instruction("cmp QWORD PTR [r8 + r10 * 8], rdi");                   // does this slot match the requested descriptor?
    emitter.instruction("je __rt_tls_clear_found_x86_64");                      // clear and return the matching session
    emitter.label("__rt_tls_clear_next_x86_64");
    emitter.instruction("add r10, 1");                                          // advance to the next bounded slot
    emitter.instruction("jmp __rt_tls_clear_loop_x86_64");                      // continue the lookup
    emitter.label("__rt_tls_clear_found_x86_64");
    emitter.instruction("mov QWORD PTR [r9 + r10 * 8], 0");                     // mark the session slot empty first
    emitter.instruction("mov QWORD PTR [r8 + r10 * 8], 0");                     // clear the stale descriptor key
    emitter.instruction("mov rax, r11");                                        // return the removed session for close_notify
    emitter.instruction("ret");                                                 // return the removed session
    emitter.label("__rt_tls_clear_miss_x86_64");
    emitter.instruction("xor eax, eax");                                        // no live session was attached to this descriptor
    emitter.instruction("ret");                                                 // return the clear miss
}

#[cfg(test)]
mod tests {
    use crate::codegen_support::platform::{Platform, Target};

    use super::*;

    /// Verifies x86_64 lookups compare full-width descriptor keys rather than using raw indexing.
    #[test]
    fn x86_64_table_uses_full_width_descriptor_keys() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_tls_session_table(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("cmp QWORD PTR [r8 + r10 * 8], rdi"));
        assert!(!asm.contains("_tls_sessions + rdi * 8"));
        assert!(asm.contains("cmp r10, 256"));
    }
}
