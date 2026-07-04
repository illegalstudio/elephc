//! Purpose:
//! Emits the `__rt_exit` runtime helper: the single indirection every PHP
//! `exit()`/`die()` travels through. It decides at runtime between terminating
//! the process (CLI, worker boot, `--web-worker` handler mode) and unwinding to
//! the current `--web`/`--web-worker=script` request boundary (ending the request
//! while keeping the prefork worker alive).
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::system`.
//! - `crate::codegen_ir::lower_inst::builtins::system::lower_exit` emits `call __rt_exit`.
//!
//! Key details:
//! - The status code is passed in the int result register (`x0`/`rax`), matching
//!   how `lower_exit` already materializes it. The helper never returns.
//! - The bailout path longjmps into `_exit_jmp_buf`, a channel SEPARATE from the
//!   exception handler chain (`_exc_handler_top`), so `exit()` is uncatchable by
//!   user `catch (\Throwable)` and skips `finally` — exactly like PHP.
//! - `_exit_boundary_active` is 1 only while a request boundary is installed
//!   (see `emit_web_exit_boundary`); it is 0 in CLI builds, during worker boot,
//!   and in `--web-worker` handler mode, so the helper terminates the process
//!   there just as `exit()` did before.

use crate::codegen::abi;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the `__rt_exit` runtime helper for the current target.
///
/// Input: the process/exit status in the int result register (`x0` / `rax`).
/// The helper never returns: it either terminates the process with that status
/// or longjmps into the active request boundary's `_exit_jmp_buf` (return value
/// 1), landing at the handler's bailout epilogue which flushes the response and
/// returns to the Rust worker loop.
///
/// Dispatches to `emit_rt_exit_x86_64` on x86_64; the AArch64 body covers both
/// `macos-aarch64` and `linux-aarch64`.
pub fn emit_rt_exit(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_rt_exit_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: exit (PHP exit()/die() dispatch) ---");
    emitter.label_global("__rt_exit");

    // -- decide: process exit vs. request-boundary bailout (status is in x0) --
    abi::emit_load_symbol_to_reg(emitter, "x10", "_exit_boundary_active", 0);    // x10 = 1 if a --web/script request boundary is installed
    emitter.instruction("cbz x10, __rt_exit_process");                          // no boundary → terminate the process with the status in x0

    // -- request boundary active: release nested frames, then longjmp to the handler --
    // Uses _exit_jmp_buf, a channel separate from _exc_handler_top, so no user
    // catch/finally can intercept it (PHP: exit is not an exception). The status
    // is dropped: an HTTP request has no observable process exit code. Cleanup runs
    // FIRST, while every PHP frame is still live, so the callbacks can read/free their
    // locals before the longjmp discards the stack.
    abi::emit_load_symbol_to_reg(emitter, "x0", "_exit_survivor_frame", 0);      // x0 = survivor = request-entry frame top: release every frame above it
    emitter.instruction("bl __rt_exception_cleanup_frames");                     // free owned locals of every PHP frame exit()/die() unwinds through
    abi::emit_symbol_address(emitter, "x0", "_exit_jmp_buf");                    // x0 = &_exit_jmp_buf (the request boundary's setjmp buffer)
    emitter.instruction("mov x1, #1");                                          // longjmp return value = 1 → handler's setjmp branches to the bailout landing
    emitter.bl_c("longjmp");                                                     // unwind to the request boundary; never returns here

    // -- no boundary (CLI / worker boot / handler mode): real process exit --
    emitter.label("__rt_exit_process");
    emitter.syscall(1);                                                         // exit(status); status already sits in x0
}

/// Emits the x86_64 Linux variant of the `__rt_exit` runtime helper.
///
/// Identical logic to the AArch64 path using System V registers: the status
/// arrives in `rax`, the boundary flag is tested in `r10`, and the bailout path
/// aligns the stack before `call longjmp`. The process path performs the Linux
/// `exit` syscall (`rax=60`, `rdi=status`).
fn emit_rt_exit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: exit (PHP exit()/die() dispatch) ---");
    emitter.label_global("__rt_exit");

    // -- decide: process exit vs. request-boundary bailout (status is in rax) --
    abi::emit_load_symbol_to_reg(emitter, "r10", "_exit_boundary_active", 0);    // r10 = 1 if a --web/script request boundary is installed
    emitter.instruction("test r10, r10");                                       // is a request boundary currently installed?
    emitter.instruction("jz __rt_exit_process");                                // no boundary → terminate the process with the status in rax

    // -- request boundary active: release nested frames, then longjmp to the handler --
    // The single `push rbp` 16-aligns rsp for BOTH C calls below (cleanup + longjmp);
    // the helper never returns, so it is never popped. Cleanup runs FIRST, while every
    // PHP frame is still live, so the callbacks can free their locals before longjmp.
    emitter.instruction("push rbp");                                            // align rsp to 16 for the C calls (the helper never returns, so it is never popped)
    abi::emit_load_symbol_to_reg(emitter, "rdi", "_exit_survivor_frame", 0);     // rdi = survivor = request-entry frame top: release every frame above it
    emitter.instruction("call __rt_exception_cleanup_frames");                  // free owned locals of every PHP frame exit()/die() unwinds through
    abi::emit_symbol_address(emitter, "rdi", "_exit_jmp_buf");                   // rdi = &_exit_jmp_buf (arg0: the request boundary's setjmp buffer)
    emitter.instruction("mov esi, 1");                                          // rsi = 1 (arg1: longjmp return value → handler branches to the bailout landing)
    emitter.bl_c("longjmp");                                                     // unwind to the request boundary; never returns here

    // -- no boundary (CLI / worker boot / handler mode): real process exit --
    emitter.label("__rt_exit_process");
    emitter.instruction("mov rdi, rax");                                        // exit code = status (SysV first argument register)
    emitter.instruction("mov eax, 60");                                         // Linux x86_64 syscall 60 = exit
    emitter.instruction("syscall");                                             // terminate the process with the requested status
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::{Platform, Target};

    /// Renders the `__rt_exit` helper for one target.
    fn render(platform: Platform, arch: Arch) -> String {
        let mut emitter = Emitter::new(Target::new(platform, arch));
        emit_rt_exit(&mut emitter);
        emitter.output()
    }

    /// Verifies the helper exports its global label and both dispatch arms on
    /// every supported target: the boundary-flag test, the longjmp bailout, and
    /// the process-exit fallback.
    #[test]
    fn emits_both_dispatch_arms_for_all_targets() {
        for (platform, arch) in [
            (Platform::MacOS, Arch::AArch64),
            (Platform::Linux, Arch::AArch64),
            (Platform::Linux, Arch::X86_64),
        ] {
            let asm = render(platform, arch);
            assert!(
                asm.contains(".globl __rt_exit\n"),
                "missing global label for {:?}/{:?}",
                platform,
                arch
            );
            assert!(
                asm.contains("_exit_boundary_active"),
                "missing boundary-flag load for {:?}/{:?}",
                platform,
                arch
            );
            assert!(
                asm.contains("_exit_jmp_buf"),
                "missing longjmp bailout for {:?}/{:?}",
                platform,
                arch
            );
            assert!(
                asm.contains("__rt_exit_process"),
                "missing process-exit fallback label for {:?}/{:?}",
                platform,
                arch
            );
        }
    }

    /// Verifies the bailout path calls libc `longjmp` with the platform C-ABI
    /// mangling (`_longjmp` on macOS, `longjmp` on Linux) so the reference
    /// matches the same symbol the exception unwinder already links.
    #[test]
    fn bailout_calls_longjmp_with_platform_mangling() {
        assert!(render(Platform::MacOS, Arch::AArch64).contains("bl _longjmp"));
        assert!(render(Platform::Linux, Arch::AArch64).contains("bl longjmp"));
        assert!(render(Platform::Linux, Arch::X86_64).contains("call longjmp"));
    }

    /// Verifies the process-exit fallback performs the platform exit syscall
    /// (macOS trap `svc #0x80` with `x16`, Linux x86_64 syscall number 60).
    #[test]
    fn process_path_performs_the_exit_syscall() {
        let mac = render(Platform::MacOS, Arch::AArch64);
        assert!(mac.contains("mov x16, #1"));
        assert!(mac.contains("svc #0x80"));
        let linux_x86 = render(Platform::Linux, Arch::X86_64);
        assert!(linux_x86.contains("mov eax, 60"));
        assert!(linux_x86.contains("syscall"));
    }
}
