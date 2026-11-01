//! Purpose:
//! Emits the `__rt_error_log` runtime helper implementing PHP's `error_log()`
//! for the compiled program: it writes the message to stderr (fd 2) followed by
//! a single trailing newline, matching PHP's CLI/SAPI default (message_type 0).
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::system`.
//!
//! Key details:
//! - The message arrives in the string result registers (ptr in `x1`/`rax`, len
//!   in `x2`/`rdx`); the caller sets the boolean `true` result after the call.
//! - PHP's `error_log("hi")` emits the bytes `hi\n` on stderr — the trailing
//!   newline is appended here from the shared `_error_log_nl` literal.

use crate::codegen::{abi, emit::Emitter, platform::Arch};

/// Emits `__rt_error_log` for every supported target.
///
/// Writes the caller-provided message (string result registers) to stderr,
/// then writes one trailing newline, and returns. The boolean `true` return
/// value is materialized by the caller.
pub fn emit_error_log(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_error_log_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: error_log ---");
    emitter.label_global("__rt_error_log");

    // -- write the message to stderr (fd 2); ptr already in x1, len in x2 --
    emitter.instruction("mov x0, #2");                                          // fd = stderr for the error_log message
    emitter.syscall(4);                                                         // write(2, msg_ptr, msg_len)

    // -- append the trailing newline PHP's error_log emits --
    abi::emit_symbol_address(emitter, "x1", "_error_log_nl");                   // load the address of the trailing newline literal
    emitter.instruction("mov x2, #1");                                          // newline length = 1 byte
    emitter.instruction("mov x0, #2");                                          // fd = stderr for the trailing newline
    emitter.syscall(4);                                                         // write(2, "\n", 1)
    emitter.instruction("ret");                                                 // return to the error_log() call site
}

/// Emits `__rt_error_log` for x86_64 Linux targets.
///
/// The message pointer arrives in `rax` and its length in `rdx` (the x86_64
/// string result registers); both stderr writes use the Linux `write` syscall
/// (number 1) with arguments in `rdi`/`rsi`/`rdx`.
fn emit_error_log_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: error_log ---");
    emitter.label_global("__rt_error_log");

    // -- write the message to stderr (fd 2); ptr in rax, len in rdx --
    emitter.instruction("mov rsi, rax");                                        // move the message pointer into the Linux write buffer register
    emitter.instruction("mov edi, 2");                                          // fd = stderr for the error_log message
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall 1 = write
    emitter.instruction("syscall");                                             // write(2, msg_ptr, msg_len)

    // -- append the trailing newline PHP's error_log emits --
    abi::emit_symbol_address(emitter, "rsi", "_error_log_nl");                  // load the address of the trailing newline literal into the write buffer register
    emitter.instruction("mov edx, 1");                                          // newline length = 1 byte
    emitter.instruction("mov edi, 2");                                          // fd = stderr for the trailing newline
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall 1 = write
    emitter.instruction("syscall");                                             // write(2, "\n", 1)
    emitter.instruction("ret");                                                 // return to the error_log() call site
}
