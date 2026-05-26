//! Purpose:
//! Emits the `__rt_enum_from_fail` runtime helper assembly for enum from fail.
//! Keeps PHP builtin semantics, libc/syscall boundaries, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::system`.
//!
//! Key details:
//! - System helpers must preserve PHP-visible behavior while crossing libc, syscall, JSON, regex, and date formatter boundaries.

use crate::codegen::{abi, emit::Emitter, platform::Arch};

/// Emits the `__rt_enum_from_fail` runtime helper.
///
/// Writes a 33-byte diagnostic message to stderr (fd 2) then terminates the
/// process with exit status 70 (`EX_SOFTWARE`). Used when a `From` impl for an
/// enum fails — preserving PHP's fatal-throw semantics at the libc boundary.
///
/// - AArch64: uses `syscall 4` (write) then `syscall 1` (exit)
/// - X86_64: uses `syscall 1` (write) then `syscall 60` (exit)
///
/// # Arguments
/// * `emitter` — target-aware assembly emitter
pub fn emit_enum_from_fail(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: enum_from_fail ---");
    emitter.label_global("__rt_enum_from_fail");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.adrp("x1", "_enum_from_msg");                    // load the enum-from error message page
            emitter.add_lo12("x1", "x1", "_enum_from_msg");         // resolve the enum-from error message address
            emitter.instruction("mov x2, #33");                                 // byte length of the enum-from error message
            emitter.instruction("mov x0, #2");                                  // write diagnostics to stderr
            emitter.syscall(4);
            emitter.instruction("mov x0, #70");                                 // use EX_SOFTWARE as the process exit status
            emitter.syscall(1);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "rsi", "_enum_from_msg"); // materialize the enum-from error message address for x86_64
            emitter.instruction("mov edx, 33");                                 // byte length of the enum-from error message
            emitter.instruction("mov edi, 2");                                  // write diagnostics to stderr
            emitter.instruction("mov eax, 1");                                  // Linux x86_64 syscall number 1 = write
            emitter.instruction("syscall");                                     // emit the enum-from error message
            emitter.instruction("mov edi, 70");                                 // use EX_SOFTWARE as the process exit status
            emitter.instruction("mov eax, 60");                                 // Linux x86_64 syscall number 60 = exit
            emitter.instruction("syscall");                                     // terminate the process after the fatal enum conversion failure
        }
    }
}
