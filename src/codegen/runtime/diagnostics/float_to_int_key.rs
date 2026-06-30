//! Purpose:
//! Emits the `__rt_warn_float_to_int_key` runtime helper for float-to-int array key conversion.
//! Formats the PHP "Implicit conversion from float … to int loses precision" deprecation.
//!
//! Called from:
//! - `crate::codegen::runtime::diagnostics::emit_float_to_int_key_deprecation()`.
//!
//! Key details:
//! - The helper is deprecation-only: callers still truncate the float to perform the lookup.
//! - `__rt_ftoa` uses `_concat_buf`, so `_concat_off` is restored before returning.

use crate::codegen::abi;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

const FLOAT_KEY_PREFIX_LEN: usize = "Deprecated: Implicit conversion from float ".len();
const FLOAT_KEY_SUFFIX_LEN: usize = " to int loses precision\n".len();

/// Emits `__rt_warn_float_to_int_key` for the active target.
///
/// # ABI
/// - ARM64: input float in `d0`.
/// - x86_64 Linux: input float in `xmm0`.
///
/// # Behavior
/// Writes `Deprecated: Implicit conversion from float <V> to int loses precision\n`
/// to stderr (via `__rt_diag_warning`) when `@` suppression is inactive, then returns.
pub fn emit_float_to_int_key_deprecation(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_float_to_int_key_deprecation_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: float_to_int_key_deprecation ---");
    emitter.label_global("__rt_warn_float_to_int_key");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #64");                                     // reserve saved float, concat cursor, and frame linkage
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish a stable runtime deprecation frame
    emitter.instruction("str d0, [sp, #0]");                                    // save the float key across deprecation fragments
    abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // snapshot concat scratch state before formatting the float
    emitter.instruction("str x10, [sp, #8]");                                   // preserve the concat cursor across ftoa

    // -- emit prefix --
    abi::emit_symbol_address(emitter, "x1", "_diag_float_key_prefix");
    emitter.instruction(&format!("mov x2, #{}", FLOAT_KEY_PREFIX_LEN));         // pass the float-key deprecation prefix length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the float-key deprecation prefix

    // -- emit formatted float --
    emitter.instruction("ldr d0, [sp, #0]");                                    // reload the float key for decimal formatting
    abi::emit_call_label(emitter, "__rt_ftoa");                                 // format the float key into concat scratch
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the formatted float-key value
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the pre-warning concat cursor
    abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("str x10, [x9]");                                       // restore concat scratch state for surrounding expressions

    // -- emit suffix --
    abi::emit_symbol_address(emitter, "x1", "_diag_float_key_suffix");
    emitter.instruction(&format!("mov x2, #{}", FLOAT_KEY_SUFFIX_LEN));         // pass the float-key deprecation suffix length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the float-key deprecation suffix

    // -- restore stack frame --
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the runtime deprecation frame
    emitter.instruction("ret");                                                 // return to the hash-key caller
}

/// Emits the x86_64 implementation of `__rt_warn_float_to_int_key`.
fn emit_float_to_int_key_deprecation_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: float_to_int_key_deprecation ---");
    emitter.label_global("__rt_warn_float_to_int_key");

    // -- set up stack frame --
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable runtime deprecation frame
    emitter.instruction("sub rsp, 32");                                         // reserve saved float and concat cursor while keeping calls aligned
    emitter.instruction("movsd QWORD PTR [rbp - 8], xmm0");                     // save the float key across deprecation fragments
    abi::emit_load_symbol_to_reg(emitter, "r10", "_concat_off", 0);             // snapshot concat scratch state before formatting the float
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // preserve the concat cursor across ftoa

    // -- emit prefix --
    abi::emit_symbol_address(emitter, "rdi", "_diag_float_key_prefix");
    emitter.instruction(&format!("mov esi, {}", FLOAT_KEY_PREFIX_LEN));         // pass the float-key deprecation prefix length
    abi::emit_call_label(emitter, "__rt_diag_warning");                          // emit or suppress the float-key deprecation prefix

    // -- emit formatted float --
    emitter.instruction("movsd xmm0, QWORD PTR [rbp - 8]");                     // reload the float key for decimal formatting
    abi::emit_call_label(emitter, "__rt_ftoa");                                  // format the float key into concat scratch
    emitter.instruction("mov rdi, rax");                                        // pass the formatted float-key pointer to the deprecation helper
    emitter.instruction("mov rsi, rdx");                                        // pass the formatted float-key length to the deprecation helper
    abi::emit_call_label(emitter, "__rt_diag_warning");                          // emit or suppress the formatted float-key value
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the pre-warning concat cursor
    abi::emit_store_reg_to_symbol(emitter, "r10", "_concat_off", 0);             // restore concat scratch state for surrounding expressions

    // -- emit suffix --
    abi::emit_symbol_address(emitter, "rdi", "_diag_float_key_suffix");
    emitter.instruction(&format!("mov esi, {}", FLOAT_KEY_SUFFIX_LEN));         // pass the float-key deprecation suffix length
    abi::emit_call_label(emitter, "__rt_diag_warning");                          // emit or suppress the float-key deprecation suffix

    // -- restore stack frame --
    emitter.instruction("mov rsp, rbp");                                        // release the runtime deprecation frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the hash-key caller
}