//! Purpose:
//! Emits the `__rt_curl_*` helpers whose whole job is to forward already-marshalled C
//! arguments to one `elephc_curl` entry point and hand back its integer answer:
//! `__rt_curl_easy_setopt_long`, `__rt_curl_easy_setopt_str`, `__rt_curl_easy_perform`,
//! `__rt_curl_option_kind`, and `__rt_curl_easy_errno`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::curl::emit_curl`.
//!
//! Key details:
//! - THE LOWERINGS MARSHAL, THESE HELPERS FORWARD. Each helper's inputs are already in
//!   the target's C argument registers when it is entered, in the exact order the C entry
//!   point declares, so the body is a slot probe plus an indirect call. The helpers exist
//!   at all (rather than the lowerings calling the bridge directly) so the null-slot
//!   fallback, the return-width fix-up, and the frame handling are written ONCE per
//!   target instead of once per call site.
//! - RETURN WIDTH IS NOT COSMETIC. The C entry points return `int32_t`; both ABIs leave
//!   the upper 32 bits of the return register unspecified. Three of these are booleans
//!   and are zero-extended; `curl_errno` is a signed `CURLcode` and is sign-extended.
//!   Skipping the extension leaves PHP comparing against garbage in the high half.
//! - A null slot answers `0` for all of them: `false` for the booleans, `CURLE_OK` for
//!   `curl_errno`, and `KIND_INVALID` for `curl_option_kind`, i.e. "nothing happened",
//!   never a fabricated success. `KIND_INVALID` makes the prelude raise `ValueError`,
//!   which is the loudest of the three and the right answer for a missing bridge.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

use super::slots::{
    emit_call_entry, emit_load_entry_or_branch, emit_sign_extend_int_result,
    emit_zero_extend_int_result,
};

/// How a helper widens the C `int32_t` its bridge entry point returns.
#[derive(Clone, Copy, PartialEq, Eq)]
enum IntResult {
    /// A `0`/`1` acceptance flag.
    Boolean,
    /// A signed `CURLcode`.
    CurlCode,
}

/// Emits every forwarding helper for the target.
pub(crate) fn emit_curl_easy_scalar_helpers(emitter: &mut Emitter) {
    emit_forwarder(
        emitter,
        "__rt_curl_easy_setopt_long",
        "_elephc_curl_easy_setopt_long_fn",
        "curl_easy_setopt_long (apply an integer-valued libcurl option)",
        IntResult::Boolean,
    );
    emit_forwarder(
        emitter,
        "__rt_curl_easy_setopt_str",
        "_elephc_curl_easy_setopt_str_fn",
        "curl_easy_setopt_str (apply a string-valued libcurl option)",
        IntResult::Boolean,
    );
    emit_forwarder(
        emitter,
        "__rt_curl_easy_setopt_slist",
        "_elephc_curl_easy_setopt_slist_fn",
        "curl_easy_setopt_slist (apply a string-list libcurl option)",
        IntResult::Boolean,
    );
    emit_forwarder(
        emitter,
        "__rt_curl_easy_perform",
        "_elephc_curl_easy_perform_fn",
        "curl_easy_perform (run the configured transfer)",
        IntResult::Boolean,
    );
    emit_forwarder(
        emitter,
        "__rt_curl_option_kind",
        "_elephc_curl_option_kind_fn",
        "curl_option_kind (classify a curl_setopt option number)",
        IntResult::Boolean,
    );
    emit_forwarder(
        emitter,
        "__rt_curl_easy_reset",
        "_elephc_curl_easy_reset_fn",
        "curl_easy_reset (reset every option on an easy handle)",
        IntResult::Boolean,
    );
    emit_forwarder(
        emitter,
        "__rt_curl_easy_upkeep",
        "_elephc_curl_easy_upkeep_fn",
        "curl_easy_upkeep (run connection upkeep on an easy handle)",
        IntResult::Boolean,
    );
    emit_forwarder(
        emitter,
        "__rt_curl_easy_pause",
        "_elephc_curl_easy_pause_fn",
        "curl_easy_pause (apply a CURLPAUSE bitmask)",
        IntResult::CurlCode,
    );
    emit_forwarder(
        emitter,
        "__rt_curl_easy_errno",
        "_elephc_curl_easy_errno_fn",
        "curl_easy_errno (report the last CURLcode)",
        IntResult::CurlCode,
    );
}

/// Emits one forwarding helper: probe the slot, call it with the arguments already in the
/// C argument registers, widen the answer, return. `0` when the slot is null.
fn emit_forwarder(
    emitter: &mut Emitter,
    label: &str,
    slot: &str,
    description: &str,
    result: IntResult,
) {
    let missing = format!("{label}_missing");
    emitter.blank();
    emitter.comment(&format!("--- runtime: {description} ---"));
    emitter.label_global(label);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #16");                             // frame to preserve the link register across the indirect call

            emitter.instruction("stp x29, x30, [sp]");                          // save frame pointer and return address

            emitter.instruction("mov x29, sp");                                 // set the frame pointer

            // The probe only touches the entry scratch register, so the C arguments the
            // lowering left in x0-x3 are still live when the call is made.
            emit_load_entry_or_branch(emitter, slot, &missing);
            emit_call_entry(emitter);
            match result {
                IntResult::Boolean => emit_zero_extend_int_result(emitter),
                IntResult::CurlCode => emit_sign_extend_int_result(emitter),
            }
            emitter.instruction("ldp x29, x30, [sp]");                          // restore frame pointer and return address

            emitter.instruction("add sp, sp, #16");                             // release the frame

            emitter.instruction("ret");                                         // return the widened answer

            emitter.label(&missing);
            emitter.instruction("mov x0, #0");                                  // bridge not linked -> false / CURLE_OK

            emitter.instruction("ldp x29, x30, [sp]");                          // restore frame pointer and return address

            emitter.instruction("add sp, sp, #16");                             // release the frame

            emitter.instruction("ret");                                         // return the fail-closed answer

        }
        Arch::X86_64 => {
            let missing_x86 = format!("{missing}_x86");
            emitter.instruction("push rbp");                                    // preserve the caller frame pointer

            emitter.instruction("mov rbp, rsp");                                // establish the frame base

            emitter.instruction("sub rsp, 16");                                 // keep the indirect call 16-byte aligned

            // The probe only touches the entry scratch register, so the C arguments the
            // lowering left in rdi/rsi/rdx/rcx are still live when the call is made.
            emit_load_entry_or_branch(emitter, slot, &missing_x86);
            emit_call_entry(emitter);
            match result {
                IntResult::Boolean => emit_zero_extend_int_result(emitter),
                IntResult::CurlCode => emit_sign_extend_int_result(emitter),
            }
            emitter.instruction("mov rsp, rbp");                                // release the frame

            emitter.instruction("pop rbp");                                     // restore the caller frame pointer

            emitter.instruction("ret");                                         // return the widened answer

            emitter.label(&missing_x86);
            emitter.instruction("xor eax, eax");                                // bridge not linked -> false / CURLE_OK

            emitter.instruction("mov rsp, rbp");                                // release the frame

            emitter.instruction("pop rbp");                                     // restore the caller frame pointer

            emitter.instruction("ret");                                         // return the fail-closed answer

        }
    }
}
