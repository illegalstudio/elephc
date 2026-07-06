//! Purpose:
//! Emits the `__rt_warn_string_offset` runtime helper for out-of-bounds string offsets.
//! Formats the PHP "Uninitialized string offset N" warning carrying the runtime offset value.
//!
//! Called from:
//! - `crate::codegen::runtime::diagnostics::emit_string_offset_warning()`.
//!
//! Key details:
//! - The helper is warning-only: callers still materialize their own empty-string fallback.
//! - `__rt_itoa` uses `_concat_buf`, so `_concat_off` is restored before returning.

use crate::codegen::abi;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

const STRING_OFFSET_PREFIX_LEN: usize = "Warning: Uninitialized string offset ".len();
const STRING_OFFSET_NL_LEN: usize = "\n".len();

/// Emits `__rt_warn_string_offset` for the active target.
///
/// # ABI
/// - ARM64: input offset in `x0`.
/// - x86_64 Linux: input offset in `rax`.
///
/// # Behavior
/// Writes `Warning: Uninitialized string offset <N>\n` to stderr (via
/// `__rt_diag_warning`) when `@` suppression is inactive, then returns.
pub fn emit_string_offset_warning(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_string_offset_warning_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: string_offset_warning ---");
    emitter.label_global("__rt_warn_string_offset");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #48");                                     // reserve saved offset, concat cursor, and frame linkage
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish a stable runtime warning frame
    emitter.instruction("str x0, [sp, #0]");                                    // save the out-of-bounds offset across warning fragments
    abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // snapshot concat scratch state before formatting the offset
    emitter.instruction("str x10, [sp, #8]");                                   // preserve the concat cursor across itoa

    // -- emit prefix --
    abi::emit_symbol_address(emitter, "x1", "_diag_string_offset_prefix");
    emitter.instruction(&format!("mov x2, #{}", STRING_OFFSET_PREFIX_LEN));     // pass the string-offset warning prefix length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the string-offset warning prefix

    // -- emit formatted offset --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the out-of-bounds offset for decimal formatting
    abi::emit_call_label(emitter, "__rt_itoa");                                  // format the offset into concat scratch
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the formatted offset value
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the pre-warning concat cursor
    abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("str x10, [x9]");                                       // restore concat scratch state for surrounding expressions

    // -- emit newline suffix --
    abi::emit_symbol_address(emitter, "x1", "_diag_string_offset_nl");
    emitter.instruction(&format!("mov x2, #{}", STRING_OFFSET_NL_LEN));         // pass the string-offset warning newline length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the string-offset warning newline

    // -- restore stack frame --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the runtime warning frame
    emitter.instruction("ret");                                                 // return to the string-index caller
}

/// Emits the x86_64 implementation of `__rt_warn_string_offset`.
fn emit_string_offset_warning_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: string_offset_warning ---");
    emitter.label_global("__rt_warn_string_offset");

    // -- set up stack frame --
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable runtime warning frame
    emitter.instruction("sub rsp, 32");                                         // reserve saved offset and concat cursor while keeping calls aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the out-of-bounds offset across warning fragments
    abi::emit_load_symbol_to_reg(emitter, "r10", "_concat_off", 0);             // snapshot concat scratch state before formatting the offset
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // preserve the concat cursor across itoa

    // -- emit prefix --
    abi::emit_symbol_address(emitter, "rdi", "_diag_string_offset_prefix");
    emitter.instruction(&format!("mov esi, {}", STRING_OFFSET_PREFIX_LEN));     // pass the string-offset warning prefix length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the string-offset warning prefix

    // -- emit formatted offset --
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the out-of-bounds offset for decimal formatting
    abi::emit_call_label(emitter, "__rt_itoa");                                  // format the offset into concat scratch
    emitter.instruction("mov rdi, rax");                                        // pass the formatted offset pointer to the warning helper
    emitter.instruction("mov rsi, rdx");                                        // pass the formatted offset length to the warning helper
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the formatted offset value
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the pre-warning concat cursor
    abi::emit_store_reg_to_symbol(emitter, "r10", "_concat_off", 0);            // restore concat scratch state for surrounding expressions

    // -- emit newline suffix --
    abi::emit_symbol_address(emitter, "rdi", "_diag_string_offset_nl");
    emitter.instruction(&format!("mov esi, {}", STRING_OFFSET_NL_LEN));         // pass the string-offset warning newline length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the string-offset warning newline

    // -- restore stack frame --
    emitter.instruction("mov rsp, rbp");                                        // release the runtime warning frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the string-index caller
}