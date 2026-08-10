//! Purpose:
//! Emits the `stream_wrapper_register` runtime helper
//! `__rt_stream_wrapper_register`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - `__rt_stream_wrapper_register` is the entry point invoked by the
//!   `stream_wrapper_register` builtin.
//!
//! Key details:
//! - Stores up to 64 `(protocol_ptr, protocol_len, class_ptr, class_len)`
//!   tuples in `_user_wrappers` (each slot is 32 bytes; an empty slot has a
//!   null `protocol_ptr`). Returns 1 on a successful registration, 0 when the
//!   table is full.
//! - Both strings are persisted with `__rt_str_persist` before being stored:
//!   the registration outlives the call, so keeping the caller's pointer means
//!   the registered scheme silently follows whatever that buffer holds later.
//!   A literal argument hid this — it lives in rodata and never moves — while
//!   `$s = "aa"; stream_wrapper_register($s, "W"); $s = "zz";` left `aa://`
//!   unroutable and `zz://`, never registered, dispatching into the wrapper.
//! - The free slot is located BEFORE persisting, so a full table allocates
//!   nothing.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits the `__rt_stream_wrapper_register` runtime helper.
/// Input:  AArch64 x0 = proto ptr, x1 = proto len, x2 = class ptr, x3 = class len.
///         x86_64  rdi = proto ptr, rsi = proto len, rdx = class ptr, rcx = class len.
/// Output: 1 when the registration was stored, 0 when the table is full.
pub fn emit_stream_wrapper_register(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stream_wrapper_register_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: stream_wrapper_register ---");
    emitter.label_global("__rt_stream_wrapper_register");

    // Frame: [sp,#0..16] x29/x30, [sp,#16] free slot base, [sp,#24] class-name
    //   pointer, [sp,#32] class-name length.
    emitter.instruction("sub sp, sp, #48");                                     // frame for the two string-persist calls
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer

    // -- scan _user_wrappers for the first empty slot --
    abi::emit_symbol_address(emitter, "x4", "_user_wrappers");
    emitter.instruction("mov x5, #0");                                          // wrapper slot index
    emitter.label("__rt_swr_scan");
    emitter.instruction("cmp x5, #64");                                         // is the wrapper table full (USER_WRAPPER_REGISTRATIONS_CAP)?
    emitter.instruction("b.ge __rt_swr_full");                                  // no empty slot remains
    emitter.instruction("add x6, x4, x5, lsl #5");                              // slot base = table + index * 32
    emitter.instruction("ldr x7, [x6]");                                        // load the slot's protocol pointer
    emitter.instruction("cbz x7, __rt_swr_store");                              // a null pointer marks an empty slot
    emitter.instruction("add x5, x5, #1");                                      // advance the slot index
    emitter.instruction("b __rt_swr_scan");                                     // continue scanning

    // -- take ownership of both strings, then store the registration --
    emitter.label("__rt_swr_store");
    emitter.instruction("str x6, [sp, #16]");                                   // save the free slot base across the persist calls
    emitter.instruction("str x2, [sp, #24]");                                   // save the class-name pointer across the protocol persist
    emitter.instruction("str x3, [sp, #32]");                                   // save the class-name length across the protocol persist
    emitter.instruction("mov x2, x1");                                          // protocol length → persist length input
    emitter.instruction("mov x1, x0");                                          // protocol pointer → persist pointer input
    emitter.instruction("bl __rt_str_persist");                                 // x1 = owned protocol bytes, x2 = length
    emitter.instruction("ldr x6, [sp, #16]");                                   // reload the free slot base
    emitter.instruction("str x1, [x6]");                                        // owned protocol pointer
    emitter.instruction("str x2, [x6, #8]");                                    // protocol length
    emitter.instruction("ldr x1, [sp, #24]");                                   // class-name pointer → persist pointer input
    emitter.instruction("ldr x2, [sp, #32]");                                   // class-name length → persist length input
    emitter.instruction("bl __rt_str_persist");                                 // x1 = owned class name, x2 = length
    emitter.instruction("ldr x6, [sp, #16]");                                   // reload the free slot base
    emitter.instruction("str x1, [x6, #16]");                                   // owned class-name pointer
    emitter.instruction("str x2, [x6, #24]");                                   // class-name length
    emitter.instruction("mov x0, #1");                                          // return true for a successful registration
    emitter.instruction("b __rt_swr_ret");                                      // share the common epilogue

    emitter.label("__rt_swr_full");
    emitter.instruction("mov x0, #0");                                          // return false when the table is full

    emitter.label("__rt_swr_ret");
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return to the caller
}

/// Emits the Linux x86_64 stream runtime helper for stream wrapper register.
fn emit_stream_wrapper_register_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_wrapper_register ---");
    emitter.label_global("__rt_stream_wrapper_register");

    // Frame: [rbp-8] free slot base, [rbp-16] class-name pointer, [rbp-24]
    //   class-name length. push rbp then sub rsp,48 keeps rsp 16-aligned.
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 48");                                         // frame for the two string-persist calls

    // -- scan _user_wrappers for the first empty slot --
    abi::emit_symbol_address(emitter, "r8", "_user_wrappers");                  // wrapper table base
    emitter.instruction("xor r9, r9");                                          // wrapper slot index
    emitter.label("__rt_swr_scan_x86");
    emitter.instruction("cmp r9, 64");                                          // is the wrapper table full (USER_WRAPPER_REGISTRATIONS_CAP)?
    emitter.instruction("jge __rt_swr_full_x86");                               // no empty slot remains
    emitter.instruction("mov r10, r9");                                         // copy the slot index for scaling
    emitter.instruction("shl r10, 5");                                          // slot offset = index * 32
    emitter.instruction("add r10, r8");                                         // slot base = table + offset
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load the slot's protocol pointer
    emitter.instruction("test r11, r11");                                       // a null pointer marks an empty slot
    emitter.instruction("jz __rt_swr_store_x86");                               // store here
    emitter.instruction("inc r9");                                              // advance the slot index
    emitter.instruction("jmp __rt_swr_scan_x86");                               // continue scanning

    // -- take ownership of both strings, then store the registration --
    emitter.label("__rt_swr_store_x86");
    emitter.instruction("mov QWORD PTR [rbp - 8], r10");                        // save the free slot base across the persist calls
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the class-name pointer across the protocol persist
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // save the class-name length across the protocol persist
    emitter.instruction("mov rax, rdi");                                        // protocol pointer → persist pointer input
    emitter.instruction("mov rdx, rsi");                                        // protocol length → persist length input
    emitter.instruction("call __rt_str_persist");                               // rax = owned protocol bytes, rdx = length
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the free slot base
    emitter.instruction("mov QWORD PTR [r10], rax");                            // owned protocol pointer
    emitter.instruction("mov QWORD PTR [r10 + 8], rdx");                        // protocol length
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // class-name pointer → persist pointer input
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // class-name length → persist length input
    emitter.instruction("call __rt_str_persist");                               // rax = owned class name, rdx = length
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the free slot base
    emitter.instruction("mov QWORD PTR [r10 + 16], rax");                       // owned class-name pointer
    emitter.instruction("mov QWORD PTR [r10 + 24], rdx");                       // class-name length
    emitter.instruction("mov eax, 1");                                          // return true for a successful registration
    emitter.instruction("jmp __rt_swr_ret_x86");                                // share the common epilogue

    emitter.label("__rt_swr_full_x86");
    emitter.instruction("xor eax, eax");                                        // return false when the table is full

    emitter.label("__rt_swr_ret_x86");
    emitter.instruction("add rsp, 48");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller
}
