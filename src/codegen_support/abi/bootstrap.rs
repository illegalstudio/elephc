//! Purpose:
//! Emits process bootstrap snippets that move OS-provided values into compiler-managed locations.
//! Provides small target-aware helpers for heap debug setup, frame copying, and process exit.
//!
//! Called from:
//! - `crate::codegen::block_emit` and top-level program prologue emission.
//!
//! Key details:
//! - Register choices must match the platform entry convention before normal PHP frame setup begins.

use crate::codegen_support::{emit::Emitter, platform::Arch};

use super::{
    emit_load_int_immediate, emit_store_reg_to_symbol, process_argc_reg, process_argv_reg,
    temp_int_reg,
};

/// Ignore SIGPIPE for the whole process, the way the PHP CLI does.
///
/// Without this, writing to a socket whose peer has closed kills the program
/// with signal 13 before any output is flushed: `fwrite()` on a half-closed
/// connection terminated the process instead of returning a byte count.
///
/// `signal(2)` is called through libc so the platform picks its own sigaction
/// shim; SIGPIPE is 13 and SIG_IGN is 1 on both supported targets.
///
/// THE CALL GOES THROUGH `bl_c`, WHICH IS PLATFORM-AWARE. Writing the mnemonic by
/// hand once emitted `bl _signal` on the whole AArch64 arm — right on macOS, where C
/// symbols carry a leading underscore, and wrong on Linux, where the symbol is
/// `signal`. Every `--web` program then failed to link with
/// `undefined reference to '_signal'`, and a program with enough objects linked in
/// resolved it elsewhere and crashed at run time instead. The arch match now decides
/// only the ARGUMENT registers; the symbol name is never spelled per-arch.
pub fn emit_ignore_sigpipe(emitter: &mut Emitter) {
    emitter.comment("ignore SIGPIPE so a closed peer cannot kill the process");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #13");                                 // SIGPIPE
            emitter.instruction("mov x1, #1");                                  // SIG_IGN
        }
        Arch::X86_64 => {
            emitter.instruction("mov edi, 13");                                 // SIGPIPE
            emitter.instruction("mov esi, 1");                                  // SIG_IGN
        }
    }
    emitter.bl_c("signal");
}

/// Store OS-provided argc and argv into global symbols.
pub fn emit_store_process_args_to_globals(emitter: &mut Emitter) {
    emit_store_reg_to_symbol(emitter, process_argc_reg(emitter.target), "_global_argc", 0);
    emit_store_reg_to_symbol(emitter, process_argv_reg(emitter.target), "_global_argv", 0);
}

/// Set the heap debug flag to 1 in global symbol storage.
pub fn emit_enable_heap_debug_flag(emitter: &mut Emitter) {
    let scratch = temp_int_reg(emitter.target);
    emit_load_int_immediate(emitter, scratch, 1);
    emit_store_reg_to_symbol(emitter, scratch, "_heap_debug_enabled", 0);
}

/// Set the web heap-guard flag to 1 in global symbol storage.
///
/// Enables the cheap small-bin double-free detection in `--web` builds without the
/// expensive per-allocation free-list validation that `--heap-debug` also turns on.
/// A detected double free routes to `__rt_heap_debug_fail`, which writes the diagnostic
/// and `_exit(1)`s the worker so the prefork master respawns it, containing the
/// corruption to a single request rather than aborting the whole server.
pub fn emit_enable_web_heap_guard_flag(emitter: &mut Emitter) {
    let scratch = temp_int_reg(emitter.target);
    emit_load_int_immediate(emitter, scratch, 1);
    emit_store_reg_to_symbol(emitter, scratch, "_web_heap_guard_enabled", 0);
}

/// Copy the current frame pointer into the destination scratch register.
#[cfg(test)]
pub fn emit_copy_frame_pointer(emitter: &mut Emitter, dest: &str) {
    emitter.instruction(&format!(
        "mov {}, {}",
        dest,
        super::registers::frame_pointer_reg(emitter)
    )); // copy the current frame pointer into the requested scratch register
}

/// Emit a process-exit sequence for the current target, then return control to the OS.
///
/// # Arguments
/// - `code`: the exit code visible to the OS; must fit in the target's exit register.
///
/// # Platform behavior
/// - **macOS ARM64**: loads `code` into `x0` and invokes syscall 1 (`sys_exit`).
/// - **Linux ARM64**: loads `code` into `x0` and invokes syscall 94 (`exit_group`).
/// - **Linux x86_64**: loads `code` into `edi` and invokes syscall 231 (`exit_group`).
/// - **macOS x86_64**: panics — not yet implemented.
///
/// This routine never returns to the calling code. The syscall consumes the current execution context.
pub fn emit_exit(emitter: &mut Emitter, code: u32) {
    match (emitter.target.platform, emitter.target.arch) {
        (super::super::platform::Platform::MacOS, Arch::AArch64)
        | (super::super::platform::Platform::Linux, Arch::AArch64) => {
            emitter.instruction("bl __rt_ob_flush_all");                        // drain still-active output buffers to stdout before terminating
            emitter.instruction(&format!("mov x0, #{}", code));                 // load the requested process exit code into the ABI return register
            emitter.syscall(1);
        }
        (super::super::platform::Platform::Linux, Arch::X86_64) => {
            emitter.instruction("and rsp, -16");                                // realign the stack for the flush call (this path never returns)
            emitter.instruction("call __rt_ob_flush_all");                      // drain still-active output buffers to stdout before terminating
            emitter.instruction(&format!("mov edi, {}", code));                 // load the requested process exit code into the SysV first-argument register
            emitter.instruction("mov eax, 231");                                // Linux x86_64 syscall 231 = exit_group
            emitter.instruction("syscall");                                     // terminate the process through the Linux x86_64 syscall ABI
        }
        (super::super::platform::Platform::MacOS, Arch::X86_64) => {
            panic!("process exit emission is not implemented yet for target macos-x86_64");
        }
        (super::super::platform::Platform::Windows, _) => {
            panic!("Windows target is not yet supported (see issue #379)");
        }
    }
}

/// Emit a process-exit sequence that uses the integer result register as the exit code.
///
/// Unlike `emit_exit`, which takes a constant, this routine exits with whatever
/// value a preceding call left in the target's integer result register (`x0` /
/// `rax`). Used by the `--web` process-entry stub to surface `elephc_web_run`'s
/// return value as the process exit code.
///
/// # Platform behavior
/// - **macOS ARM64 / Linux ARM64**: the return value already sits in `x0`; the
///   target invokes `sys_exit` on macOS or `exit_group` on Linux.
/// - **Linux x86_64**: moves `eax` (the C return value) into `edi` (the SysV exit
///   argument) and invokes syscall 231 (`exit_group`).
/// - **macOS x86_64**: panics — not in the supported target matrix.
///
/// This routine never returns to the calling code.
pub fn emit_exit_with_result_reg(emitter: &mut Emitter) {
    match (emitter.target.platform, emitter.target.arch) {
        (super::super::platform::Platform::MacOS, Arch::AArch64)
        | (super::super::platform::Platform::Linux, Arch::AArch64) => {
            emitter.instruction("mov x19, x0");                                 // stash the exit code in a callee-saved register (this path never returns)
            emitter.instruction("bl __rt_ob_flush_all");                        // drain still-active output buffers to stdout before terminating
            emitter.instruction("mov x0, x19");                                 // restore the exit code into the syscall argument register
            emitter.syscall(1);
        }
        (super::super::platform::Platform::Linux, Arch::X86_64) => {
            emitter.instruction("mov rbx, rax");                                // stash the exit code in a callee-saved register (this path never returns)
            emitter.instruction("and rsp, -16");                                // realign the stack for the flush call (this path never returns)
            emitter.instruction("call __rt_ob_flush_all");                      // drain still-active output buffers to stdout before terminating
            emitter.instruction("mov edi, ebx");                                // move the stashed return value into the SysV exit argument register
            emitter.instruction("mov eax, 231");                                // Linux x86_64 syscall 231 = exit_group
            emitter.instruction("syscall");                                     // terminate the process with the bridge return code
        }
        (super::super::platform::Platform::MacOS, Arch::X86_64) => {
            panic!("process exit emission is not implemented yet for target macos-x86_64");
        }
        (super::super::platform::Platform::Windows, _) => {
            panic!("Windows target is not yet supported (see issue #379)");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Pins the SIGPIPE bootstrap call to the C symbol each PLATFORM actually exports.
    ///
    /// macOS prefixes C symbols with an underscore and Linux does not, so the mnemonic
    /// cannot be written per-ARCH. It was: the AArch64 arm emitted `bl _signal` for both
    /// platforms, which linked on macOS and broke every Linux program —
    /// `undefined reference to '_signal'` for a `--web` build, and a run-time crash once
    /// enough objects were linked for the name to resolve to something else. The local
    /// suite is macOS-only, so only CI could see it.
    #[test]
    fn the_sigpipe_call_uses_each_platforms_c_symbol() {
        for (target, expected) in [
            (Target::new(Platform::MacOS, Arch::AArch64), "bl _signal"),
            (Target::new(Platform::Linux, Arch::AArch64), "bl signal"),
            (Target::new(Platform::Linux, Arch::X86_64), "call signal"),
        ] {
            let mut emitter = Emitter::new(target);
            emit_ignore_sigpipe(&mut emitter);
            let asm = emitter.output();
            assert!(
                asm.contains(expected),
                "{target:?} must call the C symbol as `{expected}`:\n{asm}"
            );
            assert!(
                !asm.contains("_signal\n") || expected.contains("_signal"),
                "{target:?} must not emit the macOS-mangled name:\n{asm}"
            );
        }
    }
}
