//! Purpose:
//! Emits the runtime half of the call-stack overflow guard: `__rt_stack_limit_init`
//! measures the running stack and publishes the low-water address every compiled
//! function prologue compares against, and `__rt_stack_overflow` is the controlled
//! fatal reached when a prologue finds the stack pointer below that address.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::system`.
//!
//! Key details:
//! - `_stack_limit` is the *current* execution context's floor. It is zero-initialized,
//!   and a zero value disables the guard, so any program that never reaches the
//!   initializer behaves exactly as it did before the guard existed.
//! - `_stack_limit_main` remembers the OS-thread floor so `__rt_fiber_switch` can restore
//!   it when control leaves a coroutine stack; fiber stacks get their own floor derived
//!   from the fiber's mmap base.
//! - `__rt_stack_overflow` never returns normally and never needs a valid frame: prologues
//!   reach it with a plain branch, and cdylibs escape through the active host boundary.

use crate::codegen_support::runtime::data::STACK_OVERFLOW_MSG;
use crate::codegen_support::{
    abi,
    emit::Emitter,
    platform::{Arch, Platform},
};

/// Symbol holding the low-water stack address for the currently running context.
/// Zero means "guard disabled"; every prologue check is an unsigned compare against it.
pub(crate) const STACK_LIMIT_SYMBOL: &str = "_stack_limit";

/// Symbol holding the OS-thread (non-fiber) stack floor, used to restore
/// `_stack_limit` when a fiber switch returns to the main context.
pub(crate) const STACK_LIMIT_MAIN_SYMBOL: &str = "_stack_limit_main";

/// Bytes kept in reserve below the published limit.
///
/// The prologue check runs *after* the frame has been reserved, so this margin only has to
/// cover what a single guarded frame can still consume before the next guarded call:
/// outgoing stack-argument areas, `__rt_*` helper frames, and the libc calls those make.
/// 32 KiB is far above any of those, costs 0.4% of a default 8 MiB OS stack, and stays at
/// one eighth of the 256 KiB coroutine stack a Fiber or Generator body runs on — the one
/// place where an over-generous reserve would visibly cut the usable recursion depth.
pub(crate) const STACK_GUARD_RESERVE_BYTES: i64 = 32 * 1024;

/// `RLIMIT_STACK` resource number. Identical (3) on Linux and macOS.
const RLIMIT_STACK: i64 = 3;

/// Upper clamp on the measured stack budget.
///
/// `getrlimit` reports `RLIM_INFINITY` for an unlimited stack (`0xFFFF_FFFF_FFFF_FFFF` on
/// Linux, `0x7FFF_FFFF_FFFF_FFFF` on macOS). Clamping keeps the computed floor a real
/// address instead of wrapping below zero; the cost is that a genuinely unlimited stack
/// reports the fatal after 64 MiB instead of running until the OS refuses to grow it.
const STACK_BUDGET_CAP_BYTES: i64 = 64 * 1024 * 1024;

/// Budget used when `getrlimit` fails outright (8 MiB — the default on both platforms).
const STACK_BUDGET_FALLBACK_BYTES: i64 = 8 * 1024 * 1024;

/// Smallest budget worth guarding. Below this the reserve would swallow the whole stack,
/// so the guard is disabled instead of publishing a floor that is effectively at the
/// current stack pointer.
const STACK_BUDGET_MIN_BYTES: i64 = 256 * 1024;

/// x64 TEB offset of the low address of the thread's reserved stack mapping.
const WINDOWS_X64_TEB_DEALLOCATION_STACK_OFFSET: i32 = 0x1478;

/// Emits `__rt_stack_limit_init`, which measures the running OS stack once at process
/// start and publishes the resulting floor into `_stack_limit` and `_stack_limit_main`.
///
/// Takes no arguments and returns nothing. Clobbers the caller-saved registers a plain
/// `getrlimit(RLIMIT_STACK, &rlimit)` call clobbers, so callers must invoke it before any
/// live argument register matters (the `main` prologue calls it after argc/argv have been
/// stored to globals).
///
/// The published floor is `entry_sp - min(rlim_cur, 64 MiB) + 64 KiB`. When `getrlimit`
/// fails, the reported limit is implausibly small, or the subtraction would wrap, zero is
/// published instead and the guard stays inert for the whole process.
pub fn emit_stack_limit_init(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stack_limit_init (publish the call-stack floor) ---");
    emitter.label_global("__rt_stack_limit_init");
    match (emitter.target.platform, emitter.target.arch) {
        (Platform::Windows, Arch::X86_64) => emit_stack_limit_init_windows_x86_64(emitter),
        (_, Arch::AArch64) => emit_stack_limit_init_aarch64(emitter),
        (_, Arch::X86_64) => emit_stack_limit_init_x86_64(emitter),
    }
}

/// Windows x86_64 implementation using the current thread's TEB stack reservation.
///
/// Windows has no `getrlimit`. `DeallocationStack` is the low address of the reserved
/// mapping and remains stable while guard pages commit downward; `StackLimit` is used only
/// as a defensive fallback. Publishing that low address plus the ordinary helper reserve
/// gives compiled prologues the same controlled-fatal boundary as Unix.
fn emit_stack_limit_init_windows_x86_64(emitter: &mut Emitter) {
    emitter.instruction(&format!("mov rax, QWORD PTR gs:[{}]", WINDOWS_X64_TEB_DEALLOCATION_STACK_OFFSET)); // load the stable low address of the thread stack reservation
    emitter.instruction("test rax, rax");                                       // is DeallocationStack available for this thread?
    emitter.instruction("jnz __rt_stack_limit_init_windows_base_ready");        // use the reservation base when present
    emitter.instruction("mov rax, QWORD PTR gs:[16]");                          // otherwise fall back to NT_TIB.StackLimit
    emitter.label("__rt_stack_limit_init_windows_base_ready");
    emitter.instruction(&format!("add rax, {}", STACK_GUARD_RESERVE_BYTES));    // retain helper/fatal-path headroom above the reservation base
    emitter.instruction("mov rcx, QWORD PTR gs:[8]");                           // rcx = NT_TIB.StackBase (high address)
    emitter.instruction("cmp rax, rcx");                                        // did a malformed TEB value cross the stack top?
    emitter.instruction("jb __rt_stack_limit_init_windows_store");              // a real low-water address is safe to publish
    emitter.instruction("xor eax, eax");                                        // zero disables the guard on an unusable TEB tuple
    emitter.label("__rt_stack_limit_init_windows_store");
    abi::emit_store_reg_to_symbol(emitter, "rax", STACK_LIMIT_SYMBOL, 0);
    abi::emit_store_reg_to_symbol(emitter, "rax", STACK_LIMIT_MAIN_SYMBOL, 0);
    emitter.instruction("ret");                                                 // no native call or helper frame was needed
}

/// AArch64 implementation of `__rt_stack_limit_init` (macOS and Linux share it).
///
/// Reserves a 32-byte frame: the low 16 bytes are the `struct rlimit` output buffer and
/// the high 16 bytes hold the saved x29/x30 pair. x29 doubles as the "stack top" reference
/// because it is callee-saved and therefore survives the `getrlimit` call unchanged.
fn emit_stack_limit_init_aarch64(emitter: &mut Emitter) {
    emitter.instruction("sub sp, sp, #32");                                     // reserve the rlimit output buffer plus this helper's frame footer
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save the caller frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // anchor the frame pointer and remember it as the stack-top reference

    // -- getrlimit(RLIMIT_STACK, &rlimit) --
    emitter.instruction(&format!("mov x0, #{}", RLIMIT_STACK));                 // resource = RLIMIT_STACK
    emitter.instruction("mov x1, sp");                                          // destination = the 16-byte rlimit buffer at the bottom of this frame
    emitter.bl_c("getrlimit");                                                  // x0 = 0 on success, -1 on failure

    // -- pick the budget: rlim_cur on success, the platform default otherwise --
    abi::emit_load_int_immediate(emitter, "x1", STACK_BUDGET_FALLBACK_BYTES);
    emitter.instruction("cbnz x0, __rt_stack_limit_init_clamp");                // keep the fallback budget when getrlimit reported a failure
    emitter.instruction("ldr x1, [sp]");                                        // x1 = rlim_cur, the soft stack limit in bytes

    // -- clamp the budget into a range that yields a real address --
    emitter.label("__rt_stack_limit_init_clamp");
    abi::emit_load_int_immediate(emitter, "x2", STACK_BUDGET_CAP_BYTES);
    emitter.instruction("cmp x1, x2");                                          // is the reported budget above the cap (or RLIM_INFINITY)?
    emitter.instruction("csel x1, x1, x2, lo");                                 // x1 = min(budget, cap) using an unsigned comparison
    abi::emit_load_int_immediate(emitter, "x2", STACK_BUDGET_MIN_BYTES);
    emitter.instruction("cmp x1, x2");                                          // is the budget too small to be worth guarding?
    emitter.instruction("b.lo __rt_stack_limit_init_disable");                  // yes — leave the guard disabled rather than publish a bogus floor

    // -- floor = entry stack pointer - (budget - reserve) --
    abi::emit_load_int_immediate(emitter, "x2", STACK_GUARD_RESERVE_BYTES);
    emitter.instruction("sub x1, x1, x2");                                      // subtract the reserve kept for helper frames and the fatal path
    emitter.instruction("sub x0, x29, x1");                                     // x0 = the lowest stack address compiled prologues may reach
    emitter.instruction("cmp x0, x29");                                         // did the subtraction wrap below address zero?
    emitter.instruction("b.hs __rt_stack_limit_init_disable");                  // yes — an unusable floor, so disable the guard instead
    emitter.instruction("b __rt_stack_limit_init_store");                       // publish the computed floor

    // -- disabled: publish zero so every prologue compare passes --
    emitter.label("__rt_stack_limit_init_disable");
    emitter.instruction("mov x0, #0");                                          // zero disables the guard for the rest of the process

    emitter.label("__rt_stack_limit_init_store");
    abi::emit_store_reg_to_symbol(emitter, "x0", STACK_LIMIT_SYMBOL, 0);
    abi::emit_store_reg_to_symbol(emitter, "x0", STACK_LIMIT_MAIN_SYMBOL, 0);
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore the caller frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the rlimit buffer and frame footer
    emitter.instruction("ret");                                                 // return to the process entry prologue
}

/// x86_64 (Linux) implementation of `__rt_stack_limit_init`.
///
/// Mirrors the AArch64 path with SysV registers. `rbp` is the stack-top reference because
/// it is callee-saved and therefore preserved across the `getrlimit` call; the 16 bytes
/// below it are the `struct rlimit` output buffer, which also keeps `rsp` 16-byte aligned
/// at the call site.
fn emit_stack_limit_init_x86_64(emitter: &mut Emitter) {
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // anchor the frame pointer and remember it as the stack-top reference
    emitter.instruction("sub rsp, 16");                                         // reserve the 16-byte rlimit output buffer, keeping rsp 16-byte aligned

    // -- getrlimit(RLIMIT_STACK, &rlimit) --
    emitter.instruction(&format!("mov edi, {}", RLIMIT_STACK));                 // resource = RLIMIT_STACK
    emitter.instruction("mov rsi, rsp");                                        // destination = the 16-byte rlimit buffer below the frame pointer
    emitter.bl_c("getrlimit");                                                  // eax = 0 on success, -1 on failure

    // -- pick the budget: rlim_cur on success, the platform default otherwise --
    emitter.instruction(&format!("mov rcx, {}", STACK_BUDGET_FALLBACK_BYTES));  // preload the fallback budget without disturbing the result flags
    emitter.instruction("test eax, eax");                                       // did getrlimit succeed?
    emitter.instruction("jnz __rt_stack_limit_init_clamp");                     // keep the fallback budget when getrlimit reported a failure
    emitter.instruction("mov rcx, QWORD PTR [rsp]");                            // rcx = rlim_cur, the soft stack limit in bytes

    // -- clamp the budget into a range that yields a real address --
    emitter.label("__rt_stack_limit_init_clamp");
    emitter.instruction(&format!("mov rdx, {}", STACK_BUDGET_CAP_BYTES));       // materialize the budget cap for the clamp comparison
    emitter.instruction("cmp rcx, rdx");                                        // is the reported budget above the cap (or RLIM_INFINITY)?
    emitter.instruction("cmova rcx, rdx");                                      // rcx = min(budget, cap) using an unsigned comparison
    emitter.instruction(&format!("mov rdx, {}", STACK_BUDGET_MIN_BYTES));       // materialize the smallest budget worth guarding
    emitter.instruction("cmp rcx, rdx");                                        // is the budget too small to be worth guarding?
    emitter.instruction("jb __rt_stack_limit_init_disable");                    // yes — leave the guard disabled rather than publish a bogus floor

    // -- floor = entry stack pointer - (budget - reserve) --
    emitter.instruction(&format!("sub rcx, {}", STACK_GUARD_RESERVE_BYTES));    // subtract the reserve kept for helper frames and the fatal path
    emitter.instruction("mov rax, rbp");                                        // start from the remembered stack-top reference
    emitter.instruction("sub rax, rcx");                                        // rax = the lowest stack address compiled prologues may reach
    emitter.instruction("cmp rax, rbp");                                        // did the subtraction wrap below address zero?
    emitter.instruction("jae __rt_stack_limit_init_disable");                   // yes — an unusable floor, so disable the guard instead
    emitter.instruction("jmp __rt_stack_limit_init_store");                     // publish the computed floor

    // -- disabled: publish zero so every prologue compare passes --
    emitter.label("__rt_stack_limit_init_disable");
    emitter.instruction("xor eax, eax");                                        // zero disables the guard for the rest of the process

    emitter.label("__rt_stack_limit_init_store");
    abi::emit_store_reg_to_symbol(emitter, "rax", STACK_LIMIT_SYMBOL, 0);
    abi::emit_store_reg_to_symbol(emitter, "rax", STACK_LIMIT_MAIN_SYMBOL, 0);
    emitter.instruction("mov rsp, rbp");                                        // release the rlimit output buffer
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the process entry prologue
}

/// Emits `__rt_stack_overflow`, the controlled fatal for call-stack exhaustion.
///
/// Reached by a plain branch (never a call) from a function prologue that found the stack
/// pointer below `_stack_limit`, so it must not assume a usable frame and never returns.
/// Writes PHP's stack-overflow wording to stderr, then escapes an active cdylib boundary or
/// exits a standalone process with status 255, the status PHP uses for an uncaught fatal.
pub fn emit_stack_overflow(emitter: &mut Emitter) {
    let msg_len = STACK_OVERFLOW_MSG.len();
    emitter.blank();
    emitter.comment("--- runtime: stack_overflow (controlled call-depth fatal) ---");
    emitter.label_global("__rt_stack_overflow");
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x1", "_stack_err_msg");
            emitter.instruction(&format!("mov x2, #{}", msg_len));              // byte length of the call-stack overflow message
            emitter.instruction("mov x0, #2");                                  // write the diagnostic to stderr
            emitter.syscall(4);
            abi::emit_cdylib_exit_escape(emitter);
            emitter.instruction("mov x0, #255");                                // exit status 255, matching PHP's uncaught fatal error
            emitter.syscall(1);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "rsi", "_stack_err_msg");
            emitter.instruction(&format!("mov edx, {}", msg_len));              // byte length of the call-stack overflow message
            emitter.instruction("mov edi, 2");                                  // write the diagnostic to stderr
            emitter.instruction("mov eax, 1");                                  // Linux x86_64 syscall number 1 = write
            emitter.instruction("syscall");                                     // emit the call-stack overflow diagnostic
            abi::emit_cdylib_exit_escape(emitter);
            emitter.instruction("mov edi, 255");                                // exit status 255, matching PHP's uncaught fatal error
            emitter.instruction("mov eax, 60");                                 // Linux x86_64 syscall number 60 = exit
            emitter.instruction("syscall");                                     // terminate the process after reporting the overflow
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Every supported target must emit both guard helpers, and the initializer must reach
    /// `getrlimit` through the platform's C-symbol spelling rather than a hardcoded name.
    #[test]
    fn test_stack_guard_helpers_emit_for_every_supported_target() {
        for (target, getrlimit_call) in [
            (Target::new(Platform::MacOS, Arch::AArch64), "bl _getrlimit"),
            (Target::new(Platform::Linux, Arch::AArch64), "bl getrlimit"),
            (Target::new(Platform::Linux, Arch::X86_64), "call getrlimit"),
        ] {
            let mut emitter = Emitter::new(target);
            emit_stack_limit_init(&mut emitter);
            emit_stack_overflow(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("__rt_stack_limit_init:"), "{target:?}: {asm}");
            assert!(asm.contains("__rt_stack_overflow:"), "{target:?}: {asm}");
            assert!(asm.contains(getrlimit_call), "{target:?}: {asm}");
            assert!(asm.contains("_stack_limit"), "{target:?}: {asm}");
            assert!(asm.contains("_stack_limit_main"), "{target:?}: {asm}");
            assert!(asm.contains("_stack_err_msg"), "{target:?}: {asm}");
        }
    }

    /// Windows derives its stack reservation from the TEB and never names Unix `getrlimit`.
    #[test]
    fn test_windows_stack_guard_uses_teb_reservation_bounds() {
        let target = Target::new(Platform::Windows, Arch::X86_64);
        let mut emitter = Emitter::new(target);
        emit_stack_limit_init(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("gs:[5240]"), "missing TEB DeallocationStack load: {asm}");
        assert!(asm.contains("gs:[8]"), "missing TEB StackBase load: {asm}");
        assert!(!asm.contains("getrlimit"), "Windows must not link getrlimit: {asm}");
        assert!(asm.contains("_stack_limit"), "{asm}");
        assert!(asm.contains("_stack_limit_main"), "{asm}");
    }

    /// Cdylib stack exhaustion must unwind through the active host boundary while
    /// retaining the standalone exit sequence as the inactive-boundary fallback.
    #[test]
    fn test_stack_overflow_escapes_active_cdylib_boundary() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new_cdylib(target);
            emit_stack_overflow(&mut emitter);
            let asm = emitter.output();
            assert!(
                asm.contains(crate::codegen_support::cdylib::BOUNDARY_ACTIVE),
                "{target:?}: {asm}"
            );
            assert!(
                asm.contains(crate::codegen_support::cdylib::BOUNDARY_STATUS),
                "{target:?}: {asm}"
            );
            assert!(asm.contains("__rt_throw_current"), "{target:?}: {asm}");
            assert!(
                asm.contains(if target.arch == Arch::X86_64 {
                    "mov edi, 255"
                } else {
                    "mov x0, #255"
                }),
                "{target:?}: {asm}"
            );
        }
    }

    /// The fatal must exit with 255, the status PHP reports for an uncaught fatal error,
    /// and must write exactly as many bytes as the message actually has.
    #[test]
    fn test_stack_overflow_reports_php_exit_status_and_exact_length() {
        let len = STACK_OVERFLOW_MSG.len();
        let mut arm = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_stack_overflow(&mut arm);
        let arm = arm.output();
        assert!(arm.contains(&format!("mov x2, #{len}")), "{arm}");
        assert!(arm.contains("mov x0, #255"), "{arm}");

        let mut x86 = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_stack_overflow(&mut x86);
        let x86 = x86.output();
        assert!(x86.contains(&format!("mov edx, {len}")), "{x86}");
        assert!(x86.contains("mov edi, 255"), "{x86}");
    }

    /// The reserve has to stay well below the default fiber stack, or the floor published
    /// on a coroutine stack would sit above its initial stack pointer and every generator
    /// body would trip the guard on its first frame.
    #[test]
    fn test_reserve_leaves_usable_room_on_a_default_fiber_stack() {
        let fiber_usable =
            i64::from(crate::codegen_support::runtime::fibers::FIBER_DEFAULT_STACK_SIZE);
        assert!(
            STACK_GUARD_RESERVE_BYTES * 8 <= fiber_usable,
            "reserve {STACK_GUARD_RESERVE_BYTES} is too large for a {fiber_usable}-byte fiber stack"
        );
        assert!(STACK_BUDGET_MIN_BYTES > STACK_GUARD_RESERVE_BYTES);
        assert!(STACK_BUDGET_FALLBACK_BYTES <= STACK_BUDGET_CAP_BYTES);
    }
}
