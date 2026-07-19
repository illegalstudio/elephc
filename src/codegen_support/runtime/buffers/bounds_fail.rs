//! Purpose:
//! Emits the `__rt_buffer_bounds_fail` runtime helper assembly for buffer bounds fatal diagnostics.
//! Keeps compiler buffer extension checks and fatal paths aligned with generated pointer operations.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::buffers`.
//!
//! Key details:
//! - Buffer helpers enforce extension ownership rules, including live headers, bounds checks, and fatal paths before unsafe access.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the `__rt_buffer_bounds_fail` runtime helper for the current target.
/// Writes a fixed 40-byte buffer-bounds error message to stderr and terminates
/// the process with exit code 70 (EX_SOFTWARE).
pub fn emit_buffer_bounds_fail(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_buffer_bounds_fail_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: buffer_bounds_fail ---");
    emitter.label_global("__rt_buffer_bounds_fail");
    abi::emit_symbol_address(emitter, "x1", "_buffer_bounds_msg");              // load the error message page
    emitter.instruction("mov x2, #40");                                         // byte length of the fixed buffer bounds error message
    emitter.instruction("mov x0, #2");                                          // write diagnostics to stderr
    emitter.syscall(4);
    emitter.instruction("mov x0, #70");                                         // use EX_SOFTWARE as the process exit status
    emitter.syscall(1);
}

/// Emits the Linux x86_64 variant of `__rt_buffer_bounds_fail`.
/// Uses syscall 1 (write) to emit the error message to stderr, then syscall 231 (`exit_group`)
/// to terminate with exit code 70.
fn emit_buffer_bounds_fail_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: buffer_bounds_fail ---");
    emitter.label_global("__rt_buffer_bounds_fail");
    emitter.instruction("mov edi, 2");                                          // write diagnostics to the Linux stderr file descriptor
    abi::emit_symbol_address(emitter, "rsi", "_buffer_bounds_msg");
    emitter.instruction("mov edx, 40");                                         // byte length of the fixed buffer-bounds error message
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall 1 = write
    emitter.instruction("syscall");                                             // emit the fatal buffer-bounds diagnostic to stderr
    emitter.instruction("mov edi, 70");                                         // use EX_SOFTWARE as the process exit status for consistency with the ARM runtime
    emitter.instruction("mov eax, 231");                                        // Linux x86_64 syscall 231 = exit_group
    emitter.instruction("syscall");                                             // terminate the process immediately after the fatal buffer-bounds diagnostic
}
