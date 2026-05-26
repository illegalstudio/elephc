//! Purpose:
//! Emits the `__rt_buffer_use_after_free` runtime helper assembly for buffer use-after-free fatal diagnostics.
//! Keeps compiler buffer extension checks and fatal paths aligned with generated pointer operations.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::buffers`.
//!
//! Key details:
//! - Buffer helpers enforce extension ownership rules, including live headers, bounds checks, and fatal paths before unsafe access.

use crate::codegen::abi;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits the `__rt_buffer_use_after_free` runtime helper for buffer use-after-free diagnostics.
/// Dispatches to the platform-specific variant based on `emitter.target.arch`.
/// Terminates the process with exit code 70 after emitting the error message.
pub fn emit_buffer_use_after_free(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_buffer_use_after_free_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: buffer_use_after_free ---");
    emitter.label_global("__rt_buffer_use_after_free");
    emitter.adrp("x1", "_buffer_uaf_msg");                       // load the error message page
    emitter.add_lo12("x1", "x1", "_buffer_uaf_msg");                 // resolve the use-after-free message address
    emitter.instruction("mov x2, #47");                                         // byte length of the use-after-free error message
    emitter.instruction("mov x0, #2");                                          // write diagnostics to stderr
    emitter.syscall(4);
    emitter.instruction("mov x0, #70");                                         // use EX_SOFTWARE as the process exit status
    emitter.syscall(1);
}

/// Emits the Linux x86_64 variant of `__rt_buffer_use_after_free`.
/// Uses Linux syscall 1 (`write`) to emit the error message to stderr (fd=2), then
/// syscall 60 (`exit`) to terminate with exit code 70 (EX_SOFTWARE), matching the ARM64 runtime.
fn emit_buffer_use_after_free_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: buffer_use_after_free ---");
    emitter.label_global("__rt_buffer_use_after_free");
    emitter.instruction("mov edi, 2");                                          // write diagnostics to the Linux stderr file descriptor
    abi::emit_symbol_address(emitter, "rsi", "_buffer_uaf_msg");
    emitter.instruction("mov edx, 47");                                         // byte length of the fixed buffer use-after-free error message
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall 1 = write
    emitter.instruction("syscall");                                             // emit the fatal buffer use-after-free diagnostic to stderr
    emitter.instruction("mov edi, 70");                                         // use EX_SOFTWARE as the process exit status for consistency with the ARM runtime
    emitter.instruction("mov eax, 60");                                         // Linux x86_64 syscall 60 = exit
    emitter.instruction("syscall");                                             // terminate the process immediately after the fatal buffer use-after-free diagnostic
}
