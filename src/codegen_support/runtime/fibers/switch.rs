//! Purpose:
//! Emits the low-level Fiber context switch primitive.
//! Owns saving the current execution context and restoring the target Fiber or main-thread context.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::fibers`.
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

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use super::FIBER_SAVED_SP_OFFSET;
use elephc_memoir::coroutine_storage::{MAIN_SAVED_SP, SuspendedContext};

/// Total bytes saved on the stack by an AArch64 context switch (must stay 16-aligned).
const AARCH64_SWITCH_SAVE_BYTES: i32 = 160;

/// Total bytes pushed by a Linux x86_64 context switch, excluding the call return address.
const X86_64_SWITCH_SAVE_BYTES: i32 = 48;

/// Total bytes present in a fresh Linux x86_64 fiber frame, including the resume address.
const X86_64_INITIAL_FRAME_BYTES: i32 = X86_64_SWITCH_SAVE_BYTES + 8;

/// Offset within the Linux x86_64 initial frame where the resume address lives.
const X86_64_INITIAL_FRAME_RIP_OFFSET: i32 = X86_64_SWITCH_SAVE_BYTES;

/// Emits `__rt_fiber_switch`: a cooperative context switch between Fiber execution contexts.
///
/// # Input
/// - `x0`: target `Fiber*` — NULL switches back to the main thread, non-NULL switches to that fiber.
/// - Callee-saved registers and floating-point state of the *source* context are saved to the source's stack.
/// - Global `_fiber_current`, `_exc_handler_top`, `_exc_call_frame_top`, and
///   `_magic_set_guard_head` track the active execution context across switches.
/// - Deep-release GC suppression follows its coroutine. The collector reentry flag remains
///   global, so switching stacks cannot start another collector inside an active collection.
///
/// # Output
/// - Control does not return from this function normally. When the current context is later switched back to,
///   execution resumes immediately after the `ret` instruction with all state fully restored.
///
/// # Behavior
/// - When `x0` is NULL: the source's SP and runtime chain heads are saved to the main-thread
///   globals, then the main thread's saved context is restored.
/// - When `x0` is non-NULL: the source's state is persisted into the source `Fiber` object at offsets
///   including `FIBER_OWN_MAGIC_SET_GUARD_OFFSET`; the target fiber's saved state is loaded
///   and restored, adopting the target's stack.
///
/// # ABI Notes
/// - ARM64: saves x19–x28, x29, x30, d8–d15 (160 bytes, 16-aligned) to the source stack, then restores the
///   same register set from the target's stack before returning.
/// - x86_64: uses the matched helper `emit_x86_64` which saves/restores rbx, rbp, r12–r15 per the SysV ABI.
///
/// Called from `emit_fiber_switch` on ARM64 targets.
pub fn emit_fiber_switch(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: fiber_switch ---");
    emitter.label_global("__rt_fiber_switch");

    // -- PHP forbids cooperative context switches from inside a signal callback --
    abi::emit_load_symbol_to_reg(emitter, "x9", "__rt_pcntl_dispatching", 0);  // x9 = active signal-dispatch guard
    emitter.instruction("cbz x9, __rt_fiber_switch_signal_ok");                 // ordinary fiber switches proceed outside handlers
    abi::emit_symbol_address(emitter, "x0", "_fiber_msg_switch_signal");       // x0 = stable FiberError message
    emitter.instruction("mov x1, #49");                                         // x1 = message byte length
    emitter.instruction("bl __rt_fiber_throw_state_error");                     // reject the context switch through PHP's exception path
    emitter.label("__rt_fiber_switch_signal_ok");

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
    SuspendedContext::Fiber("x10").emit_save(emitter);
    emitter.instruction("b __rt_fiber_switch_load_target");                     // skip the main-thread save path

    // -- source = main thread: persist its SP, exception chain head, and call-frame chain head into globals --
    emitter.label("__rt_fiber_switch_save_main_sp");
    abi::emit_store_reg_to_symbol(emitter, "x11", MAIN_SAVED_SP, 0);   // _fiber_main_saved_sp = SP
    SuspendedContext::Main.emit_save(emitter);

    // -- swap _fiber_current to the target and load its context --
    emitter.label("__rt_fiber_switch_load_target");
    abi::emit_store_reg_to_symbol(emitter, "x0", "_fiber_current", 0);          // _fiber_current = target fiber* (or NULL = main)
    emitter.instruction("cbz x0, __rt_fiber_switch_load_main");                 // restore main-thread state when target is NULL

    // -- target = a fiber: load its SP, exception chain head, and call-frame chain head from the object --
    SuspendedContext::Fiber("x0").emit_restore(emitter);
    elephc_memoir::stacks::emit_adopt_fiber_floor(emitter);
    emitter.instruction(&format!("ldr x11, [x0, #{}]", FIBER_SAVED_SP_OFFSET)); // x11 = target fiber's saved SP
    emitter.instruction("mov sp, x11");                                         // adopt the target fiber's stack
    emitter.instruction("b __rt_fiber_switch_restore");                         // proceed to restore callee-saved registers

    // -- target = main thread: load saved SP, exception chain head, and call-frame chain head from globals --
    emitter.label("__rt_fiber_switch_load_main");
    SuspendedContext::Main.emit_restore(emitter);
    elephc_memoir::stacks::emit_restore_main_floor(emitter, "x14");
    abi::emit_load_symbol_to_reg(emitter, "x11", MAIN_SAVED_SP, 0);    // x11 = main thread's saved SP
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


/// Returns the total bytes reserved on a freshly-created fiber stack for the entry frame.
///
/// ARM64: equal to `AARCH64_SWITCH_SAVE_BYTES` (160 bytes, 16-aligned).
/// x86_64: equal to `X86_64_INITIAL_FRAME_BYTES` (56 bytes, including the resume address slot).
///
/// Used during fiber creation to allocate the initial stack area so the first switch into the fiber
/// can restore registers without reading uninitialized memory.
pub(crate) fn fiber_initial_stack_frame_bytes(arch: Arch) -> i32 {
    match arch {
        Arch::AArch64 => AARCH64_SWITCH_SAVE_BYTES,
        Arch::X86_64 => X86_64_INITIAL_FRAME_BYTES,
    }
}

/// Returns the offset within a fiber's initial stack frame where the entry trampoline address is stored.
///
/// ARM64: offset 88 — the entry trampoline lives 88 bytes below the frame base (within the 160-byte save area,
/// at the slot previously used by the saved x30/LR, which is the resume address for a new fiber).
/// x86_64: `X86_64_INITIAL_FRAME_RIP_OFFSET` — the resume address occupies the 8-byte slot immediately
/// after the saved GPRs, at offset 48 in a 56-byte initial frame.
///
/// Used when creating a fiber to write the entry-point address into the correct slot so the first switch
/// to that fiber jumps to the fiber's trampoline.
pub(crate) fn fiber_initial_entry_offset(arch: Arch) -> i32 {
    match arch {
        Arch::AArch64 => 88,
        Arch::X86_64 => X86_64_INITIAL_FRAME_RIP_OFFSET,
    }
}

/// Emits the x86_64 SysV ABI variant of `__rt_fiber_switch`.
///
/// Saves the source context's callee-saved registers (rbx, rbp, r12–r15) and resume address to its own stack,
/// persists SP and runtime chain heads into either the source Fiber object or the main-thread globals,
/// then restores the target context's state and returns into it.
///
/// # Differences from ARM64
/// - Saves 6 GPRs (48 bytes) + 1 resume address (8 bytes) = 56-byte frame, excluding the call-return address
///   that sits above the frame (handled by `ret`).
/// - Uses a `push`/`pop` sequence rather than a contiguous store; the return address is implicit in the `ret`.
/// - Entry trampoline address is stored at `X86_64_INITIAL_FRAME_RIP_OFFSET` (48) in the initial frame.
///
/// Called from `emit_fiber_switch` when `emitter.target.arch == Arch::X86_64`.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: fiber_switch ---");
    emitter.label_global("__rt_fiber_switch");

    // -- PHP forbids cooperative context switches from inside a signal callback --
    abi::emit_load_symbol_to_reg(emitter, "r9", "__rt_pcntl_dispatching", 0);  // r9 = active signal-dispatch guard
    emitter.instruction("test r9, r9");                                         // is a PHP signal handler currently running?
    emitter.instruction("jz __rt_fiber_switch_signal_ok_x86");                  // ordinary fiber switches proceed outside handlers
    abi::emit_symbol_address(emitter, "rdi", "_fiber_msg_switch_signal");     // rdi = stable FiberError message
    emitter.instruction("mov esi, 49");                                         // rsi = message byte length
    emitter.instruction("sub rsp, 8");                                          // align the SysV stack before calling the throwing helper
    emitter.instruction("call __rt_fiber_throw_state_error");                   // reject the context switch through PHP's exception path
    emitter.instruction("add rsp, 8");                                          // restore the entry stack if the throwing helper unexpectedly returns
    emitter.label("__rt_fiber_switch_signal_ok_x86");

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
    SuspendedContext::Fiber("r10").emit_save(emitter);
    emitter.instruction("jmp __rt_fiber_switch_load_target");                   // skip the main-thread save path

    // -- source = main thread: persist its SP and exception/call-frame chain heads --
    emitter.label("__rt_fiber_switch_save_main_sp");
    abi::emit_store_reg_to_symbol(emitter, "rsp", MAIN_SAVED_SP, 0);   // _fiber_main_saved_sp = source resume SP
    SuspendedContext::Main.emit_save(emitter);

    // -- swap _fiber_current to the target and load its context --
    emitter.label("__rt_fiber_switch_load_target");
    abi::emit_store_reg_to_symbol(emitter, "rdi", "_fiber_current", 0);         // _fiber_current = target fiber* (or NULL = main)
    emitter.instruction("test rdi, rdi");                                       // is the target the main thread?
    emitter.instruction("jz __rt_fiber_switch_load_main");                      // restore main-thread state when target is NULL

    // -- target = a fiber: load its SP and exception/call-frame chain heads --
    SuspendedContext::Fiber("rdi").emit_restore(emitter);
    elephc_memoir::stacks::emit_adopt_fiber_floor(emitter);
    emitter.instruction(&format!("mov rsp, QWORD PTR [rdi + {}]", FIBER_SAVED_SP_OFFSET)); // adopt the target fiber's saved stack pointer
    emitter.instruction("jmp __rt_fiber_switch_restore");                       // proceed to restore callee-saved registers

    // -- target = main thread: load saved SP and exception/call-frame chain heads --
    emitter.label("__rt_fiber_switch_load_main");
    SuspendedContext::Main.emit_restore(emitter);
    elephc_memoir::stacks::emit_restore_main_floor(emitter, "rax");
    abi::emit_load_symbol_to_reg(emitter, "rsp", MAIN_SAVED_SP, 0);    // adopt the main thread's saved stack pointer

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;
    use elephc_memoir::coroutine_storage::{FIBER_OWN_GC_RELEASE_SUPPRESSED_OFFSET, FIBER_OWN_MAGIC_SET_GUARD_OFFSET};

    /// Each switch preserves deep-release protection without moving the global collector lock.
    #[test]
    fn switches_gc_release_protection_on_every_supported_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            super::super::emit_fiber_construct(&mut emitter);
            emit_fiber_switch(&mut emitter);
            let asm = emitter.output();
            let offset = FIBER_OWN_GC_RELEASE_SUPPRESSED_OFFSET;
            assert_eq!(super::super::FIBER_OBJECT_SIZE, offset + 8);
            let (save, load) = if target.arch == Arch::AArch64 {
                assert!(asm.contains(&format!("str xzr, [x21, #{offset}]")), "{name}: fresh context");
                (format!("str x14, [x10, #{offset}]"), format!("ldr x14, [x0, #{offset}]"))
            } else {
                assert!(asm.contains(&format!("mov r11, {}", (super::super::FIBER_OBJECT_SIZE - 8) / 8)),
                    "{name}: initialize the entire native coroutine payload");
                (format!("mov QWORD PTR [r10 + {offset}], r11"), format!("mov r11, QWORD PTR [rdi + {offset}]"))
            };
            assert!(asm.contains(&save) && asm.contains(&load), "{name}: preserve coroutine-local protection");
            let save_main = asm.split_once("__rt_fiber_switch_save_main_sp:").unwrap().1
                .split_once("__rt_fiber_switch_load_target:").unwrap().0;
            let load_main = asm.split_once("__rt_fiber_switch_load_main:").unwrap().1
                .split_once("__rt_fiber_switch_restore:").unwrap().0;
            for body in [save_main, load_main] {
                assert!(body.contains("_gc_release_suppressed")
                    && body.contains("_fiber_main_saved_gc_release_suppressed"), "{name}: main context");
            }
            assert!(!asm.contains("_gc_collecting"), "{name}: collector reentry remains globally guarded");
        }
    }

    #[test]
    /// Verifies both architecture emitters save and restore coroutine magic-setter guard chains.
    fn switches_magic_set_guard_heads_on_both_emitters() {
        for name in ["macos-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_fiber_switch(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("_magic_set_guard_head"), "{name}");
            assert!(asm.contains("_fiber_main_saved_magic_set_guard"), "{name}");
            assert!(
                asm.contains(&FIBER_OWN_MAGIC_SET_GUARD_OFFSET.to_string()),
                "{name}"
            );
        }
    }
}
