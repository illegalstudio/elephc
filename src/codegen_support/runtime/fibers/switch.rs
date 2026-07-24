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
//!
//! Windows x86_64 state preserved across switches:
//!   * General-purpose: rbx, rbp, rsi, rdi, r12–r15
//!   * Floating-point:  xmm6–xmm15
//!   * Resume address:  normal call return address above the aligned save area
//! Total = 8 GPRs + 168-byte aligned XMM area + 1 return address = 240 bytes.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::{Arch, Platform, Target};

use super::{
    alloc::FIBER_GUARD_PAGE_SIZE,
    FIBER_OWN_CALL_FRAME_OFFSET, FIBER_OWN_EXC_HEAD_OFFSET, FIBER_SAVED_SP_OFFSET,
    FIBER_STACK_BASE_OFFSET, FIBER_STACK_TOP_OFFSET,
};

/// Total bytes saved on the stack by an AArch64 context switch (must stay 16-aligned).
const AARCH64_SWITCH_SAVE_BYTES: i32 = 160;

/// Total bytes pushed by a Linux x86_64 context switch, excluding the call return address.
const LINUX_X86_64_SWITCH_SAVE_BYTES: i32 = 48;

/// Total bytes present in a fresh Linux x86_64 fiber frame, including the resume address.
const LINUX_X86_64_INITIAL_FRAME_BYTES: i32 = LINUX_X86_64_SWITCH_SAVE_BYTES + 8;

/// Offset within the Linux x86_64 initial frame where the resume address lives.
const LINUX_X86_64_INITIAL_FRAME_RIP_OFFSET: i32 = LINUX_X86_64_SWITCH_SAVE_BYTES;

/// Total bytes pushed for the eight MS x64 callee-saved general-purpose registers.
const WINDOWS_X86_64_GPR_SAVE_BYTES: i32 = 64;

/// Total bytes occupied by the ten MS x64 callee-saved XMM registers.
const WINDOWS_X86_64_XMM_SAVE_BYTES: i32 = 160;

/// Stack reservation for the Windows XMM save area, including the alignment pad.
const WINDOWS_X86_64_XMM_STACK_RESERVE_BYTES: i32 = WINDOWS_X86_64_XMM_SAVE_BYTES + 8;

/// Total bytes present in a fresh Windows x64 fiber frame, including the resume address.
const WINDOWS_X86_64_INITIAL_FRAME_BYTES: i32 =
    WINDOWS_X86_64_GPR_SAVE_BYTES + WINDOWS_X86_64_XMM_STACK_RESERVE_BYTES + 8;

/// Offset within the Windows x64 initial frame where the resume address lives.
const WINDOWS_X86_64_INITIAL_FRAME_RIP_OFFSET: i32 =
    WINDOWS_X86_64_GPR_SAVE_BYTES + WINDOWS_X86_64_XMM_STACK_RESERVE_BYTES;

/// Windows x64 TEB offset of the allocation base for the active stack.
const WINDOWS_X64_TEB_DEALLOCATION_STACK_OFFSET: i32 = 0x1478;

/// Emits `__rt_fiber_switch`: a cooperative context switch between Fiber execution contexts.
///
/// # Input
/// - `x0`: target `Fiber*` — NULL switches back to the main thread, non-NULL switches to that fiber.
/// - Callee-saved registers and floating-point state of the *source* context are saved to the source's stack.
/// - Global `_fiber_current`, `_exc_handler_top`, and `_exc_call_frame_top` are updated to track the source's
///   suspended state so exception unwinding and stack-frame cleanup remain correct across switches.
///
/// # Output
/// - Control does not return from this function normally. When the current context is later switched back to,
///   execution resumes immediately after the `ret` instruction with all state fully restored.
///
/// # Behavior
/// - When `x0` is NULL: the source's SP, exception chain, and call-frame chain are saved to the main-thread
///   globals (`_fiber_main_saved_sp`, `_fiber_main_saved_exc`, `_fiber_main_saved_call_frame`), then the
///   main thread's saved context is restored and execution resumes on the main stack.
/// - When `x0` is non-NULL: the source's state is persisted into the source `Fiber` object at offsets
///   `FIBER_SAVED_SP_OFFSET`, `FIBER_OWN_EXC_HEAD_OFFSET`, and `FIBER_OWN_CALL_FRAME_OFFSET`; the target
///   fiber's saved state is loaded and restored, adopting the target's stack.
///
/// # ABI Notes
/// - ARM64: saves x19–x28, x29, x30, d8–d15 (160 bytes, 16-aligned) to the source stack, then restores the
///   same register set from the target's stack before returning.
/// - x86_64: uses the matched helper `emit_x86_64` which saves/restores rbx, rbp, r12–r15 per the SysV ABI.
///   On the Windows x86_64 target, `emit_x86_64` additionally resyncs the TEB stack metadata
///   (`StackBase`, `StackLimit`, and `DeallocationStack`) to whichever stack is about to run,
///   snapshotting/restoring the main thread's values through `_fiber_main_saved_*`; other targets
///   are unaffected. `NT_TIB::FiberData` deliberately remains owned by Windows because elephc's
///   manual stacks are not Win32 `CreateFiber` objects.
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

/// Returns the total bytes reserved on a freshly-created fiber stack for the entry frame.
///
/// ARM64: equal to `AARCH64_SWITCH_SAVE_BYTES` (160 bytes, 16-aligned).
/// Linux x86_64: 56 bytes (six SysV callee-saved GPRs plus the resume address).
/// Windows x86_64: 240 bytes (eight MS x64 GPRs, ten 16-byte XMM values, alignment, and RIP).
///
/// Used during fiber creation to allocate the initial stack area so the first switch into the fiber
/// can restore registers without reading uninitialized memory.
pub(crate) fn fiber_initial_stack_frame_bytes(target: Target) -> i32 {
    match (target.platform, target.arch) {
        (_, Arch::AArch64) => AARCH64_SWITCH_SAVE_BYTES,
        (Platform::Windows, Arch::X86_64) => WINDOWS_X86_64_INITIAL_FRAME_BYTES,
        (_, Arch::X86_64) => LINUX_X86_64_INITIAL_FRAME_BYTES,
    }
}

/// Returns the offset within a fiber's initial stack frame where the entry trampoline address is stored.
///
/// ARM64: offset 88 — the entry trampoline lives 88 bytes below the frame base (within the 160-byte save area,
/// at the slot previously used by the saved x30/LR, which is the resume address for a new fiber).
/// Linux x86_64: the resume address follows six saved GPRs at offset 48.
/// Windows x86_64: it follows eight saved GPRs and the aligned XMM6–XMM15 area at offset 232.
///
/// Used when creating a fiber to write the entry-point address into the correct slot so the first switch
/// to that fiber jumps to the fiber's trampoline.
pub(crate) fn fiber_initial_entry_offset(target: Target) -> i32 {
    match (target.platform, target.arch) {
        (_, Arch::AArch64) => 88,
        (Platform::Windows, Arch::X86_64) => WINDOWS_X86_64_INITIAL_FRAME_RIP_OFFSET,
        (_, Arch::X86_64) => LINUX_X86_64_INITIAL_FRAME_RIP_OFFSET,
    }
}

/// Emits the x86_64 SysV ABI variant of `__rt_fiber_switch`.
///
/// Saves the source context's callee-saved registers and resume address to its own stack,
/// persists SP and exception/call-frame chain heads into either the source Fiber object or the main-thread
/// globals, then restores the target context's state and returns into it.
///
/// # Differences from ARM64
/// - Linux saves 6 SysV GPRs (48 bytes) + 1 resume address (8 bytes) = a 56-byte frame.
/// - Windows additionally saves rsi/rdi and xmm6–xmm15 in a 16-byte-aligned area: 240 bytes total.
/// - Uses a `push`/`pop` sequence rather than a contiguous store; the return address is implicit in the `ret`.
/// - The entry trampoline offset is target-aware so the fresh frame matches the restore order.
///
/// # Windows (PE32+) TEB resync
/// - On the Windows x86_64 target only, the switch additionally resyncs `NT_TIB::StackBase`
///   (`gs:[0x08]`), `NT_TIB::StackLimit` (`gs:[0x10]`), and `TEB::DeallocationStack`
///   (`gs:[0x1478]`) to whichever stack is about to run. The main thread's values are snapshotted
///   into `_fiber_main_saved_*` globals when leaving it and restored when switching back.
/// - `NT_TIB::FiberData` (`gs:[0x20]`) is intentionally not changed. elephc switches private
///   runtime stacks manually and does not construct the opaque object expected by the Win32 Fiber
///   and FLS APIs. Keeping the OS value untouched preserves an enclosing Win32 fiber identity and
///   avoids falsely advertising an elephc `Fiber*` as a Win32 fiber control block.
/// - Non-Windows targets are unaffected.
///
/// Called from `emit_fiber_switch` when `emitter.target.arch == Arch::X86_64`.
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

    if emitter.target.platform == Platform::Windows {
        emitter.instruction("push rsi");                                        // preserve MS x64's fifth callee-saved general-purpose register
        emitter.instruction("push rdi");                                        // preserve MS x64's sixth callee-saved general-purpose register
        emitter.instruction("sub rsp, 168");                                    // reserve aligned XMM6–XMM15 state plus the 8-byte alignment pad
        emitter.instruction("movaps XMMWORD PTR [rsp], xmm6");                  // preserve MS x64 callee-saved XMM6 in its aligned context slot
        emitter.instruction("movaps XMMWORD PTR [rsp + 16], xmm7");             // preserve MS x64 callee-saved XMM7 in its aligned context slot
        emitter.instruction("movaps XMMWORD PTR [rsp + 32], xmm8");             // preserve MS x64 callee-saved XMM8 in its aligned context slot
        emitter.instruction("movaps XMMWORD PTR [rsp + 48], xmm9");             // preserve MS x64 callee-saved XMM9 in its aligned context slot
        emitter.instruction("movaps XMMWORD PTR [rsp + 64], xmm10");            // preserve MS x64 callee-saved XMM10 in its aligned context slot
        emitter.instruction("movaps XMMWORD PTR [rsp + 80], xmm11");            // preserve MS x64 callee-saved XMM11 in its aligned context slot
        emitter.instruction("movaps XMMWORD PTR [rsp + 96], xmm12");            // preserve MS x64 callee-saved XMM12 in its aligned context slot
        emitter.instruction("movaps XMMWORD PTR [rsp + 112], xmm13");           // preserve MS x64 callee-saved XMM13 in its aligned context slot
        emitter.instruction("movaps XMMWORD PTR [rsp + 128], xmm14");           // preserve MS x64 callee-saved XMM14 in its aligned context slot
        emitter.instruction("movaps XMMWORD PTR [rsp + 144], xmm15");           // preserve MS x64 callee-saved XMM15 in its aligned context slot
    }

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

    // -- Windows: snapshot the main thread's TEB stack metadata so a later switch
    //    back to main can restore it. FiberData stays untouched because this is a
    //    manual stack switch, not a Win32 CreateFiber/SwitchToFiber transition. --
    if emitter.target.platform == Platform::Windows {
        emitter.instruction("mov r11, QWORD PTR gs:[8]");                       // r11 = TEB StackBase (main stack high address)
        abi::emit_store_reg_to_symbol(emitter, "r11", "_fiber_main_saved_stack_base", 0); // remember main's StackBase across the fiber run
        emitter.instruction("mov r11, QWORD PTR gs:[16]");                      // r11 = TEB StackLimit (main stack low address)
        abi::emit_store_reg_to_symbol(emitter, "r11", "_fiber_main_saved_stack_limit", 0); // remember main's StackLimit across the fiber run
        emitter.instruction(&format!(                                           // load the main stack reservation base from the TEB
            "mov r11, QWORD PTR gs:[{}]",
            WINDOWS_X64_TEB_DEALLOCATION_STACK_OFFSET
        ));
        abi::emit_store_reg_to_symbol(
            emitter,
            "r11",
            "_fiber_main_saved_deallocation_stack",
            0,
        );                                                                      // remember main's stack allocation base across the fiber run
    }

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

    // -- Windows: point all TEB stack metadata at the target fiber. StackLimit is
    //    the first usable byte above the guard; DeallocationStack is the mapping base. --
    if emitter.target.platform == Platform::Windows {
        emitter.instruction(&format!("mov r11, QWORD PTR [rdi + {}]", FIBER_STACK_TOP_OFFSET)); // r11 = fiber stack_top (high address)
        emitter.instruction("mov QWORD PTR gs:[8], r11");                       // TEB StackBase = fiber stack_top
        emitter.instruction(&format!("mov r11, QWORD PTR [rdi + {}]", FIBER_STACK_BASE_OFFSET)); // r11 = fiber stack_base (low address)
        emitter.instruction(&format!("add r11, {}", FIBER_GUARD_PAGE_SIZE));    // advance past the protected guard region
        emitter.instruction("mov QWORD PTR gs:[16], r11");                      // TEB StackLimit = first usable stack byte
        emitter.instruction(&format!("mov r11, QWORD PTR [rdi + {}]", FIBER_STACK_BASE_OFFSET)); // reload the fiber mapping base
        emitter.instruction(&format!(                                           // store the fiber mapping base in the TEB deallocation slot
            "mov QWORD PTR gs:[{}], r11",
            WINDOWS_X64_TEB_DEALLOCATION_STACK_OFFSET
        ));
    }
    emitter.instruction(&format!("mov rsp, QWORD PTR [rdi + {}]", FIBER_SAVED_SP_OFFSET)); // adopt the target fiber's saved stack pointer
    emitter.instruction("jmp __rt_fiber_switch_restore");                       // proceed to restore callee-saved registers

    // -- target = main thread: load saved SP and exception/call-frame chain heads --
    emitter.label("__rt_fiber_switch_load_main");
    abi::emit_load_symbol_to_reg(emitter, "r11", "_fiber_main_saved_exc", 0);   // r11 = main thread's saved handler chain head
    abi::emit_store_reg_to_symbol(emitter, "r11", "_exc_handler_top", 0);       // restore the main thread handler chain head globally
    abi::emit_load_symbol_to_reg(emitter, "r11", "_fiber_main_saved_call_frame", 0); // r11 = main thread's saved cleanup chain head
    abi::emit_store_reg_to_symbol(emitter, "r11", "_exc_call_frame_top", 0);    // restore the main thread cleanup chain head globally

    // -- Windows: restore the main thread's complete saved TEB stack metadata. --
    if emitter.target.platform == Platform::Windows {
        abi::emit_load_symbol_to_reg(emitter, "r11", "_fiber_main_saved_stack_base", 0); // r11 = main's saved StackBase
        emitter.instruction("mov QWORD PTR gs:[8], r11");                       // TEB StackBase = main stack high address
        abi::emit_load_symbol_to_reg(emitter, "r11", "_fiber_main_saved_stack_limit", 0); // r11 = main's saved StackLimit
        emitter.instruction("mov QWORD PTR gs:[16], r11");                      // TEB StackLimit = main stack low address
        abi::emit_load_symbol_to_reg(
            emitter,
            "r11",
            "_fiber_main_saved_deallocation_stack",
            0,
        );                                                                      // r11 = main's saved stack allocation base
        emitter.instruction(&format!(                                           // restore the main stack reservation base in the TEB
            "mov QWORD PTR gs:[{}], r11",
            WINDOWS_X64_TEB_DEALLOCATION_STACK_OFFSET
        ));
    }
    abi::emit_load_symbol_to_reg(emitter, "rsp", "_fiber_main_saved_sp", 0);    // adopt the main thread's saved stack pointer

    // -- restore callee-saved state from the target stack and return into it --
    emitter.label("__rt_fiber_switch_restore");
    if emitter.target.platform == Platform::Windows {
        emitter.instruction("movaps xmm6, XMMWORD PTR [rsp]");                  // restore MS x64 callee-saved XMM6 from the aligned context slot
        emitter.instruction("movaps xmm7, XMMWORD PTR [rsp + 16]");             // restore MS x64 callee-saved XMM7 from the aligned context slot
        emitter.instruction("movaps xmm8, XMMWORD PTR [rsp + 32]");             // restore MS x64 callee-saved XMM8 from the aligned context slot
        emitter.instruction("movaps xmm9, XMMWORD PTR [rsp + 48]");             // restore MS x64 callee-saved XMM9 from the aligned context slot
        emitter.instruction("movaps xmm10, XMMWORD PTR [rsp + 64]");            // restore MS x64 callee-saved XMM10 from the aligned context slot
        emitter.instruction("movaps xmm11, XMMWORD PTR [rsp + 80]");            // restore MS x64 callee-saved XMM11 from the aligned context slot
        emitter.instruction("movaps xmm12, XMMWORD PTR [rsp + 96]");            // restore MS x64 callee-saved XMM12 from the aligned context slot
        emitter.instruction("movaps xmm13, XMMWORD PTR [rsp + 112]");           // restore MS x64 callee-saved XMM13 from the aligned context slot
        emitter.instruction("movaps xmm14, XMMWORD PTR [rsp + 128]");           // restore MS x64 callee-saved XMM14 from the aligned context slot
        emitter.instruction("movaps xmm15, XMMWORD PTR [rsp + 144]");           // restore MS x64 callee-saved XMM15 from the aligned context slot
        emitter.instruction("add rsp, 168");                                    // release the aligned XMM save area and alignment pad
        emitter.instruction("pop rdi");                                         // restore MS x64 callee-saved rdi
        emitter.instruction("pop rsi");                                         // restore MS x64 callee-saved rsi
    }
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
    use crate::codegen_support::platform::{Arch, Platform, Target};

    use super::*;

    /// Verifies that the Windows x86_64 fiber switch resyncs every TEB stack field,
    /// keeps the guard page below StackLimit, and leaves Win32 FiberData untouched.
    #[test]
    fn test_x86_64_windows_switch_resyncs_complete_teb_stack_metadata() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_fiber_switch(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("mov QWORD PTR gs:[8], r11"));
        assert!(asm.contains("mov QWORD PTR gs:[16], r11"));
        assert!(asm.contains("mov QWORD PTR gs:[5240], r11"));
        assert!(asm.contains("_fiber_main_saved_stack_base"));
        assert!(asm.contains("_fiber_main_saved_stack_limit"));
        assert!(asm.contains("_fiber_main_saved_deallocation_stack"));
        assert!(asm.contains("mov r11, QWORD PTR gs:[8]"));
        assert!(asm.contains("mov r11, QWORD PTR gs:[5240]"));
        assert!(asm.contains("add r11, 16384"));
        assert!(!asm.contains("gs:[32]"));
    }

    /// Verifies the Windows switch saves and restores every MS x64 nonvolatile register.
    #[test]
    fn test_x86_64_windows_switch_preserves_full_ms_x64_context() {
        let mut emitter = Emitter::new(Target::new(Platform::Windows, Arch::X86_64));
        emit_fiber_switch(&mut emitter);
        let asm = emitter.output();

        let save_area = asm.find("sub rsp, 168").expect("missing aligned XMM save area");
        let restore_area = asm.find("add rsp, 168").expect("missing aligned XMM restore area");
        assert!(save_area < asm.find("movaps XMMWORD PTR [rsp], xmm6").unwrap());
        assert!(restore_area > asm.find("movaps xmm15, XMMWORD PTR [rsp + 144]").unwrap());
        assert!(asm.contains("push rsi"));
        assert!(asm.contains("push rdi"));
        assert!(asm.contains("pop rdi"));
        assert!(asm.contains("pop rsi"));

        for (offset, register) in [
            (0, "xmm6"),
            (16, "xmm7"),
            (32, "xmm8"),
            (48, "xmm9"),
            (64, "xmm10"),
            (80, "xmm11"),
            (96, "xmm12"),
            (112, "xmm13"),
            (128, "xmm14"),
            (144, "xmm15"),
        ] {
            let slot = if offset == 0 {
                "[rsp]".to_owned()
            } else {
                format!("[rsp + {offset}]")
            };
            assert!(asm.contains(&format!("movaps XMMWORD PTR {slot}, {register}")));
            assert!(asm.contains(&format!("movaps {register}, XMMWORD PTR {slot}")));
        }
    }

    /// Verifies each target's fresh fiber frame matches its context-switch restore sequence.
    #[test]
    fn test_fiber_initial_frame_layout_is_target_aware() {
        let windows = Target::new(Platform::Windows, Arch::X86_64);
        let linux = Target::new(Platform::Linux, Arch::X86_64);
        let arm64 = Target::new(Platform::MacOS, Arch::AArch64);

        assert_eq!(fiber_initial_stack_frame_bytes(windows), 240);
        assert_eq!(fiber_initial_entry_offset(windows), 232);
        assert_eq!(WINDOWS_X86_64_XMM_STACK_RESERVE_BYTES % 16, 8);
        assert_eq!(WINDOWS_X86_64_INITIAL_FRAME_BYTES % 16, 0);
        assert_eq!(WINDOWS_X86_64_INITIAL_FRAME_RIP_OFFSET % 16, 8);
        assert_eq!(fiber_initial_stack_frame_bytes(linux), 56);
        assert_eq!(fiber_initial_entry_offset(linux), 48);
        assert_eq!(fiber_initial_stack_frame_bytes(arm64), 160);
        assert_eq!(fiber_initial_entry_offset(arm64), 88);
    }

    /// Verifies that the Linux x86_64 fiber switch is byte-identical to the SysV-only
    /// switch: no `gs:` segment override and no TEB resync symbols anywhere in the output.
    #[test]
    fn test_x86_64_linux_switch_has_no_teb_resync() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_fiber_switch(&mut emitter);
        let asm = emitter.output();
        assert!(!asm.contains("gs:"));
        assert!(!asm.contains("_fiber_main_saved_stack"));
        assert!(!asm.contains("push rsi"));
        assert!(!asm.contains("push rdi"));
        assert!(!asm.contains("movaps XMMWORD PTR"));
        assert!(!asm.contains("sub rsp, 168"));
    }

    /// Verifies that the AArch64 fiber switch is unaffected by the Windows TIB resync:
    /// no `gs:` segment override and no TEB resync symbols anywhere in the output.
    #[test]
    fn test_aarch64_switch_has_no_teb_resync() {
        let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_fiber_switch(&mut emitter);
        let asm = emitter.output();
        assert!(!asm.contains("gs:"));
        assert!(!asm.contains("_fiber_main_saved_stack"));
    }
}
