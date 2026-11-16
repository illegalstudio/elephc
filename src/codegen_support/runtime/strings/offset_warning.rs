//! Purpose:
//! Emits the `__rt_warn_string_offset` and `__rt_warn_illegal_string_offset` runtime helpers.
//! Formats the PHP "Uninitialized string offset N" (read) and "Illegal string offset N" (write)
//! warnings carrying the runtime offset value.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()`.
//! - `__rt_str_offset_set` for the write-side illegal-offset warning.
//!
//! Key details:
//! - The helpers are warning-only: callers still materialize their own fallback result.
//! - `__rt_itoa` uses `_concat_buf`, so `_concat_off` is restored before returning.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

const STRING_OFFSET_NL_LEN: usize = "\n".len();

/// One offset-carrying string warning: `<prefix><N>\n`.
struct OffsetWarning {
    /// Global label of the helper.
    label: &'static str,
    /// Data symbol holding the message text before the formatted offset.
    prefix_symbol: &'static str,
    /// Byte length of that prefix text.
    prefix_len: usize,
    /// Name used in the emitted runtime banner comment.
    banner: &'static str,
}

const UNINITIALIZED_OFFSET: OffsetWarning = OffsetWarning {
    label: "__rt_warn_string_offset",
    prefix_symbol: "_diag_string_offset_prefix",
    prefix_len: "Warning: Uninitialized string offset ".len(),
    banner: "string_offset_warning",
};

const ILLEGAL_OFFSET: OffsetWarning = OffsetWarning {
    label: "__rt_warn_illegal_string_offset",
    prefix_symbol: "_diag_illegal_string_offset_prefix",
    prefix_len: "Warning: Illegal string offset ".len(),
    banner: "illegal_string_offset_warning",
};

/// Emits `__rt_warn_string_offset` and `__rt_warn_illegal_string_offset` for the active target.
///
/// # ABI
/// - ARM64: input offset in `x0`.
/// - x86_64 Linux: input offset in `rax`.
///
/// # Behavior
/// Writes `Warning: Uninitialized string offset <N>\n` (read of a missing offset) or
/// `Warning: Illegal string offset <N>\n` (write before the start of the string) as one PHP
/// warning through the shared diagnostic dispatcher, then returns.
pub fn emit_string_offset_warning(emitter: &mut Emitter) {
    for spec in [&UNINITIALIZED_OFFSET, &ILLEGAL_OFFSET] {
        if emitter.target.arch == Arch::X86_64 {
            emit_offset_warning_x86_64(emitter, spec);
        } else {
            emit_offset_warning_aarch64(emitter, spec);
        }
    }
}

/// Emits the ARM64 implementation of one offset-carrying string warning helper.
fn emit_offset_warning_aarch64(emitter: &mut Emitter, spec: &OffsetWarning) {
    emitter.blank();
    emitter.comment(&format!("--- runtime: {} ---", spec.banner));
    emitter.label_global(spec.label);

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #48");                                     // reserve saved offset, concat cursor, and frame linkage
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish a stable runtime warning frame
    emitter.instruction("str x0, [sp, #0]");                                    // save the offending offset across warning fragments
    abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // snapshot concat scratch state before formatting the offset
    emitter.instruction("str x10, [sp, #8]");                                   // preserve the concat cursor across itoa

    // -- emit prefix --
    abi::emit_symbol_address(emitter, "x1", spec.prefix_symbol);
    emitter.instruction(&format!("mov x2, #{}", spec.prefix_len));              // pass the string-offset warning prefix length
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");                // append the string-offset warning prefix

    // -- emit formatted offset --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the offending offset for decimal formatting
    abi::emit_call_label(emitter, "__rt_itoa");                                  // format the offset into concat scratch
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");                // append the formatted offset value
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
    emitter.instruction("ret");                                                 // return to the string-offset caller
}

/// Emits the x86_64 implementation of one offset-carrying string warning helper.
fn emit_offset_warning_x86_64(emitter: &mut Emitter, spec: &OffsetWarning) {
    emitter.blank();
    emitter.comment(&format!("--- runtime: {} ---", spec.banner));
    emitter.label_global(spec.label);

    // -- set up stack frame --
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable runtime warning frame
    emitter.instruction("sub rsp, 32");                                         // reserve saved offset and concat cursor while keeping calls aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the offending offset across warning fragments
    abi::emit_load_symbol_to_reg(emitter, "r10", "_concat_off", 0);             // snapshot concat scratch state before formatting the offset
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // preserve the concat cursor across itoa

    // -- emit prefix --
    abi::emit_symbol_address(emitter, "rdi", spec.prefix_symbol);
    emitter.instruction(&format!("mov esi, {}", spec.prefix_len));              // pass the string-offset warning prefix length
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");                // append the string-offset warning prefix

    // -- emit formatted offset --
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the offending offset for decimal formatting
    abi::emit_call_label(emitter, "__rt_itoa");                                  // format the offset into concat scratch
    emitter.instruction("mov rdi, rax");                                        // pass the formatted offset pointer to the warning helper
    emitter.instruction("mov rsi, rdx");                                        // pass the formatted offset length to the warning helper
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");                // append the formatted offset value
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the pre-warning concat cursor
    abi::emit_store_reg_to_symbol(emitter, "r10", "_concat_off", 0);            // restore concat scratch state for surrounding expressions

    // -- emit newline suffix --
    abi::emit_symbol_address(emitter, "rdi", "_diag_string_offset_nl");
    emitter.instruction(&format!("mov esi, {}", STRING_OFFSET_NL_LEN));         // pass the string-offset warning newline length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the string-offset warning newline

    // -- restore stack frame --
    emitter.instruction("mov rsp, rbp");                                        // release the runtime warning frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the string-offset caller
}
