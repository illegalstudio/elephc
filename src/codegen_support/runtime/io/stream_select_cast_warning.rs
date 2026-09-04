//! Purpose:
//! Emits `__rt_stream_select_cast_warning`, the diagnostics php raises for a stream it cannot turn
//! into a `select()`able descriptor.
//!
//! Called from:
//! - `crate::codegen_support::runtime::io::stream_select`, once per entry the cast rejected.
//!
//! Key details:
//! - php raises TWO warnings for a userspace wrapper that has no `stream_cast()` — first
//!   `W::stream_cast is not implemented!`, then `Cannot represent a stream of type user-space as a
//!   select()able descriptor` — and only the SECOND when the method exists and answers `false`.
//!   Measured on `php -n` 8.5.6 with both shapes. elephc raised neither, so a `stream_select()`
//!   over an uncastable wrapper produced the `ValueError` with nothing to explain it.
//! - The first message names the class, so it is composed at run time through
//!   `__rt_wrapper_missing_hook_warning`, the same composer every other missing-hook diagnostic
//!   uses. That is also what makes the two cases distinguishable: the vtable slot is empty in
//!   exactly the case php names the method.

use crate::codegen_support::runtime::data::{
    SELECT_CAST_UNREPRESENTABLE, WRAPPER_MISSING_HOOK_HEAD_SELECT, WRAPPER_MISSING_HOOK_TAIL_CAST,
};
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// The `stream_cast` vtable slot, mirroring `user_wrapper_cast`.
const VTABLE_SLOT_CAST: usize = 10;

/// Emits `__rt_stream_select_cast_warning(fd)`.
///
/// `fd` is the synthetic user-wrapper descriptor the cast refused. A descriptor that resolves to no
/// object reports only the second warning, because there is no class left to name.
pub fn emit_stream_select_cast_warning(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// The AArch64 composer.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_select cast warnings ---");
    emitter.label_global("__rt_stream_select_cast_warning");
    emitter.instruction("sub sp, sp, #32");                                     // frame for the saved linkage
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish the helper frame pointer

    // -- resolve the wrapper object behind the synthetic descriptor --
    emitter.instruction("mov w9, #0x4000");                                     // high half of the synthetic fd base
    emitter.instruction("lsl x9, x9, #16");                                     // form 0x40000000 in x9
    emitter.instruction("sub x9, x0, x9");                                      // x9 = fd - 0x40000000 = handle slot index
    super::emit_load_handles_cap(emitter, "x10");
    emitter.instruction("cmp x9, x10");
    emitter.instruction("b.hs __rt_sscw_unrepresentable");                      // out of range: nothing to name
    super::emit_load_handles_base(emitter, "x10");
    emitter.instruction("ldr x0, [x10, x9, lsl #3]");                           // obj = _user_wrapper_handles[slot]
    emitter.instruction("cbz x0, __rt_sscw_unrepresentable");                   // already closed: nothing to name

    // -- php names the method only when the class does not define it --
    emitter.instruction("ldr x10, [x0]");                                       // class_id stored at the head of every wrapper object
    abi::emit_symbol_address(emitter, "x11", "_user_wrapper_vtable_ptrs");
    emitter.instruction("ldr x11, [x11, x10, lsl #3]");                         // this class's wrapper vtable
    emitter.instruction("cbz x11, __rt_sscw_missing");                          // no vtable at all reads as no method
    emitter.instruction(&format!("ldr x11, [x11, #{}]", VTABLE_SLOT_CAST * 8)); // the stream_cast slot
    emitter.instruction("cbnz x11, __rt_sscw_unrepresentable");                 // it exists and simply refused
    emitter.label("__rt_sscw_missing");
    emitter.instruction("ldr x0, [x0]");                                        // the class id the composer names
    abi::emit_symbol_address(emitter, "x1", "_uwmh_head_select");
    emitter.instruction(&format!("mov x2, #{}", WRAPPER_MISSING_HOOK_HEAD_SELECT.len()));
    abi::emit_symbol_address(emitter, "x3", "_uwmh_tail_cast");
    emitter.instruction(&format!("mov x4, #{}", WRAPPER_MISSING_HOOK_TAIL_CAST.len()));
    emitter.instruction("bl __rt_wrapper_missing_hook_warning");

    emitter.label("__rt_sscw_unrepresentable");
    abi::emit_symbol_address(emitter, "x1", "_select_cast_unrepresentable");
    emitter.instruction(&format!("mov x2, #{}", SELECT_CAST_UNREPRESENTABLE.len()));
    emitter.instruction("bl __rt_diag_warning");                                // warnings honour the @ suppression depth
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");
}

/// The x86_64 composer.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_select cast warnings ---");
    emitter.label_global("__rt_stream_select_cast_warning");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame
    emitter.instruction("sub rsp, 16");                                         // keep the stack aligned for the calls

    emitter.instruction("mov r9, rdi");
    emitter.instruction("sub r9, 0x40000000");                                  // r9 = fd - 0x40000000 = handle slot index
    super::emit_load_handles_cap(emitter, "r10");
    emitter.instruction("cmp r9, r10");
    emitter.instruction("jae __rt_sscw_unrepresentable_x86");                   // out of range: nothing to name
    super::emit_load_handles_base(emitter, "r10");
    emitter.instruction("mov rdi, QWORD PTR [r10 + r9*8]");                     // obj = _user_wrapper_handles[slot]
    emitter.instruction("test rdi, rdi");
    emitter.instruction("jz __rt_sscw_unrepresentable_x86");                    // already closed: nothing to name

    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // class_id stored at the head of every wrapper object
    abi::emit_symbol_address(emitter, "r11", "_user_wrapper_vtable_ptrs");
    emitter.instruction("mov r11, QWORD PTR [r11 + r10*8]");                    // this class's wrapper vtable
    emitter.instruction("test r11, r11");
    emitter.instruction("jz __rt_sscw_missing_x86");                            // no vtable at all reads as no method
    emitter.instruction(&format!(
        "mov r11, QWORD PTR [r11 + {}]", VTABLE_SLOT_CAST * 8
    ));                                                                         // the stream_cast slot
    emitter.instruction("test r11, r11");
    emitter.instruction("jnz __rt_sscw_unrepresentable_x86");                   // it exists and simply refused
    emitter.label("__rt_sscw_missing_x86");
    emitter.instruction("mov rdi, QWORD PTR [rdi]");                            // the class id the composer names
    abi::emit_symbol_address(emitter, "rsi", "_uwmh_head_select");
    emitter.instruction(&format!("mov rdx, {}", WRAPPER_MISSING_HOOK_HEAD_SELECT.len()));
    abi::emit_symbol_address(emitter, "rcx", "_uwmh_tail_cast");
    emitter.instruction(&format!("mov r8, {}", WRAPPER_MISSING_HOOK_TAIL_CAST.len()));
    emitter.instruction("call __rt_wrapper_missing_hook_warning");

    emitter.label("__rt_sscw_unrepresentable_x86");
    abi::emit_symbol_address(emitter, "rdi", "_select_cast_unrepresentable");
    emitter.instruction(&format!("mov rsi, {}", SELECT_CAST_UNREPRESENTABLE.len()));
    emitter.instruction("call __rt_diag_warning");                              // warnings honour the @ suppression depth
    emitter.instruction("add rsp, 16");                                         // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");
}
