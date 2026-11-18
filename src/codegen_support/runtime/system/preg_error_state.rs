//! Purpose:
//! Owns the AOT `preg_last_error()` state: the `_preg_last_error` code, PHP's message
//! table, `__rt_preg_last_error_msg`, and the `__rt_preg_exec` wrapper every regex
//! helper calls instead of the raw PCRE2 match entry point.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` (message helper always,
//!   the exec wrapper only with the regex runtime) and fixed data emission.
//!
//! Key details:
//! - EIR lowering clears `_preg_last_error` and `_preg_exec_error` before each `preg_*`
//!   call and records `PREG_INTERNAL_ERROR` itself when the call reports an invalid
//!   pattern; the exec wrapper only ever records a match failure.
//! - `_preg_exec_error` holds the outcome of the latest exec only. The lowering reads it
//!   to decide whether the call failed, so an error a nested `preg_*` call recorded from
//!   inside a `preg_replace_callback()` callback stays visible to `preg_last_error()`
//!   without failing the outer call, like php-src.
//! - The opaque shim keeps the PCRE2 match-error categories php-src tells apart:
//!   `elephc_builtin_contract::pcre_errors` maps the match limit to
//!   `PREG_BACKTRACK_LIMIT_ERROR`, the depth limit to `PREG_RECURSION_LIMIT_ERROR`,
//!   malformed UTF-8 to `PREG_BAD_UTF8_ERROR`, a bad UTF-8 offset to
//!   `PREG_BAD_UTF8_OFFSET_ERROR`, and every other failure (heap limit and out of memory
//!   included) to `PREG_INTERNAL_ERROR`. `REG_NOMATCH` is not an error.
//! - The wrapper preserves the shim's ABI: arguments pass through untouched and the
//!   status comes back in `w0` / `eax`.

use elephc_builtin_contract::pcre_errors::{
    PCRE2_SHIM_NOMATCH, PCRE2_SHIM_STATUS_ERRORS, PREG_ERROR_MESSAGES as MESSAGES,
    PREG_INTERNAL_ERROR,
};

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits the message strings, their `(ptr, len)` table and the table cardinality.
pub(crate) fn emit_preg_error_data() -> String {
    let mut out = String::new();
    for (code, message) in MESSAGES.iter().enumerate() {
        out.push_str(&format!(
            ".globl _preg_err_msg_{code}\n_preg_err_msg_{code}:\n    .ascii \"{message}\"\n"
        ));
    }
    out.push_str("    .balign 8\n.globl _preg_err_msg_table\n_preg_err_msg_table:\n");
    for (code, message) in MESSAGES.iter().enumerate() {
        out.push_str(&format!("    .quad _preg_err_msg_{code}\n    .quad {}\n", message.len()));
    }
    out
}

/// Emits `__rt_preg_last_error_msg`: the message for `_preg_last_error` as a string slice.
///
/// Output: ARM64 `x1`/`x2`, x86_64 `rax`/`rdx`. Out-of-range codes read as "No error".
pub(crate) fn emit_preg_last_error_msg(emitter: &mut Emitter) {
    let count = MESSAGES.len();
    emitter.blank();
    emitter.comment("--- runtime: preg_last_error_msg ---");
    emitter.label_global("__rt_preg_last_error_msg");
    match emitter.target.arch {
        Arch::AArch64 => {
            // -- clamp the code, then index the (ptr, len) table --
            abi::emit_load_symbol_to_reg(emitter, "x10", "_preg_last_error", 0);
            emitter.instruction(&format!("cmp x10, #{}", count));               // is the code inside the message table?
            emitter.instruction("b.lo __rt_preg_last_error_msg_index");         // in-range codes index the table directly
            emitter.instruction("mov x10, #0");                                 // out-of-range codes read as PREG_NO_ERROR
            emitter.label("__rt_preg_last_error_msg_index");
            abi::emit_symbol_address(emitter, "x9", "_preg_err_msg_table");
            emitter.instruction("lsl x10, x10, #4");                            // step over one 16-byte (ptr, len) entry per code
            emitter.instruction("add x9, x9, x10");                             // address the entry for this code
            emitter.instruction("ldr x1, [x9]");                                // return the message pointer
            emitter.instruction("ldr x2, [x9, #8]");                            // return the message length
            emitter.instruction("ret");                                         // return the borrowed static message
        }
        Arch::X86_64 => {
            // -- clamp the code, then index the (ptr, len) table --
            abi::emit_load_symbol_to_reg(emitter, "rcx", "_preg_last_error", 0);
            emitter.instruction(&format!("cmp rcx, {}", count));                // is the code inside the message table?
            let index = "__rt_preg_last_error_msg_index_linux_x86_64";
            emitter.instruction(&format!("jb {index}"));                        // in-range codes index the table directly
            emitter.instruction("xor ecx, ecx");                                // out-of-range codes read as PREG_NO_ERROR
            emitter.label(index);
            abi::emit_symbol_address(emitter, "r9", "_preg_err_msg_table");
            emitter.instruction("shl rcx, 4");                                  // step over one 16-byte (ptr, len) entry per code
            emitter.instruction("add r9, rcx");                                 // address the entry for this code
            emitter.instruction("mov rax, QWORD PTR [r9]");                     // return the message pointer
            emitter.instruction("mov rdx, QWORD PTR [r9 + 8]");                 // return the message length
            emitter.instruction("ret");                                         // return the borrowed static message
        }
    }
}

/// Emits `__rt_preg_exec`: calls the shim's match entry point, records a failing status
/// (other than an ordinary failed match) in `_preg_last_error`, and stores this call's
/// own outcome (`0` or the error code) in `_preg_exec_error`.
///
/// `_preg_exec_error` lets a helper that runs PHP callbacks between matches
/// (`preg_replace_callback()`) tell its own match failure apart from one a nested
/// `preg_*` call in the callback recorded: the exec that ends a helper's match loop is
/// always the helper's own, and it runs after the last callback.
pub(crate) fn emit_preg_exec_wrapper(emitter: &mut Emitter) {
    let label = "__rt_preg_exec";
    let shim = "elephc_pcre2_v1_exec";
    let outcome = format!("{label}_outcome");
    let record = format!("{label}_record");
    emitter.blank();
    emitter.comment("--- runtime: preg_exec ---");
    emitter.label_global(label);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x29, x30, [sp, #-16]!");                   // save frame pointer and return address
            emitter.instruction("mov x29, sp");                                 // establish the wrapper frame
            emitter.bl_c(shim);                                                 // call the opaque PCRE2 shim with the caller's arguments
            emitter.instruction("mov x11, #0");                                 // stage PREG_NO_ERROR as this call's outcome
            emitter.instruction(&format!("cbz w0, {outcome}"));                 // success leaves the error state alone
            emitter.instruction(&format!("cmp w0, #{}", PCRE2_SHIM_NOMATCH));   // is this an ordinary failed match?
            emitter.instruction(&format!("b.eq {outcome}"));                    // a failed match is not an error
            for (status, code) in PCRE2_SHIM_STATUS_ERRORS {
                emitter.instruction(&format!("mov x11, #{code}"));              // stage the PHP code for this shim status
                emitter.instruction(&format!("cmp w0, #{status}"));             // did PCRE2 fail with this error category?
                emitter.instruction(&format!("b.eq {record}"));                 // record the staged PREG_*_ERROR code
            }
            emitter.instruction(&format!("mov x11, #{}", PREG_INTERNAL_ERROR)); // every other failure is an internal error
            emitter.label(&record);
            abi::emit_store_reg_to_symbol(emitter, "x11", "_preg_last_error", 0);
            emitter.label(&outcome);
            abi::emit_store_reg_to_symbol(emitter, "x11", "_preg_exec_error", 0);
            emitter.instruction("ldp x29, x30, [sp], #16");                     // restore frame pointer and return address
            emitter.instruction("ret");                                         // return the shim status in w0
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // keep the C call 16-byte aligned
            emitter.instruction("mov rbp, rsp");                                // establish the wrapper frame
            emitter.bl_c(shim);                                                 // call the opaque PCRE2 shim with the caller's arguments
            emitter.instruction("xor ecx, ecx");                                // stage PREG_NO_ERROR as this call's outcome
            emitter.instruction("test eax, eax");                               // did the shim succeed?
            emitter.instruction(&format!("jz {outcome}"));                      // success leaves the error state alone
            emitter.instruction(&format!("cmp eax, {}", PCRE2_SHIM_NOMATCH));   // is this an ordinary failed match?
            emitter.instruction(&format!("je {outcome}"));                      // a failed match is not an error
            for (status, code) in PCRE2_SHIM_STATUS_ERRORS {
                emitter.instruction(&format!("mov ecx, {code}"));               // stage the PHP code for this shim status
                emitter.instruction(&format!("cmp eax, {status}"));             // did PCRE2 fail with this error category?
                emitter.instruction(&format!("je {record}"));                   // record the staged PREG_*_ERROR code
            }
            emitter.instruction(&format!("mov ecx, {}", PREG_INTERNAL_ERROR));  // every other failure is an internal error
            emitter.label(&record);
            abi::emit_store_reg_to_symbol(emitter, "rcx", "_preg_last_error", 0);
            emitter.label(&outcome);
            abi::emit_store_reg_to_symbol(emitter, "rcx", "_preg_exec_error", 0);
            emitter.instruction("pop rbp");                                     // restore the caller frame pointer
            emitter.instruction("ret");                                         // return the shim status in eax
        }
    }
}

/// Emits a call to `__rt_preg_exec` in place of a direct `elephc_pcre2_v1_exec` call.
pub(crate) fn emit_exec_call(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("bl __rt_preg_exec"),              // execute through the error-recording wrapper
        Arch::X86_64 => emitter.instruction("call __rt_preg_exec"),             // execute through the error-recording wrapper
    }
}
