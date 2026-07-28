//! Purpose:
//! Emits process bootstrap snippets that move OS-provided values into compiler-managed locations.
//! Provides small target-aware helpers for heap debug setup, frame copying, and process exit.
//!
//! Called from:
//! - `crate::codegen::block_emit` and top-level program prologue emission.
//!
//! Key details:
//! - Register choices must match the platform entry convention before normal PHP frame setup begins.

use crate::codegen_support::{emit::Emitter, platform::{Arch, Platform}};

use super::{
    emit_load_int_immediate, emit_store_reg_to_symbol, process_argc_reg, process_argv_reg,
    temp_int_reg,
};

/// Store OS-provided argc and argv into global symbols.
///
/// On macOS-AArch64, Linux-AArch64, and Linux-x86_64 the process entry
/// convention places argc/argv in the arch's first/second integer argument
/// registers (`x0`/`x1`, `rdi`/`rsi`), which are stored directly into
/// `_global_argc`/`_global_argv` byte-for-byte as before.
///
/// On Windows-x86_64 the MinGW CRT `main` wrapper receives `argc` as a 32-bit
/// `int` in `ecx` whose upper 32 bits are not defined by MSx64; spilling full
/// `rcx`/`rdx` can leave a garbage `_global_argc` that makes `__rt_build_argv`
/// write to NULL. Instead this path emits a single `call __rt_sys_init_argv`,
/// a Unicode Win32 shim that parses `GetCommandLineW` through
/// `CommandLineToArgvW` and stores strict UTF-8 arguments in the globals. The AArch64 and
/// Linux-x86_64 paths remain byte-identical to the original direct stores.
pub fn emit_store_process_args_to_globals(emitter: &mut Emitter) {
    if (emitter.target.platform, emitter.target.arch) == (Platform::Windows, Arch::X86_64) {
        emitter.instruction("call __rt_sys_init_argv");                         // populate globals from native Unicode Windows argv
        return;
    }
    emit_store_reg_to_symbol(emitter, process_argc_reg(emitter.target), "_global_argc", 0);
    emit_store_reg_to_symbol(emitter, process_argv_reg(emitter.target), "_global_argv", 0);
}

/// Set the heap debug flag to 1 in global symbol storage.
pub fn emit_enable_heap_debug_flag(emitter: &mut Emitter) {
    let scratch = temp_int_reg(emitter.target);
    emit_load_int_immediate(emitter, scratch, 1);
    emit_store_reg_to_symbol(emitter, scratch, "_heap_debug_enabled", 0);
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
/// - **macOS ARM64 / Linux ARM64**: loads `code` into `x0` and invokes syscall 1 (`sys_exit`).
/// - **Linux x86_64**: loads `code` into `edi` (SysV first-argument register) and invokes syscall 60 (`exit`).
/// - **Windows x86_64**: loads `code` into `edi` and calls the `__rt_sys_exit` shim
///   (`ExitProcess`), which reads `rdi`. Terminating here — identical to an explicit
///   `exit(code)` — instead of returning through the MinGW CRT is deliberate: the CRT's
///   `exit` reaches the same `rdi`-consuming shim, so a return path that left `rdi`
///   holding leftover data would exit with a garbage code.
/// - **macOS x86_64**: panics — not yet implemented.
///
/// This routine never returns to the calling code. The exit consumes the current execution context.
pub fn emit_exit(emitter: &mut Emitter, code: u32) {
    match (emitter.target.platform, emitter.target.arch) {
        (Platform::MacOS, Arch::AArch64) | (Platform::Linux, Arch::AArch64) => {
            emitter.instruction("bl __rt_ob_flush_all");                        // drain still-active output buffers to stdout before terminating
            emitter.instruction(&format!("mov x0, #{}", code));                 // load the requested process exit code into the ABI return register
            emitter.syscall(1);
        }
        (Platform::Linux, Arch::X86_64) => {
            emitter.instruction("and rsp, -16");                                // realign the stack for the flush call (this path never returns)
            emitter.instruction("call __rt_ob_flush_all");                      // drain still-active output buffers to stdout before terminating
            emitter.instruction(&format!("mov edi, {}", code));                 // load the requested process exit code into the SysV first-argument register
            emitter.instruction("mov eax, 60");                                 // Linux x86_64 syscall 60 = exit
            emitter.instruction("syscall");                                     // terminate the process through the Linux x86_64 syscall ABI
        }
        (Platform::MacOS, Arch::X86_64) => {
            panic!("process exit emission is not implemented yet for target macos-x86_64");
        }
        (Platform::Windows, Arch::X86_64) => {
            emitter.instruction(&format!("mov ebx, {}", code));                 // preserve the requested exit code across the output-buffer flush
            emitter.instruction("and rsp, -16");                                // realign the stack for runtime calls on this terminal path
            emitter.instruction("call __rt_ob_flush_all");                      // drain still-active output buffers before terminating
            emitter.instruction("mov rdi, rbx");                                // load the runtime shim's SysV-style integer argument register
            emitter.instruction("call __rt_sys_exit");                          // terminate via the Win32 ExitProcess shim, which reads rdi (never returns)
        }
        (Platform::Windows, Arch::AArch64) => {
            panic!("Windows ARM64 target is not yet supported (see issue #379)");
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
/// - **macOS ARM64 / Linux ARM64**: the return value already sits in `x0`, which
///   is `sys_exit`'s argument register, so it invokes syscall 1 directly.
/// - **Linux x86_64**: moves `eax` (the C return value) into `edi` (the SysV exit
///   argument) and invokes syscall 60 (`exit`).
/// - **Windows x86_64**: moves `eax` into `edi` and calls the `__rt_sys_exit` shim
///   (`ExitProcess`), which reads `rdi` — terminating directly rather than returning
///   through the MinGW CRT, for the same `rdi`-consuming reason as `emit_exit`.
/// - **macOS x86_64**: panics — not in the supported target matrix.
///
/// This routine never returns to the calling code.
pub fn emit_exit_with_result_reg(emitter: &mut Emitter) {
    match (emitter.target.platform, emitter.target.arch) {
        (Platform::MacOS, Arch::AArch64) | (Platform::Linux, Arch::AArch64) => {
            emitter.instruction("mov x19, x0");                                 // stash the exit code in a callee-saved register (this path never returns)
            emitter.instruction("bl __rt_ob_flush_all");                        // drain still-active output buffers to stdout before terminating
            emitter.instruction("mov x0, x19");                                 // restore the exit code into the syscall argument register
            emitter.syscall(1);
        }
        (Platform::Linux, Arch::X86_64) => {
            emitter.instruction("mov rbx, rax");                                // stash the exit code in a callee-saved register (this path never returns)
            emitter.instruction("and rsp, -16");                                // realign the stack for the flush call (this path never returns)
            emitter.instruction("call __rt_ob_flush_all");                      // drain still-active output buffers to stdout before terminating
            emitter.instruction("mov edi, ebx");                                // move the stashed return value into the SysV exit argument register
            emitter.instruction("mov eax, 60");                                 // Linux x86_64 syscall 60 = exit
            emitter.instruction("syscall");                                     // terminate the process with the bridge return code
        }
        (Platform::MacOS, Arch::X86_64) => {
            panic!("process exit emission is not implemented yet for target macos-x86_64");
        }
        (Platform::Windows, Arch::X86_64) => {
            emitter.instruction("mov rbx, rax");                                // preserve the bridge return value across the output-buffer flush
            emitter.instruction("and rsp, -16");                                // realign the stack for runtime calls on this terminal path
            emitter.instruction("call __rt_ob_flush_all");                      // drain still-active output buffers before terminating
            emitter.instruction("mov rdi, rbx");                                // load the runtime shim's SysV-style integer argument register
            emitter.instruction("call __rt_sys_exit");                          // terminate via the Win32 ExitProcess shim, which reads rdi (never returns)
        }
        (Platform::Windows, Arch::AArch64) => {
            panic!("Windows ARM64 target is not yet supported (see issue #379)");
        }
    }
}
