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
/// - **macOS x86_64**: panics — not yet implemented.
///
/// This routine never returns to the calling code. The syscall consumes the current execution context.
pub fn emit_exit(emitter: &mut Emitter, code: u32) {
    match (emitter.target.platform, emitter.target.arch) {
        (super::super::platform::Platform::MacOS, Arch::AArch64)
        | (super::super::platform::Platform::Linux, Arch::AArch64) => {
            emitter.instruction(&format!("mov x0, #{}", code)); // load the requested process exit code into the ABI return register
            emitter.syscall(1);
        }
        (super::super::platform::Platform::Linux, Arch::X86_64) => {
            emitter.instruction(&format!("mov edi, {}", code)); // load the requested process exit code into the SysV first-argument register
            emitter.instruction("mov eax, 60"); // Linux x86_64 syscall 60 = exit
            emitter.instruction("syscall"); // terminate the process through the Linux x86_64 syscall ABI
        }
        (super::super::platform::Platform::MacOS, Arch::X86_64) => {
            panic!("process exit emission is not implemented yet for target macos-x86_64");
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
/// - **macOS x86_64**: panics — not in the supported target matrix.
///
/// This routine never returns to the calling code.
pub fn emit_exit_with_result_reg(emitter: &mut Emitter) {
    match (emitter.target.platform, emitter.target.arch) {
        (super::super::platform::Platform::MacOS, Arch::AArch64)
        | (super::super::platform::Platform::Linux, Arch::AArch64) => {
            emitter.syscall(1);
        }
        (super::super::platform::Platform::Linux, Arch::X86_64) => {
            emitter.instruction("mov edi, eax"); // move the C return value into the SysV exit argument register
            emitter.instruction("mov eax, 60"); // Linux x86_64 syscall 60 = exit
            emitter.instruction("syscall"); // terminate the process with the bridge return code
        }
        (super::super::platform::Platform::MacOS, Arch::X86_64) => {
            panic!("process exit emission is not implemented yet for target macos-x86_64");
        }
    }
}
