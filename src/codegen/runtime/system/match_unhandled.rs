//! Purpose:
//! Emits the `__rt_match_unhandled` runtime helper assembly for match unhandled.
//! Keeps PHP builtin semantics, libc/syscall boundaries, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::system`.
//!
//! Key details:
//! - System helpers must preserve PHP-visible behavior while crossing libc, syscall, JSON, regex, and date formatter boundaries.

use crate::codegen::{abi, emit::Emitter, platform::Arch};

/// Emits the `__rt_match_unhandled` runtime helper for both AArch64 and x86_64.
///
/// This fatal handler writes a hardcoded error message to stderr and terminates the
/// process with exit code 70 (EX_SOFTWARE). It is invoked by generated code when a
/// match expression has no corresponding arm for a given discriminant value.
///
/// AArch64 path: uses syscall 4 (sys_write) then syscall 1 (sys_exit).
/// x86_64 path: uses syscall 1 (write) then syscall 60 (exit).
pub fn emit_match_unhandled(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: match_unhandled ---");
    emitter.label_global("__rt_match_unhandled");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.adrp("x1", "_match_unhandled_msg");                          // load the unhandled-match error message page for the AArch64 fatal path
            emitter.add_lo12("x1", "x1", "_match_unhandled_msg");               // resolve the exact unhandled-match error message address for the AArch64 fatal path
            emitter.instruction("mov x2, #34");                                 // byte length of the unhandled-match error message
            emitter.instruction("mov x0, #2");                                  // write diagnostics to stderr on the AArch64 fatal path
            emitter.syscall(4);
            emitter.instruction("mov x0, #70");                                 // use EX_SOFTWARE as the process exit status on the AArch64 fatal path
            emitter.syscall(1);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "rsi", "_match_unhandled_msg");   // materialize the unhandled-match error message address for the x86_64 fatal path
            emitter.instruction("mov edx, 34");                                 // byte length of the unhandled-match error message
            emitter.instruction("mov edi, 2");                                  // write diagnostics to stderr on the x86_64 fatal path
            emitter.instruction("mov eax, 1");                                  // Linux x86_64 syscall number 1 = write
            emitter.instruction("syscall");                                     // emit the unhandled-match fatal diagnostic on x86_64
            emitter.instruction("mov edi, 70");                                 // use EX_SOFTWARE as the process exit status on the x86_64 fatal path
            emitter.instruction("mov eax, 60");                                 // Linux x86_64 syscall number 60 = exit
            emitter.instruction("syscall");                                     // terminate the process after reporting the unhandled match case
        }
    }
}
