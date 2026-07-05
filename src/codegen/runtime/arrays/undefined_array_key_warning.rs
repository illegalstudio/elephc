//! Purpose:
//! Emits runtime warning helpers for undefined integer and string array keys.
//! Formats missing key values while preserving concat scratch state where needed.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()`.
//!
//! Key details:
//! - The helper is warning-only: callers still materialize their own null fallback.
//! - `__rt_itoa` uses `_concat_buf`, so `_concat_off` is restored before returning.

use crate::codegen::abi;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

const UNDEFINED_ARRAY_KEY_PREFIX_LEN: usize = "Warning: Undefined array key ".len();
const UNDEFINED_ARRAY_KEY_QUOTE_LEN: usize = "\"".len();
const UNDEFINED_ARRAY_KEY_SUFFIX_LEN: usize = "\n".len();

/// Emits `__rt_warn_undefined_array_key_int` for the active target.
pub fn emit_undefined_array_key_warning(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_undefined_array_key_warning_x86_64(emitter);
        emit_undefined_array_key_string_warning_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: undefined_array_key_warning ---");
    emitter.label_global("__rt_warn_undefined_array_key_int");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #48");                                     // reserve saved key, concat cursor, and frame linkage
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish a stable runtime warning frame
    emitter.instruction("str x0, [sp, #0]");                                    // save the missing integer key across warning fragments
    abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("ldr x10, [x9]");                                       // snapshot concat scratch state before formatting the key
    emitter.instruction("str x10, [sp, #8]");                                   // preserve the concat cursor across itoa

    // -- emit prefix --
    abi::emit_symbol_address(emitter, "x1", "_diag_undefined_array_key_prefix");
    emitter.instruction(&format!("mov x2, #{}", UNDEFINED_ARRAY_KEY_PREFIX_LEN)); // pass the undefined-key warning prefix length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the undefined-key warning prefix

    // -- emit formatted key --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the missing integer key for decimal formatting
    abi::emit_call_label(emitter, "__rt_itoa");                                 // format the missing key into concat scratch
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the formatted missing-key value
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the pre-warning concat cursor
    abi::emit_symbol_address(emitter, "x9", "_concat_off");
    emitter.instruction("str x10, [x9]");                                       // restore concat scratch state for surrounding expressions

    // -- emit suffix --
    abi::emit_symbol_address(emitter, "x1", "_diag_undefined_array_key_suffix");
    emitter.instruction(&format!("mov x2, #{}", UNDEFINED_ARRAY_KEY_SUFFIX_LEN)); // pass the undefined-key warning suffix length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the undefined-key warning suffix

    // -- restore stack frame --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the runtime warning frame
    emitter.instruction("ret");                                                 // return to the array-miss caller

    emit_undefined_array_key_string_warning_aarch64(emitter);
}

/// Emits the x86_64 implementation of `__rt_warn_undefined_array_key_int`.
fn emit_undefined_array_key_warning_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: undefined_array_key_warning ---");
    emitter.label_global("__rt_warn_undefined_array_key_int");

    // -- set up stack frame --
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable runtime warning frame
    emitter.instruction("sub rsp, 32");                                         // reserve saved key and concat cursor while keeping calls aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the missing integer key across warning fragments
    abi::emit_load_symbol_to_reg(emitter, "r10", "_concat_off", 0);             // snapshot concat scratch state before formatting the key
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // preserve the concat cursor across itoa

    // -- emit prefix --
    abi::emit_symbol_address(emitter, "rdi", "_diag_undefined_array_key_prefix");
    emitter.instruction(&format!("mov esi, {}", UNDEFINED_ARRAY_KEY_PREFIX_LEN)); // pass the undefined-key warning prefix length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the undefined-key warning prefix

    // -- emit formatted key --
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the missing integer key for decimal formatting
    abi::emit_call_label(emitter, "__rt_itoa");                                 // format the missing key into concat scratch
    emitter.instruction("mov rdi, rax");                                        // pass the formatted missing-key pointer to the warning helper
    emitter.instruction("mov rsi, rdx");                                        // pass the formatted missing-key length to the warning helper
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the formatted missing-key value
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the pre-warning concat cursor
    abi::emit_store_reg_to_symbol(emitter, "r10", "_concat_off", 0);            // restore concat scratch state for surrounding expressions

    // -- emit suffix --
    abi::emit_symbol_address(emitter, "rdi", "_diag_undefined_array_key_suffix");
    emitter.instruction(&format!("mov esi, {}", UNDEFINED_ARRAY_KEY_SUFFIX_LEN)); // pass the undefined-key warning suffix length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the undefined-key warning suffix

    // -- restore stack frame --
    emitter.instruction("mov rsp, rbp");                                        // release the runtime warning frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the array-miss caller
}

/// Emits the ARM64 implementation of `__rt_warn_undefined_array_key_str`.
fn emit_undefined_array_key_string_warning_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: undefined_array_key_string_warning ---");
    emitter.label_global("__rt_warn_undefined_array_key_str");

    // -- set up stack frame --
    emitter.instruction("sub sp, sp, #48");                                     // reserve saved string key and frame linkage
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish a stable runtime warning frame
    emitter.instruction("str x1, [sp, #0]");                                    // save the missing string key pointer across warning fragments
    emitter.instruction("str x2, [sp, #8]");                                    // save the missing string key length across warning fragments

    // -- emit prefix --
    abi::emit_symbol_address(emitter, "x1", "_diag_undefined_array_key_prefix");
    emitter.instruction(&format!("mov x2, #{}", UNDEFINED_ARRAY_KEY_PREFIX_LEN)); // pass the undefined-key warning prefix length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the undefined-key warning prefix

    // -- emit quoted string key --
    abi::emit_symbol_address(emitter, "x1", "_diag_undefined_array_key_quote");
    emitter.instruction(&format!("mov x2, #{}", UNDEFINED_ARRAY_KEY_QUOTE_LEN)); // pass the opening quote length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the opening quote
    emitter.instruction("ldr x1, [sp, #0]");                                    // reload the missing string key pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // reload the missing string key length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the missing string key bytes
    abi::emit_symbol_address(emitter, "x1", "_diag_undefined_array_key_quote");
    emitter.instruction(&format!("mov x2, #{}", UNDEFINED_ARRAY_KEY_QUOTE_LEN)); // pass the closing quote length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the closing quote

    // -- emit suffix --
    abi::emit_symbol_address(emitter, "x1", "_diag_undefined_array_key_suffix");
    emitter.instruction(&format!("mov x2, #{}", UNDEFINED_ARRAY_KEY_SUFFIX_LEN)); // pass the undefined-key warning suffix length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the undefined-key warning suffix

    // -- restore stack frame --
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the runtime warning frame
    emitter.instruction("ret");                                                 // return to the array-miss caller
}

/// Emits the x86_64 implementation of `__rt_warn_undefined_array_key_str`.
fn emit_undefined_array_key_string_warning_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: undefined_array_key_string_warning ---");
    emitter.label_global("__rt_warn_undefined_array_key_str");

    // -- set up stack frame --
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable runtime warning frame
    emitter.instruction("sub rsp, 32");                                         // reserve saved key pointer and length while keeping calls aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the missing string key pointer across warning fragments
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the missing string key length across warning fragments

    // -- emit prefix --
    abi::emit_symbol_address(emitter, "rdi", "_diag_undefined_array_key_prefix");
    emitter.instruction(&format!("mov esi, {}", UNDEFINED_ARRAY_KEY_PREFIX_LEN)); // pass the undefined-key warning prefix length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the undefined-key warning prefix

    // -- emit quoted string key --
    abi::emit_symbol_address(emitter, "rdi", "_diag_undefined_array_key_quote");
    emitter.instruction(&format!("mov esi, {}", UNDEFINED_ARRAY_KEY_QUOTE_LEN)); // pass the opening quote length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the opening quote
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the missing string key pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the missing string key length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the missing string key bytes
    abi::emit_symbol_address(emitter, "rdi", "_diag_undefined_array_key_quote");
    emitter.instruction(&format!("mov esi, {}", UNDEFINED_ARRAY_KEY_QUOTE_LEN)); // pass the closing quote length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the closing quote

    // -- emit suffix --
    abi::emit_symbol_address(emitter, "rdi", "_diag_undefined_array_key_suffix");
    emitter.instruction(&format!("mov esi, {}", UNDEFINED_ARRAY_KEY_SUFFIX_LEN)); // pass the undefined-key warning suffix length
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the undefined-key warning suffix

    // -- restore stack frame --
    emitter.instruction("mov rsp, rbp");                                        // release the runtime warning frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the array-miss caller
}
