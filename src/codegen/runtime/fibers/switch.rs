//! Purpose:
//! Emits the low-level Fiber context switch primitive.
//! Owns saving the current execution context and restoring the target Fiber or main-thread context.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::fibers`.
//! - `crate::codegen::runtime::x86_minimal::emit_runtime_linux_x86_64_minimal()`.
//!
//! Key details:
//! - Saved register sets and stack alignment must match each target ABI exactly or Fiber resumes corrupt execution state.
//!
//! ARM64 callee-saved state preserved across switches:
//!   * General-purpose: x19–x28 (10 registers)
//!   * Frame/link:      x29 (FP), x30 (LR)
//!   * Floating-point:  d8–d15 (lower 64 bits of v8–v15)
//! Total = 11 GPRs + 8 FPRs = 152 bytes; rounded up to 160 for 16-byte alignment.
//!
//! Linux x86_64 SysV state preserved across switches:
//!   * General-purpose: rbx, rbp, r12–r15
//!   * Resume address:  normal call return address, left above the saved GPRs
//! Total = 6 GPRs + 1 return address = 56 bytes for a fresh stack frame.

use crate::codegen::abi;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

use super::{FIBER_OWN_CALL_FRAME_OFFSET, FIBER_OWN_EXC_HEAD_OFFSET, FIBER_SAVED_SP_OFFSET};

/// Total bytes saved on the stack by an AArch64 context switch (must stay 16-aligned).
const AARCH64_SWITCH_SAVE_BYTES: i32 = 160;

/// Total bytes pushed by a Linux x86_64 context switch, excluding the call return address.
const X86_64_SWITCH_SAVE_BYTES: i32 = 48;

/// Total bytes present in a fresh Linux x86_64 fiber frame, including the resume address.
const X86_64_INITIAL_FRAME_BYTES: i32 = X86_64_SWITCH_SAVE_BYTES + 8;

/// Offset within the Linux x86_64 initial frame where the resume address lives.
const X86_64_INITIAL_FRAME_RIP_OFFSET: i32 = X86_64_SWITCH_SAVE_BYTES;

/// __rt_fiber_switch: cooperative context switch.
/// Input:  x0 = target Fiber* (NULL = switch back to main thread)
/// Output: control returns when the *current* context is later switched back to.
pub fn emit_fiber_switch(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: fiber_switch ---");
    emitter.label_global("__rt_fiber_switch");

    // -- save callee-saved state on the current stack --
    emitter.instruction(&format!("sub sp, sp, #{}", AARCH64_SWITCH_SAVE_BYTES)); // reserve space for the full switch save area
    emitter.instruction("stp x19, x20, [sp]");                                  // save callee-saved general-purpose registers x19/x20
    emitter.instruction("stp x21, x22, [sp, #16]");                             // save callee-saved general-purpose registers x21/x22
    emitter.instruction("stp x23, x24, [sp, #32]");                             // save callee-saved general-purpose registers x23/x24
    emitter.instruction("stp x25, x26, [sp, #48]");                             // save callee-saved general-purpose registers x25/x26
    emitter.instruction("stp x27, x28, [sp, #64]");                             // save callee-saved general-purpose registers x27/x28
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save the frame pointer and the return address that ret will jump to on resume
    emitter.instruction("stp d8, d9, [sp, #96]");                               // save callee-saved floating-point registers d8/d9
    emitter.instruction("stp d10, d11, [sp, #112]");                            // save callee-saved floating-point registers d10/d11
    emitter.instruction("stp d12, d13, [sp, #128]");                            // save callee-saved floating-point registers d12/d13
    emitter.instruction("stp d14, d15, [sp, #144]");                            // save callee-saved floating-point registers d14/d15

    // -- determine the source context (current fiber, or main if NULL) --
    abi::emit_load_symbol_to_reg(emitter, "x10", "_fiber_current", 0);          // x10 = source fiber* (NULL means we're suspending the main thread)
    emitter.instruction("mov x11, sp");                                         // x11 = SP to remember as the source context's resume point
    emitter.instruction("cbz x10, __rt_fiber_switch_save_main_sp");             // when source is the main thread, save into the main-thread slot

    // -- source = a fiber: persist its SP, exception chain head, and call-frame chain head into the object --
    emitter.instruction(&format!("str x11, [x10, #{}]", FIBER_SAVED_SP_OFFSET)); // source_fiber->saved_sp = SP
    abi::emit_load_symbol_to_reg(emitter, "x12", "_exc_handler_top", 0);        // x12 = current head of the global try/catch handler chain
    emitter.instruction(&format!("str x12, [x10, #{}]", FIBER_OWN_EXC_HEAD_OFFSET)); // source_fiber->own_exc_head = head
    abi::emit_load_symbol_to_reg(emitter, "x13", "_exc_call_frame_top", 0);     // x13 = current head of the activation-record cleanup chain
    emitter.instruction(&format!("str x13, [x10, #{}]", FIBER_OWN_CALL_FRAME_OFFSET)); // source_fiber->own_call_frame = head
    emitter.instruction("b __rt_fiber_switch_load_target");                     // skip the main-thread save path

    // -- source = main thread: persist its SP, exception chain head, and call-frame chain head into globals --
    emitter.label("__rt_fiber_switch_save_main_sp");
    abi::emit_store_reg_to_symbol(emitter, "x11", "_fiber_main_saved_sp", 0);   // _fiber_main_saved_sp = SP
    abi::emit_load_symbol_to_reg(emitter, "x12", "_exc_handler_top", 0);        // x12 = current head of the global try/catch handler chain on main
    abi::emit_store_reg_to_symbol(emitter, "x12", "_fiber_main_saved_exc", 0);  // _fiber_main_saved_exc = main thread handler chain head
    abi::emit_load_symbol_to_reg(emitter, "x13", "_exc_call_frame_top", 0);     // x13 = current head of the activation-record cleanup chain on main
    abi::emit_store_reg_to_symbol(emitter, "x13", "_fiber_main_saved_call_frame", 0); // _fiber_main_saved_call_frame = main thread call-frame chain head

    // -- swap _fiber_current to the target and load its context --
    emitter.label("__rt_fiber_switch_load_target");
    abi::emit_store_reg_to_symbol(emitter, "x0", "_fiber_current", 0);          // _fiber_current = target fiber* (or NULL = main)
    emitter.instruction("cbz x0, __rt_fiber_switch_load_main");                 // restore main-thread state when target is NULL

    // -- target = a fiber: load its SP, exception chain head, and call-frame chain head from the object --
    emitter.instruction(&format!("ldr x12, [x0, #{}]", FIBER_OWN_EXC_HEAD_OFFSET)); // x12 = target fiber's saved try/catch handler chain head
    abi::emit_store_reg_to_symbol(emitter, "x12", "_exc_handler_top", 0);       // restore the target fiber's handler chain head globally
    emitter.instruction(&format!("ldr x13, [x0, #{}]", FIBER_OWN_CALL_FRAME_OFFSET)); // x13 = target fiber's saved activation-record cleanup chain head
    abi::emit_store_reg_to_symbol(emitter, "x13", "_exc_call_frame_top", 0);    // restore the target fiber's call-frame chain head globally
    emitter.instruction(&format!("ldr x11, [x0, #{}]", FIBER_SAVED_SP_OFFSET)); // x11 = target fiber's saved SP
    emitter.instruction("mov sp, x11");                                         // adopt the target fiber's stack
    emitter.instruction("b __rt_fiber_switch_restore");                         // proceed to restore callee-saved registers

    // -- target = main thread: load saved SP, exception chain head, and call-frame chain head from globals --
    emitter.label("__rt_fiber_switch_load_main");
    abi::emit_load_symbol_to_reg(emitter, "x12", "_fiber_main_saved_exc", 0);   // x12 = main thread's saved try/catch handler chain head
    abi::emit_store_reg_to_symbol(emitter, "x12", "_exc_handler_top", 0);       // restore the main thread handler chain head globally
    abi::emit_load_symbol_to_reg(emitter, "x13", "_fiber_main_saved_call_frame", 0); // x13 = main thread's saved activation-record cleanup chain head
    abi::emit_store_reg_to_symbol(emitter, "x13", "_exc_call_frame_top", 0);    // restore the main thread call-frame chain head globally
    abi::emit_load_symbol_to_reg(emitter, "x11", "_fiber_main_saved_sp", 0);    // x11 = main thread's saved SP
    emitter.instruction("mov sp, x11");                                         // adopt the main thread's stack

    // -- restore callee-saved state from the target's stack and return into it --
    emitter.label("__rt_fiber_switch_restore");
    emitter.instruction("ldp x19, x20, [sp]");                                  // restore callee-saved general-purpose registers x19/x20
    emitter.instruction("ldp x21, x22, [sp, #16]");                             // restore callee-saved general-purpose registers x21/x22
    emitter.instruction("ldp x23, x24, [sp, #32]");                             // restore callee-saved general-purpose registers x23/x24
    emitter.instruction("ldp x25, x26, [sp, #48]");                             // restore callee-saved general-purpose registers x25/x26
    emitter.instruction("ldp x27, x28, [sp, #64]");                             // restore callee-saved general-purpose registers x27/x28
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore the target's frame pointer and the resume return address
    emitter.instruction("ldp d8, d9, [sp, #96]");                               // restore callee-saved floating-point registers d8/d9
    emitter.instruction("ldp d10, d11, [sp, #112]");                            // restore callee-saved floating-point registers d10/d11
    emitter.instruction("ldp d12, d13, [sp, #128]");                            // restore callee-saved floating-point registers d12/d13
    emitter.instruction("ldp d14, d15, [sp, #144]");                            // restore callee-saved floating-point registers d14/d15
    emitter.instruction(&format!("add sp, sp, #{}", AARCH64_SWITCH_SAVE_BYTES)); // release the switch save area on the target's stack
    emitter.instruction("ret");                                                 // resume the target context where it last yielded
}

/// Number of bytes a freshly-prepared fiber stack reserves for the entry frame.
pub(crate) fn fiber_initial_stack_frame_bytes(arch: Arch) -> i32 {
    match arch {
        Arch::AArch64 => AARCH64_SWITCH_SAVE_BYTES,
        Arch::X86_64 => X86_64_INITIAL_FRAME_BYTES,
    }
}

/// Offset within the initial frame where the entry trampoline address is stored.
pub(crate) fn fiber_initial_entry_offset(arch: Arch) -> i32 {
    match arch {
        Arch::AArch64 => 88,
        Arch::X86_64 => X86_64_INITIAL_FRAME_RIP_OFFSET,
    }
}

fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fiber_switch ---");
    emitter.label_global("__rt_fiber_switch");

    // -- save callee-saved state on the current stack --
    emitter.instruction("push rbx");                                            // preserve the source context's callee-saved base register
    emitter.instruction("push rbp");                                            // preserve the source context's frame pointer
    emitter.instruction("push r12");                                            // preserve the first source context callee-saved scratch register
    emitter.instruction("push r13");                                            // preserve the second source context callee-saved scratch register
    emitter.instruction("push r14");                                            // preserve the third source context callee-saved scratch register
    emitter.instruction("push r15");                                            // preserve the fourth source context callee-saved scratch register

    // -- determine the source context (current fiber, or main if NULL) --
    abi::emit_load_symbol_to_reg(emitter, "r10", "_fiber_current", 0);          // r10 = source fiber* (NULL means we're suspending the main thread)
    emitter.instruction("mov r11, rsp");                                        // r11 = SP to remember as the source context's resume point
    emitter.instruction("test r10, r10");                                       // is the current execution context the main thread?
    emitter.instruction("jz __rt_fiber_switch_save_main_sp");                   // when source is main, save into the main-thread slot

    // -- source = a fiber: persist its SP and exception/call-frame chain heads --
    emitter.instruction(&format!("mov QWORD PTR [r10 + {}], r11", FIBER_SAVED_SP_OFFSET)); // source_fiber->saved_sp = SP
    abi::emit_load_symbol_to_reg(emitter, "r11", "_exc_handler_top", 0);        // r11 = current head of the global try/catch handler chain
    emitter.instruction(&format!("mov QWORD PTR [r10 + {}], r11", FIBER_OWN_EXC_HEAD_OFFSET)); // source_fiber->own_exc_head = head
    abi::emit_load_symbol_to_reg(emitter, "r11", "_exc_call_frame_top", 0);     // r11 = current head of the activation-record cleanup chain
    emitter.instruction(&format!("mov QWORD PTR [r10 + {}], r11", FIBER_OWN_CALL_FRAME_OFFSET)); // source_fiber->own_call_frame = head
    emitter.instruction("jmp __rt_fiber_switch_load_target");                   // skip the main-thread save path

    // -- source = main thread: persist its SP and exception/call-frame chain heads --
    emitter.label("__rt_fiber_switch_save_main_sp");
    abi::emit_store_reg_to_symbol(emitter, "rsp", "_fiber_main_saved_sp", 0);   // _fiber_main_saved_sp = source resume SP
    abi::emit_load_symbol_to_reg(emitter, "r11", "_exc_handler_top", 0);        // r11 = current head of the main-thread handler chain
    abi::emit_store_reg_to_symbol(emitter, "r11", "_fiber_main_saved_exc", 0);  // _fiber_main_saved_exc = main thread handler chain head
    abi::emit_load_symbol_to_reg(emitter, "r11", "_exc_call_frame_top", 0);     // r11 = current head of the main-thread cleanup chain
    abi::emit_store_reg_to_symbol(emitter, "r11", "_fiber_main_saved_call_frame", 0); // _fiber_main_saved_call_frame = main thread cleanup chain head

    // -- swap _fiber_current to the target and load its context --
    emitter.label("__rt_fiber_switch_load_target");
    abi::emit_store_reg_to_symbol(emitter, "rdi", "_fiber_current", 0);         // _fiber_current = target fiber* (or NULL = main)
    emitter.instruction("test rdi, rdi");                                       // is the target the main thread?
    emitter.instruction("jz __rt_fiber_switch_load_main");                      // restore main-thread state when target is NULL

    // -- target = a fiber: load its SP and exception/call-frame chain heads --
    emitter.instruction(&format!("mov r11, QWORD PTR [rdi + {}]", FIBER_OWN_EXC_HEAD_OFFSET)); // r11 = target fiber's handler chain head
    abi::emit_store_reg_to_symbol(emitter, "r11", "_exc_handler_top", 0);       // restore the target fiber's handler chain head globally
    emitter.instruction(&format!("mov r11, QWORD PTR [rdi + {}]", FIBER_OWN_CALL_FRAME_OFFSET)); // r11 = target fiber's cleanup chain head
    abi::emit_store_reg_to_symbol(emitter, "r11", "_exc_call_frame_top", 0);    // restore the target fiber's cleanup chain head globally
    emitter.instruction(&format!("mov rsp, QWORD PTR [rdi + {}]", FIBER_SAVED_SP_OFFSET)); // adopt the target fiber's saved stack pointer
    emitter.instruction("jmp __rt_fiber_switch_restore");                       // proceed to restore callee-saved registers

    // -- target = main thread: load saved SP and exception/call-frame chain heads --
    emitter.label("__rt_fiber_switch_load_main");
    abi::emit_load_symbol_to_reg(emitter, "r11", "_fiber_main_saved_exc", 0);   // r11 = main thread's saved handler chain head
    abi::emit_store_reg_to_symbol(emitter, "r11", "_exc_handler_top", 0);       // restore the main thread handler chain head globally
    abi::emit_load_symbol_to_reg(emitter, "r11", "_fiber_main_saved_call_frame", 0); // r11 = main thread's saved cleanup chain head
    abi::emit_store_reg_to_symbol(emitter, "r11", "_exc_call_frame_top", 0);    // restore the main thread cleanup chain head globally
    abi::emit_load_symbol_to_reg(emitter, "rsp", "_fiber_main_saved_sp", 0);    // adopt the main thread's saved stack pointer

    // -- restore callee-saved state from the target stack and return into it --
    emitter.label("__rt_fiber_switch_restore");
    emitter.instruction("pop r15");                                             // restore the fourth target callee-saved scratch register
    emitter.instruction("pop r14");                                             // restore the third target callee-saved scratch register
    emitter.instruction("pop r13");                                             // restore the second target callee-saved scratch register
    emitter.instruction("pop r12");                                             // restore the first target callee-saved scratch register
    emitter.instruction("pop rbp");                                             // restore the target frame pointer
    emitter.instruction("pop rbx");                                             // restore the target callee-saved base register
    emitter.instruction("ret");                                                 // resume the target context using its saved return address
}
