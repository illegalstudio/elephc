//! Purpose:
//! Emits `__rt_curl_warn_unsupported_option`, the PHP warning `curl_setopt()` raises when
//! it is handed a real `CURLOPT_*` option this build cannot apply safely.
//!
//! Called from:
//! - `crate::codegen_support::runtime::curl::emit_curl`.
//! - The `__elephc_curl_setopt_unsupported_warning` builtin, from the elephc-PHP body of
//!   `curl_setopt()` in `crate::curl_prelude`.
//!
//! Key details:
//! - WHY A WARNING AND NOT AN EXCEPTION. An unsupported option must
//!   return `false` and SAY SO, never an inert `true`.
//!   The option really is a valid PHP `CURLOPT_*` — it is this build that cannot carry it
//!   yet — so PHP's `ValueError: … is not a valid cURL option` would be a lie, and a
//!   silent `false` would leave the caller guessing.
//! - IT USES THE EXISTING DIAGNOSTIC CHANNEL, `__rt_diag_warning`, exactly like
//!   `__rt_warn_undefined_array_key_int` (`runtime::arrays::undefined_array_key_warning`),
//!   which this emitter is modelled on line for line. That channel honours PHP's
//!   `display_errors` handling, so a suppressed warning stays suppressed; writing to
//!   stderr directly from here would bypass it.
//! - `__rt_itoa` FORMATS INTO THE SHARED CONCAT SCRATCH, so `_concat_off` is snapshotted
//!   before the call and restored after it. Skipping that corrupts any string
//!   concatenation in progress in the surrounding expression — the same hazard the
//!   undefined-array-key helper documents.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::runtime::data::{
    CURL_MULTI_SETOPT_UNSUPPORTED_PREFIX, CURL_SETOPT_UNSUPPORTED_PREFIX,
    CURL_SETOPT_UNSUPPORTED_SUFFIX,
};

/// `__rt_curl_warn_unsupported_option` — in: the option number in `x0`/`rax`. Out: nothing.
pub(crate) fn emit_curl_warn_unsupported_option(emitter: &mut Emitter) {
    emit_unsupported_option_warning(
        emitter,
        "__rt_curl_warn_unsupported_option",
        "curl_setopt unsupported-option warning",
        "_diag_curl_setopt_unsupported_prefix",
        CURL_SETOPT_UNSUPPORTED_PREFIX.len(),
    );
}

/// `__rt_curl_multi_warn_unsupported_option` — the same warning for `curl_multi_setopt()`.
///
/// IT IS A SEPARATE HELPER ONLY BECAUSE THE FUNCTION NAME IN THE MESSAGE IS PART OF THE
/// DIAGNOSTIC. PHP names the function that refused the option, and a `curl_multi_setopt()`
/// call that printed `curl_setopt():` would send a reader looking at the wrong call. Only
/// the prefix string differs; the option number and the shared suffix are the same.
pub(crate) fn emit_curl_multi_warn_unsupported_option(emitter: &mut Emitter) {
    emit_unsupported_option_warning(
        emitter,
        "__rt_curl_multi_warn_unsupported_option",
        "curl_multi_setopt unsupported-option warning",
        "_diag_curl_multi_setopt_unsupported_prefix",
        CURL_MULTI_SETOPT_UNSUPPORTED_PREFIX.len(),
    );
}

/// Emits one unsupported-option warning helper: prefix, the option number formatted
/// through `__rt_itoa`, then the shared suffix — all through `__rt_diag_warning`.
fn emit_unsupported_option_warning(
    emitter: &mut Emitter,
    label: &str,
    description: &str,
    prefix_symbol: &str,
    prefix_len: usize,
) {
    let suffix_len = CURL_SETOPT_UNSUPPORTED_SUFFIX.len();
    emitter.blank();
    emitter.comment(&format!("--- runtime: {description} ---"));
    emitter.label_global(label);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #48");                             // saved option, concat cursor, and frame linkage

            emitter.instruction("stp x29, x30, [sp, #32]");                     // save frame pointer and return address

            emitter.instruction("add x29, sp, #32");                            // establish a stable warning frame

            emitter.instruction("str x0, [sp]");                                // save the option number across the warning fragments

            abi::emit_symbol_address(emitter, "x9", "_concat_off");
            emitter.instruction("ldr x10, [x9]");                               // snapshot concat scratch state before formatting the option

            emitter.instruction("str x10, [sp, #8]");                           // preserve the concat cursor across itoa

            abi::emit_symbol_address(emitter, "x1", prefix_symbol);
            emitter.instruction(&format!("mov x2, #{prefix_len}"));             // pass the warning prefix length

            abi::emit_call_label(emitter, "__rt_diag_warning");                 // emit or suppress the warning prefix

            emitter.instruction("ldr x0, [sp]");                                // reload the option number for decimal formatting

            abi::emit_call_label(emitter, "__rt_itoa");                         // format the option number into concat scratch

            abi::emit_call_label(emitter, "__rt_diag_warning");                 // emit or suppress the formatted option number

            emitter.instruction("ldr x10, [sp, #8]");                           // reload the pre-warning concat cursor

            abi::emit_symbol_address(emitter, "x9", "_concat_off");
            emitter.instruction("str x10, [x9]");                               // restore concat scratch for surrounding expressions

            abi::emit_symbol_address(emitter, "x1", "_diag_curl_setopt_unsupported_suffix");
            emitter.instruction(&format!("mov x2, #{suffix_len}"));             // pass the warning suffix length

            abi::emit_call_label(emitter, "__rt_diag_warning");                 // emit or suppress the warning suffix

            emitter.instruction("ldp x29, x30, [sp, #32]");                     // restore frame pointer and return address

            emitter.instruction("add sp, sp, #48");                             // release the warning frame

            emitter.instruction("ret");                                         // return to the curl_setopt wrapper

        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // save the caller frame pointer

            emitter.instruction("mov rbp, rsp");                                // establish a stable warning frame

            emitter.instruction("sub rsp, 32");                                 // saved option and concat cursor, calls stay aligned

            emitter.instruction("mov QWORD PTR [rbp - 8], rax");                // save the option number across the warning fragments

            abi::emit_load_symbol_to_reg(emitter, "r10", "_concat_off", 0);     // snapshot concat scratch state before formatting the option

            emitter.instruction("mov QWORD PTR [rbp - 16], r10");               // preserve the concat cursor across itoa

            abi::emit_symbol_address(emitter, "rdi", prefix_symbol);
            emitter.instruction(&format!("mov esi, {prefix_len}"));             // pass the warning prefix length

            abi::emit_call_label(emitter, "__rt_diag_warning");                 // emit or suppress the warning prefix

            emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                // reload the option number for decimal formatting

            abi::emit_call_label(emitter, "__rt_itoa");                         // format the option number into concat scratch

            emitter.instruction("mov rdi, rax");                                // pass the formatted option pointer to the warning helper

            emitter.instruction("mov rsi, rdx");                                // pass the formatted option length to the warning helper

            abi::emit_call_label(emitter, "__rt_diag_warning");                 // emit or suppress the formatted option number

            emitter.instruction("mov r10, QWORD PTR [rbp - 16]");               // reload the pre-warning concat cursor

            abi::emit_store_reg_to_symbol(emitter, "r10", "_concat_off", 0);    // restore concat scratch for surrounding expressions

            abi::emit_symbol_address(emitter, "rdi", "_diag_curl_setopt_unsupported_suffix");
            emitter.instruction(&format!("mov esi, {suffix_len}"));             // pass the warning suffix length

            abi::emit_call_label(emitter, "__rt_diag_warning");                 // emit or suppress the warning suffix

            emitter.instruction("mov rsp, rbp");                                // release the warning frame

            emitter.instruction("pop rbp");                                     // restore the caller frame pointer

            emitter.instruction("ret");                                         // return to the curl_setopt wrapper

        }
    }
}
