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
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

pub(crate) use elephc_memoir::stacks::STACK_LIMIT_SYMBOL;

#[cfg(test)]
use elephc_memoir::stacks::STACK_GUARD_RESERVE_BYTES;

/// Measures the process stack using the target operating system's resource selector.
pub fn emit_stack_limit_init(emitter: &mut Emitter) {
    const RLIMIT_STACK: i64 = 3;
    elephc_memoir::stacks::emit_stack_limit_init(emitter, RLIMIT_STACK);
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
    if emitter.phpext_output {
        // A PHP host receives the recoverable error after native cleanup, without a raw fatal line.
        abi::emit_cdylib_exit_escape(emitter);
    }
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
    }
}
