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
//! - Stores up to 16 `(protocol_ptr, protocol_len, class_ptr, class_len)`
//!   tuples in `_user_wrappers` (each slot is 32 bytes; an empty slot has a
//!   null `protocol_ptr`). Returns 1 on a successful registration, 0 when the
//!   table is full.
//! - v1 records the registration but the wrapper class is not yet invoked by
//!   `fopen`; the dispatch is a future Phase-10 commit.

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

    // -- store the registration into the empty slot --
    emitter.label("__rt_swr_store");
    emitter.instruction("str x0, [x6]");                                        // protocol pointer
    emitter.instruction("str x1, [x6, #8]");                                    // protocol length
    emitter.instruction("str x2, [x6, #16]");                                   // class-name pointer
    emitter.instruction("str x3, [x6, #24]");                                   // class-name length
    emitter.instruction("mov x0, #1");                                          // return true for a successful registration
    emitter.instruction("ret");                                                 // return to the caller

    emitter.label("__rt_swr_full");
    emitter.instruction("mov x0, #0");                                          // return false when the table is full
    emitter.instruction("ret");                                                 // return to the caller
}

/// Emits the Linux x86_64 stream runtime helper for stream wrapper register.
fn emit_stream_wrapper_register_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_wrapper_register ---");
    emitter.label_global("__rt_stream_wrapper_register");

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

    // -- store the registration into the empty slot --
    emitter.label("__rt_swr_store_x86");
    emitter.instruction("mov QWORD PTR [r10], rdi");                            // protocol pointer
    emitter.instruction("mov QWORD PTR [r10 + 8], rsi");                        // protocol length
    emitter.instruction("mov QWORD PTR [r10 + 16], rdx");                       // class-name pointer
    emitter.instruction("mov QWORD PTR [r10 + 24], rcx");                       // class-name length
    emitter.instruction("mov eax, 1");                                          // return true for a successful registration
    emitter.instruction("ret");                                                 // return to the caller

    emitter.label("__rt_swr_full_x86");
    emitter.instruction("xor eax, eax");                                        // return false when the table is full
    emitter.instruction("ret");                                                 // return to the caller
}
