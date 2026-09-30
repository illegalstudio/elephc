//! Purpose:
//! Emits `__rt_wrapper_disabled_open_warning`, php's two-line refusal for a path operation whose
//! `file://` wrapper has been unregistered.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - `fopen::emit_refuse_when_file_wrapper_disabled_saying`, for the eight helpers php words this
//!   way: `file_get_contents`, `file`, `file_put_contents`, `readfile`, `copy`, `touch`,
//!   `opendir` and `scandir`.
//!
//! Key details:
//! - MEASURED on `php -n` 8.5.6 after `stream_wrapper_unregister("file")`:
//!
//!   ```text
//!   Warning: file_get_contents(): file:// wrapper is disabled in the server configuration
//!   Warning: file_get_contents(uw.txt): Failed to open stream: no suitable wrapper could be found
//!   ```
//!
//!   `opendir()` and `scandir()` say "Failed to open directory" in the second line and are
//!   otherwise identical, which is the whole of what the `directory` flag selects.
//! - NO buffer of its own: `__rt_diag_warning` accumulates the pieces it is handed and writes the
//!   line out when one ends in a newline, which is how the head / name / tail diagnostics already
//!   in this crate are composed. So this is eight calls and no byte copying, and the blank line,
//!   php's ` in <file> on line <n>` and the `@` suppression all come from that helper.
//! - The PATH is the elephc string the program wrote, passed straight through in the string
//!   registers — php names what the caller spelled, so there is nothing to canonicalise.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// php's opening for every diagnostic here.
pub(crate) const WARNING_HEAD: &str = "Warning: ";

/// The tail of the first line, from the callee name onwards.
pub(crate) const WRAPPER_DISABLED_TAIL: &str =
    "(): file:// wrapper is disabled in the server configuration\n";

/// The tail of the second line for an operation that opens a STREAM.
pub(crate) const NO_WRAPPER_STREAM_TAIL: &str =
    "): Failed to open stream: no suitable wrapper could be found\n";

/// The same for one that opens a DIRECTORY — `opendir()` and `scandir()`.
pub(crate) const NO_WRAPPER_DIRECTORY_TAIL: &str =
    "): Failed to open directory: no suitable wrapper could be found\n";

/// Emits `__rt_wrapper_disabled_open_warning`.
///
/// AArch64: x0 = callee name pointer, x3 = its length, x1/x2 = the path, x4 = 1 for a directory.
/// x86_64: rdi = callee name pointer, rsi = its length, rax/rdx = the path, r8 = 1 for a directory.
///
/// The path stays in the STRING registers on both arches so a call site can set the other three
/// without touching what it was already holding.
pub fn emit_wrapper_disabled_open_warning(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Emits the AArch64 form.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: the two lines php gives a disabled file:// wrapper ---");
    emitter.label_global("__rt_wrapper_disabled_open_warning");
    // Frame: [0] name ptr, [8] name len, [16] path ptr, [24] path len, [32] directory flag.
    emitter.instruction("sub sp, sp, #64");
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // the callee name php puts before the parenthesis
    emitter.instruction("str x3, [sp, #8]");
    // A caller that reads THROUGH another helper publishes the name php should print — that is
    // what `readfile()` does, which runs on `__rt_file_get_contents` and would otherwise call
    // itself `file_get_contents`. The slot is null unless a lowering just set it.
    abi::emit_load_symbol_to_reg(emitter, "x9", "_rt_open_diag_name", 0);
    emitter.instruction("cbz x9, __rt_wdow_own_name");
    emitter.instruction("str x9, [sp, #0]");
    abi::emit_load_symbol_to_reg(emitter, "x9", "_rt_open_diag_name_len", 0);
    emitter.instruction("str x9, [sp, #8]");
    emitter.label("__rt_wdow_own_name");
    emitter.instruction("str x1, [sp, #16]");                                   // the path exactly as the program wrote it
    emitter.instruction("str x2, [sp, #24]");
    emitter.instruction("str x4, [sp, #32]");                                   // 1 when php says "directory" rather than "stream"

    // -- "Warning: " + name + "(): file:// wrapper is disabled in the server configuration\n" --
    emit_piece_symbol_aarch64(emitter, "_wd_head", WARNING_HEAD.len());
    emit_piece_frame_aarch64(emitter, 0, 8);
    emit_piece_symbol_aarch64(emitter, "_wd_disabled_tail", WRAPPER_DISABLED_TAIL.len());

    // -- "Warning: " + name + "(" + path + "): Failed to open …\n" --
    emit_piece_symbol_aarch64(emitter, "_wd_head", WARNING_HEAD.len());
    emit_piece_frame_aarch64(emitter, 0, 8);
    emit_piece_symbol_aarch64(emitter, "_wd_lparen", 1);
    emit_piece_frame_aarch64(emitter, 16, 24);
    emitter.instruction("ldr x9, [sp, #32]");                                   // which of the two tails php uses
    emitter.instruction("cbnz x9, __rt_wdow_directory");
    abi::emit_symbol_address(emitter, "x1", "_wd_tail_stream");
    emitter.instruction(&format!("mov x2, #{}", NO_WRAPPER_STREAM_TAIL.len()));
    emitter.instruction("b __rt_wdow_tail_ready");
    emitter.label("__rt_wdow_directory");
    abi::emit_symbol_address(emitter, "x1", "_wd_tail_dir");
    emitter.instruction(&format!("mov x2, #{}", NO_WRAPPER_DIRECTORY_TAIL.len()));
    emitter.label("__rt_wdow_tail_ready");
    emitter.instruction("bl __rt_diag_warning");                                // the newline flushes the second line

    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the helper frame
    emitter.instruction("ret");
}

/// Hands one fixed piece to the accumulating diagnostic helper.
fn emit_piece_symbol_aarch64(emitter: &mut Emitter, symbol: &str, len: usize) {
    abi::emit_symbol_address(emitter, "x1", symbol);
    emitter.instruction(&format!("mov x2, #{len}"));
    emitter.instruction("bl __rt_diag_warning");
}

/// Hands one frame-held (pointer, length) pair to the accumulating diagnostic helper.
fn emit_piece_frame_aarch64(emitter: &mut Emitter, ptr_off: usize, len_off: usize) {
    emitter.instruction(&format!("ldr x1, [sp, #{ptr_off}]"));
    emitter.instruction(&format!("ldr x2, [sp, #{len_off}]"));
    emitter.instruction("bl __rt_diag_warning");
}

/// Emits the Linux x86_64 form.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: the two lines php gives a disabled file:// wrapper ---");
    emitter.label_global("__rt_wrapper_disabled_open_warning");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame pointer
    emitter.instruction("sub rsp, 48");                                         // the five values below, 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // the callee name php puts before the parenthesis
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");
    // See the AArch64 arm: a published name wins, which is how `readfile()` avoids naming the
    // helper it reads through.
    abi::emit_load_symbol_to_reg(emitter, "r9", "_rt_open_diag_name", 0);
    emitter.instruction("test r9, r9");
    emitter.instruction("jz __rt_wdow_own_name_x86");
    emitter.instruction("mov QWORD PTR [rbp - 8], r9");
    abi::emit_load_symbol_to_reg(emitter, "r9", "_rt_open_diag_name_len", 0);
    emitter.instruction("mov QWORD PTR [rbp - 16], r9");
    emitter.label("__rt_wdow_own_name_x86");
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // the path exactly as the program wrote it
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // 1 when php says "directory" rather than "stream"

    emit_piece_symbol_x86(emitter, "_wd_head", WARNING_HEAD.len());
    emit_piece_frame_x86(emitter, 8, 16);
    emit_piece_symbol_x86(emitter, "_wd_disabled_tail", WRAPPER_DISABLED_TAIL.len());

    emit_piece_symbol_x86(emitter, "_wd_head", WARNING_HEAD.len());
    emit_piece_frame_x86(emitter, 8, 16);
    emit_piece_symbol_x86(emitter, "_wd_lparen", 1);
    emit_piece_frame_x86(emitter, 24, 32);
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // which of the two tails php uses
    emitter.instruction("test r9, r9");
    emitter.instruction("jnz __rt_wdow_directory_x86");
    abi::emit_symbol_address(emitter, "rdi", "_wd_tail_stream");
    emitter.instruction(&format!("mov esi, {}", NO_WRAPPER_STREAM_TAIL.len()));
    emitter.instruction("jmp __rt_wdow_tail_ready_x86");
    emitter.label("__rt_wdow_directory_x86");
    abi::emit_symbol_address(emitter, "rdi", "_wd_tail_dir");
    emitter.instruction(&format!("mov esi, {}", NO_WRAPPER_DIRECTORY_TAIL.len()));
    emitter.label("__rt_wdow_tail_ready_x86");
    emitter.instruction("call __rt_diag_warning");                              // the newline flushes the second line

    emitter.instruction("mov rsp, rbp");                                        // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");
}

/// The x86_64 counterpart of [`emit_piece_symbol_aarch64`].
fn emit_piece_symbol_x86(emitter: &mut Emitter, symbol: &str, len: usize) {
    abi::emit_symbol_address(emitter, "rdi", symbol);
    emitter.instruction(&format!("mov esi, {len}"));
    emitter.instruction("call __rt_diag_warning");
}

/// The x86_64 counterpart of [`emit_piece_frame_aarch64`].
fn emit_piece_frame_x86(emitter: &mut Emitter, ptr_off: usize, len_off: usize) {
    emitter.instruction(&format!("mov rdi, QWORD PTR [rbp - {ptr_off}]"));
    emitter.instruction(&format!("mov rsi, QWORD PTR [rbp - {len_off}]"));
    emitter.instruction("call __rt_diag_warning");
}
