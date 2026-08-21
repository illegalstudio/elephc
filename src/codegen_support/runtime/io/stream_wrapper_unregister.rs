//! Purpose:
//! Emits the `stream_wrapper_unregister` runtime helper
//! `__rt_stream_wrapper_unregister`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - Scans `_user_wrappers` for a slot whose stored `(protocol_ptr,
//!   protocol_len)` matches the call argument byte-for-byte. The first match
//!   wins; the slot is cleared by setting `protocol_ptr` to `0`. Returns 1 on
//!   a successful unregistration, 0 when no registered protocol matches.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits the `__rt_stream_wrapper_unregister` runtime helper.
/// Input:  AArch64 x0 = protocol ptr, x1 = protocol len.
///         x86_64  rdi = protocol ptr, rsi = protocol len.
/// Output: 1 when a slot was cleared, 0 when no match was found.
pub fn emit_stream_wrapper_unregister(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stream_wrapper_unregister_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: stream_wrapper_unregister ---");
    emitter.label_global("__rt_stream_wrapper_unregister");
    emitter.instruction("mov x15, x0");                                         // preserve the protocol pointer for the built-in probe
    emitter.instruction("mov x16, x1");                                         // preserve the protocol length

    super::emit_load_table_base(emitter, "x4");
    emitter.instruction("mov x5, #0");                                          // wrapper slot index
    emitter.label("__rt_swu_scan");
    super::emit_load_table_cap(emitter, "x6");
    emitter.instruction("cmp x5, x6");                                         // scanned every allocated wrapper slot?
    emitter.instruction("b.ge __rt_swu_miss");                                  // no match found
    emitter.instruction("add x6, x4, x5, lsl #5");                              // slot base = table + index * 32
    emitter.instruction("ldr x7, [x6]");                                        // stored protocol pointer
    emitter.instruction("cbz x7, __rt_swu_next");                               // skip empty slots
    emitter.instruction("ldr x8, [x6, #8]");                                    // stored protocol length
    emitter.instruction("cmp x8, x1");                                          // do the lengths match?
    emitter.instruction("b.ne __rt_swu_next");                                  // different length: not this slot

    // -- lengths match: compare the protocol bytes --
    emitter.instruction("mov x9, #0");                                          // protocol byte compare index
    emitter.label("__rt_swu_cmp");
    emitter.instruction("cmp x9, x1");                                          // compared every byte?
    emitter.instruction("b.ge __rt_swu_match");                                 // bytes fully match
    emitter.instruction("ldrb w10, [x7, x9]");                                  // stored protocol byte
    emitter.instruction("ldrb w11, [x0, x9]");                                  // requested protocol byte
    emitter.instruction("cmp w10, w11");                                        // does this byte match?
    emitter.instruction("b.ne __rt_swu_next");                                  // bytes differ: not this slot
    emitter.instruction("add x9, x9, #1");                                      // advance the byte compare index
    emitter.instruction("b __rt_swu_cmp");                                      // continue comparing bytes

    emitter.label("__rt_swu_match");
    emitter.instruction("str xzr, [x6]");                                       // clear the protocol pointer to free the slot
    super::emit_load_flags_base(emitter, "x7");
    emitter.instruction("str xzr, [x7, x5, lsl #3]");                           // clear stale definition flags before slot reuse
    emitter.instruction("mov x0, #1");                                          // return true for a successful unregistration
    emitter.instruction("ret");                                                 // return to the caller

    emitter.label("__rt_swu_next");
    emitter.instruction("add x5, x5, #1");                                      // advance the slot index
    emitter.instruction("b __rt_swu_scan");                                     // continue scanning

    emitter.label("__rt_swu_miss");
    // PHP allows a built-in wrapper to be unregistered. Built-ins are not in the
    // user table, so a miss falls through to the disabled-wrapper bitmask before
    // reporting failure.
    emitter.instruction("sub sp, sp, #32");                                     // frame for the built-in probe
    emitter.instruction("stp x29, x30, [sp, #16]");
    emitter.instruction("add x29, sp, #16");
    emitter.instruction("mov x0, x15");                                         // restore the protocol pointer
    emitter.instruction("mov x1, x16");                                         // restore the protocol length
    emitter.instruction("bl __rt_builtin_wrapper_index");                       // x0 = built-in index or -1
    emitter.instruction("cmp x0, #0");
    emitter.instruction("b.lt __rt_swu_really_miss");                           // not a built-in either
    abi::emit_symbol_address(emitter, "x9", "_disabled_builtin_wrappers");
    emitter.instruction("ldr x10, [x9]");                                       // current disabled mask
    emitter.instruction("mov x11, #1");
    emitter.instruction("lsl x11, x11, x0");                                    // bit for this wrapper
    emitter.instruction("tst x10, x11");                                        // already unregistered?
    emitter.instruction("b.ne __rt_swu_really_miss");                           // PHP reports false on a second unregister
    emitter.instruction("orr x10, x10, x11");                                   // mark the built-in disabled
    emitter.instruction("str x10, [x9]");
    emitter.instruction("mov x0, #1");                                          // report a successful unregistration
    emitter.instruction("ldp x29, x30, [sp, #16]");
    emitter.instruction("add sp, sp, #32");
    emitter.instruction("ret");                                                 // return to the caller
    emitter.label("__rt_swu_really_miss");
    emitter.instruction("mov x0, #0");                                          // return false when nothing matched
    emitter.instruction("ldp x29, x30, [sp, #16]");
    emitter.instruction("add sp, sp, #32");
    emitter.instruction("ret");                                                 // return to the caller
}

/// Emits the Linux x86_64 stream runtime helper for stream wrapper unregister.
fn emit_stream_wrapper_unregister_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_wrapper_unregister ---");
    emitter.label_global("__rt_stream_wrapper_unregister");

    super::emit_load_table_base(emitter, "r8");                  // wrapper table base
    emitter.instruction("xor r9, r9");                                          // wrapper slot index
    emitter.label("__rt_swu_scan_x86");
    super::emit_load_table_cap(emitter, "r10");
    emitter.instruction("cmp r9, r10");                                          // scanned every allocated wrapper slot?
    emitter.instruction("jge __rt_swu_miss_x86");                               // no match found
    emitter.instruction("mov r10, r9");                                         // copy the slot index for scaling
    emitter.instruction("shl r10, 5");                                          // slot offset = index * 32
    emitter.instruction("add r10, r8");                                         // slot base = table + offset
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // stored protocol pointer
    emitter.instruction("test rax, rax");                                       // is this slot empty?
    emitter.instruction("jz __rt_swu_next_x86");                                // skip empty slots
    emitter.instruction("mov r11, QWORD PTR [r10 + 8]");                        // stored protocol length
    emitter.instruction("cmp r11, rsi");                                        // do the lengths match?
    emitter.instruction("jne __rt_swu_next_x86");                               // different length: not this slot

    // -- lengths match: compare the protocol bytes --
    emitter.instruction("xor rcx, rcx");                                        // protocol byte compare index
    emitter.label("__rt_swu_cmp_x86");
    emitter.instruction("cmp rcx, rsi");                                        // compared every byte?
    emitter.instruction("jge __rt_swu_match_x86");                              // bytes fully match
    emitter.instruction("movzx edx, BYTE PTR [rax + rcx]");                     // stored protocol byte
    emitter.instruction("movzx r11d, BYTE PTR [rdi + rcx]");                    // requested protocol byte
    emitter.instruction("cmp dl, r11b");                                        // does this byte match?
    emitter.instruction("jne __rt_swu_next_x86");                               // bytes differ: not this slot
    emitter.instruction("inc rcx");                                             // advance the byte compare index
    emitter.instruction("jmp __rt_swu_cmp_x86");                                // continue comparing bytes

    emitter.label("__rt_swu_match_x86");
    emitter.instruction("mov QWORD PTR [r10], 0");                              // clear the protocol pointer to free the slot
    super::emit_load_flags_base(emitter, "r10");
    emitter.instruction("mov QWORD PTR [r10 + r9 * 8], 0");                     // clear stale definition flags before slot reuse
    emitter.instruction("mov eax, 1");                                          // return true for a successful unregistration
    emitter.instruction("ret");                                                 // return to the caller

    emitter.label("__rt_swu_next_x86");
    emitter.instruction("inc r9");                                              // advance the slot index
    emitter.instruction("jmp __rt_swu_scan_x86");                               // continue scanning

    emitter.label("__rt_swu_miss_x86");
    // PHP allows a built-in wrapper to be unregistered. Built-ins are not in the user table, so
    // a miss falls through to the disabled-wrapper bitmask before reporting failure — the
    // AArch64 counterpart has always done this, and its absence here left every built-in
    // unregistration a no-op on x86_64, so `stream_wrapper_restore()` then reported the wrapper
    // as never changed.
    emitter.instruction("push rbp");                                            // align the stack for the probe call
    emitter.instruction("mov rbp, rsp");                                        // establish a frame to return through
    // rdi/rsi still hold the requested protocol: the scan above reads them but never writes them.
    emitter.instruction("call __rt_builtin_wrapper_index");                     // rax = built-in index or -1
    emitter.instruction("cmp rax, 0");
    emitter.instruction("jl __rt_swu_really_miss_x86");                         // not a built-in either
    emitter.instruction("mov rcx, rax");                                        // the index selects the bit
    abi::emit_symbol_address(emitter, "r9", "_disabled_builtin_wrappers");
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // current disabled mask
    emitter.instruction("mov r11, 1");
    emitter.instruction("shl r11, cl");                                         // bit for this wrapper
    emitter.instruction("test r10, r11");                                       // already unregistered?
    emitter.instruction("jnz __rt_swu_really_miss_x86");                        // PHP reports false on a second unregister
    emitter.instruction("or r10, r11");                                         // mark the built-in disabled
    emitter.instruction("mov QWORD PTR [r9], r10");
    emitter.instruction("mov eax, 1");                                          // report a successful unregistration
    emitter.instruction("mov rsp, rbp");
    emitter.instruction("pop rbp");
    emitter.instruction("ret");                                                 // return to the caller
    emitter.label("__rt_swu_really_miss_x86");
    emitter.instruction("xor eax, eax");                                        // return false when nothing matched
    emitter.instruction("mov rsp, rbp");
    emitter.instruction("pop rbp");
    emitter.instruction("ret");                                                 // return to the caller
}
