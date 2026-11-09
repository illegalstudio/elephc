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
//! - EIR lowering clears `_preg_last_error` before each `preg_*` call and records
//!   `PREG_INTERNAL_ERROR` itself when the call reports an invalid pattern; the exec
//!   wrapper only ever records a match failure.
//! - The opaque shim folds PCRE2 match errors into POSIX codes: `REG_ESPACE` (match,
//!   heap or memory limit) is reported as `PREG_BACKTRACK_LIMIT_ERROR`, `REG_INVARG`
//!   (PCRE2's UTF-8 and argument errors) as `PREG_BAD_UTF8_ERROR`, any other non-match
//!   status as `PREG_INTERNAL_ERROR`. `REG_NOMATCH` is not an error.
//! - The wrapper preserves the shim's ABI: arguments pass through untouched and the
//!   status comes back in `w0` / `eax`.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

const PREG_INTERNAL_ERROR: i64 = 1;
const PREG_BACKTRACK_LIMIT_ERROR: i64 = 2;
const PREG_BAD_UTF8_ERROR: i64 = 4;
const REG_ESPACE: i64 = 14;
const REG_INVARG: i64 = 16;
const REG_NOMATCH: i64 = 17;

/// PHP 8.5's `preg_last_error_msg()` strings, indexed by `PREG_*_ERROR` code.
const MESSAGES: &[&str] = &[
    "No error",
    "Internal error",
    "Backtrack limit exhausted",
    "Recursion limit exhausted",
    "Malformed UTF-8 characters, possibly incorrectly encoded",
    "The offset did not correspond to the beginning of a valid UTF-8 code point",
    "JIT stack limit exhausted",
];

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
            emitter.instruction("jb __rt_preg_last_error_msg_index_linux_x86_64"); // in-range codes index the table directly
            emitter.instruction("xor ecx, ecx");                                // out-of-range codes read as PREG_NO_ERROR
            emitter.label("__rt_preg_last_error_msg_index_linux_x86_64");
            abi::emit_symbol_address(emitter, "r9", "_preg_err_msg_table");
            emitter.instruction("shl rcx, 4");                                  // step over one 16-byte (ptr, len) entry per code
            emitter.instruction("add r9, rcx");                                 // address the entry for this code
            emitter.instruction("mov rax, QWORD PTR [r9]");                     // return the message pointer
            emitter.instruction("mov rdx, QWORD PTR [r9 + 8]");                 // return the message length
            emitter.instruction("ret");                                         // return the borrowed static message
        }
    }
}

/// Emits `__rt_preg_exec`: calls the shim's match entry point and records a failing
/// status (other than an ordinary failed match) in `_preg_last_error`.
pub(crate) fn emit_preg_exec_wrapper(emitter: &mut Emitter) {
    let label = "__rt_preg_exec";
    let shim = "elephc_pcre2_v1_exec";
    let done = format!("{label}_done");
    let store = format!("{label}_store");
    emitter.blank();
    emitter.comment("--- runtime: preg_exec ---");
    emitter.label_global(label);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x29, x30, [sp, #-16]!");                   // save frame pointer and return address
            emitter.instruction("mov x29, sp");                                 // establish the wrapper frame
            emitter.bl_c(shim);                                                 // call the opaque PCRE2 shim with the caller's arguments
            emitter.instruction(&format!("cbz w0, {done}"));                    // success leaves the error state alone
            emitter.instruction(&format!("cmp w0, #{}", REG_NOMATCH));          // is this an ordinary failed match?
            emitter.instruction(&format!("b.eq {done}"));                       // a failed match is not an error
            emitter.instruction(&format!("mov x9, #{}", PREG_BACKTRACK_LIMIT_ERROR)); // stage the match-limit error code
            emitter.instruction(&format!("cmp w0, #{}", REG_ESPACE));           // did PCRE2 exhaust its match or heap budget?
            emitter.instruction(&format!("b.eq {store}"));                      // record PREG_BACKTRACK_LIMIT_ERROR
            emitter.instruction(&format!("mov x9, #{}", PREG_BAD_UTF8_ERROR));  // stage the malformed-UTF-8 error code
            emitter.instruction(&format!("cmp w0, #{}", REG_INVARG));           // did PCRE2 reject the subject?
            emitter.instruction(&format!("b.eq {store}"));                      // record PREG_BAD_UTF8_ERROR
            emitter.instruction(&format!("mov x9, #{}", PREG_INTERNAL_ERROR));  // every other failure is an internal error
            emitter.label(&store);
            abi::emit_store_reg_to_symbol(emitter, "x9", "_preg_last_error", 0);
            emitter.label(&done);
            emitter.instruction("ldp x29, x30, [sp], #16");                     // restore frame pointer and return address
            emitter.instruction("ret");                                         // return the shim status in w0
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // keep the C call 16-byte aligned
            emitter.instruction("mov rbp, rsp");                                // establish the wrapper frame
            emitter.bl_c(shim);                                                 // call the opaque PCRE2 shim with the caller's arguments
            emitter.instruction("test eax, eax");                               // did the shim succeed?
            emitter.instruction(&format!("jz {done}"));                         // success leaves the error state alone
            emitter.instruction(&format!("cmp eax, {}", REG_NOMATCH));          // is this an ordinary failed match?
            emitter.instruction(&format!("je {done}"));                         // a failed match is not an error
            emitter.instruction(&format!("mov ecx, {}", PREG_BACKTRACK_LIMIT_ERROR)); // stage the match-limit error code
            emitter.instruction(&format!("cmp eax, {}", REG_ESPACE));           // did PCRE2 exhaust its match or heap budget?
            emitter.instruction(&format!("je {store}"));                        // record PREG_BACKTRACK_LIMIT_ERROR
            emitter.instruction(&format!("mov ecx, {}", PREG_BAD_UTF8_ERROR));  // stage the malformed-UTF-8 error code
            emitter.instruction(&format!("cmp eax, {}", REG_INVARG));           // did PCRE2 reject the subject?
            emitter.instruction(&format!("je {store}"));                        // record PREG_BAD_UTF8_ERROR
            emitter.instruction(&format!("mov ecx, {}", PREG_INTERNAL_ERROR));  // every other failure is an internal error
            emitter.label(&store);
            abi::emit_store_reg_to_symbol(emitter, "rcx", "_preg_last_error", 0);
            emitter.label(&done);
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

