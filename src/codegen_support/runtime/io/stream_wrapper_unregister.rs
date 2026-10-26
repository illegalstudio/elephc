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
//!   wins; all slot words are cleared before releasing its two owned strings. Returns 1 on
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

    abi::emit_symbol_address(emitter, "x4", "_user_wrappers");
    emitter.instruction("mov x5, #0");                                          // wrapper slot index
    emitter.label("__rt_swu_scan");
    emitter.instruction("cmp x5, #64");                                         // scanned every wrapper slot (USER_WRAPPER_REGISTRATIONS_CAP)?
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
    emitter.instruction("sub sp, sp, #32");                                     // reserve an aligned frame for string retirement
    emitter.instruction("stp x29, x30, [sp]");                                  // preserve the caller frame and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame
    emitter.instruction("ldr x0, [x6]");                                        // owned protocol pointer for release
    emitter.instruction("ldr x7, [x6, #16]");                                   // owned class-name pointer
    emitter.instruction("str x7, [sp, #16]");                                   // preserve the class name across protocol release
    emitter.instruction("stp xzr, xzr, [x6]");                                  // retire the protocol tuple before releasing storage
    emitter.instruction("stp xzr, xzr, [x6, #16]");                             // retire the class tuple before releasing storage
    emitter.instruction("bl __rt_heap_free_safe");                              // release the registration's protocol owner
    emitter.instruction("ldr x0, [sp, #16]");                                   // reload the registration's class-name owner
    emitter.instruction("bl __rt_heap_free_safe");                              // release the registration's class-name owner
    emitter.instruction("ldp x29, x30, [sp]");                                  // restore the caller frame and return address
    emitter.instruction("add sp, sp, #32");                                     // retire the aligned helper frame
    emitter.instruction("mov x0, #1");                                          // return true for a successful unregistration
    emitter.instruction("ret");                                                 // return to the caller

    emitter.label("__rt_swu_next");
    emitter.instruction("add x5, x5, #1");                                      // advance the slot index
    emitter.instruction("b __rt_swu_scan");                                     // continue scanning

    emitter.label("__rt_swu_miss");
    emitter.instruction("mov x0, #0");                                          // return false when no slot matched
    emitter.instruction("ret");                                                 // return to the caller
}

/// Emits the Linux x86_64 stream runtime helper for stream wrapper unregister.
fn emit_stream_wrapper_unregister_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_wrapper_unregister ---");
    emitter.label_global("__rt_stream_wrapper_unregister");

    abi::emit_symbol_address(emitter, "r8", "_user_wrappers");                  // wrapper table base
    emitter.instruction("xor r9, r9");                                          // wrapper slot index
    emitter.label("__rt_swu_scan_x86");
    emitter.instruction("cmp r9, 64");                                          // scanned every wrapper slot (USER_WRAPPER_REGISTRATIONS_CAP)?
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
    emitter.instruction("push rbp");                                            // preserve the caller frame
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame
    emitter.instruction("sub rsp, 16");                                         // reserve aligned storage for the second owner
    emitter.instruction("mov rcx, QWORD PTR [r10 + 16]");                       // owned class-name pointer
    emitter.instruction("mov QWORD PTR [rbp - 8], rcx");                        // preserve the class name across protocol release
    emitter.instruction("mov QWORD PTR [r10], 0");                              // retire the protocol pointer before release
    emitter.instruction("mov QWORD PTR [r10 + 8], 0");                          // retire the protocol length
    emitter.instruction("mov QWORD PTR [r10 + 16], 0");                         // retire the class pointer before release
    emitter.instruction("mov QWORD PTR [r10 + 24], 0");                         // retire the class-name length
    emitter.instruction("call __rt_heap_free_safe");                            // release the registration's protocol owner in rax
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the registration's class-name owner
    emitter.instruction("call __rt_heap_free_safe");                            // release the registration's class-name owner
    emitter.instruction("add rsp, 16");                                         // retire the aligned helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame
    emitter.instruction("mov eax, 1");                                          // return true for a successful unregistration
    emitter.instruction("ret");                                                 // return to the caller

    emitter.label("__rt_swu_next_x86");
    emitter.instruction("inc r9");                                              // advance the slot index
    emitter.instruction("jmp __rt_swu_scan_x86");                               // continue scanning

    emitter.label("__rt_swu_miss_x86");
    emitter.instruction("xor eax, eax");                                        // return false when no slot matched
    emitter.instruction("ret");                                                 // return to the caller
}
