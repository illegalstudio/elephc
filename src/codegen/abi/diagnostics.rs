//! Purpose:
//! Emits inline fatal-diagnostic-and-exit sequences shared across emitters.
//! Centralizes the target-specific write(stderr)+exit syscalls so callers do not duplicate them.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::arithmetic` (integer `/` and `%` by zero) and
//!   `crate::codegen_ir::lower_inst::floats` (float `/` by zero), plus any other EIR emitter
//!   that must abort with a message.
//!
//! Key details:
//! - Writes the message to file descriptor 2 (stderr) and terminates with exit code 1, matching
//!   elephc's existing builtin fatals (e.g. intdiv by zero). The error is NOT catchable by PHP
//!   try/catch — elephc has no runtime-error exception mechanism yet.

use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

/// Emits a write of `message` to stderr (fd 2) followed by `exit(1)`, for the current target.
///
/// The message bytes are interned in the data section. Control never returns from this sequence
/// (the process exits), so callers branch here for unrecoverable conditions such as division by
/// zero. The same sequence is emitted on every supported target; only the syscall mechanics differ.
///
/// # Arguments
/// * `emitter` - assembly emitter (provides target arch and instruction emission)
/// * `data` - data section used to intern the message string
/// * `message` - the bytes written to stderr (include a trailing newline if desired)
pub fn emit_fatal_to_stderr(emitter: &mut Emitter, data: &mut DataSection, message: &[u8]) {
    let (err_label, err_len) = data.add_string(message);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #2");                                  // fd = stderr
            emitter.adrp("x1", &err_label);                                     // load the page that contains the fatal message
            emitter.add_lo12("x1", "x1", &err_label);                           // resolve the fatal message address within that page
            emitter.instruction(&format!("mov x2, #{}", err_len));              // pass the fatal message length to write()
            emitter.syscall(4);                                                 // write the message to stderr
            emitter.instruction("mov x0, #1");                                  // exit code 1
            emitter.syscall(1);                                                 // terminate the process
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("lea rsi, [rip + {}]", err_label));    // point the write() buffer register at the fatal message
            emitter.instruction(&format!("mov edx, {}", err_len));              // pass the fatal message length to write()
            emitter.instruction("mov edi, 2");                                  // fd = stderr
            emitter.instruction("mov eax, 1");                                  // Linux x86_64 syscall 1 = write
            emitter.instruction("syscall");                                     // emit the fatal message before terminating
            emitter.instruction("mov edi, 1");                                  // exit code 1
            emitter.instruction("mov eax, 60");                                 // Linux x86_64 syscall 60 = exit
            emitter.instruction("syscall");                                     // terminate the process
        }
    }
}
