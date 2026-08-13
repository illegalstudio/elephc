//! Purpose:
//! Emits `__rt_curl_easy_error`, the runtime helper behind the internal
//! `__elephc_curl_easy_error` builtin: copies libcurl's own error message for the last
//! transfer into an owned PHP string.
//!
//! Called from:
//! - `crate::codegen_support::runtime::curl::emit_curl`.
//!
//! Key details:
//! - THE BUFFER SIZE IS NOT A GUESS. libcurl's `CURLOPT_ERRORBUFFER` contract fixes the
//!   message at at most `CURL_ERROR_SIZE` (256) bytes including the terminating NUL, and
//!   the bridge stores exactly that buffer per handle, so a 256-byte stack buffer can
//!   never be too small and no two-pass length probe is needed. The bridge still reports
//!   the true length through its out-parameter, and this helper still branches on the
//!   call's success flag, so a future larger message would produce an empty string rather
//!   than a truncated one.
//! - A HANDLE THAT NEVER FAILED REPORTS `""`. The bridge zeroes the error buffer before
//!   every transfer, so a previous failure's text can never resurface as a later
//!   success's — which is why `curl_error()` after a successful transfer is empty in PHP.
//! - The message is copied through `__rt_str_persist` for the same reason the response
//!   body is: the bridge's buffer is overwritten by the next transfer on that handle.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

use super::slots::{emit_call_entry, emit_load_entry_or_branch};

/// `CURL_ERROR_SIZE`: libcurl's fixed maximum error-message buffer, including the
/// terminating NUL. The bridge allocates exactly this per handle
/// (`crates/elephc-curl/src/handles.rs`), so it is also the exact size this helper needs.
const CURL_ERROR_SIZE: u32 = 256;

/// `__rt_curl_easy_error` — in: handle id in `x0`/`rdi`. Out: PHP string (AArch64 `x1`/`x2`,
/// x86_64 `rax`/`rdx`).
pub(crate) fn emit_curl_easy_error(emitter: &mut Emitter) {
    // 256-byte message buffer + an 8-byte length out-parameter + the 16-byte frame,
    // rounded to the 16-byte stack alignment both ABIs require.
    let frame = CURL_ERROR_SIZE + 32;
    emitter.blank();
    emitter.comment("--- runtime: curl_easy_error (copy libcurl's last error message) ---");
    emitter.label_global("__rt_curl_easy_error");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("sub sp, sp, #{frame}"));              // error buffer, length slot, and frame

            emitter.instruction(&format!("stp x29, x30, [sp, #{}]", frame - 16)); // save frame pointer and return address

            emitter.instruction(&format!("add x29, sp, #{}", frame - 16));      // set the frame pointer

            emitter.instruction(&format!("str xzr, [sp, #{CURL_ERROR_SIZE}]")); // clear the message length out-parameter

            emitter.instruction("mov x1, sp");                                  // C ABI out = the stack message buffer

            emitter.instruction(&format!("mov x2, #{CURL_ERROR_SIZE}"));        // C ABI out_cap = CURL_ERROR_SIZE

            emitter.instruction(&format!("add x3, sp, #{CURL_ERROR_SIZE}"));    // C ABI out_len = &message_length

            emit_load_entry_or_branch(
                emitter,
                "_elephc_curl_easy_error_fn",
                "__rt_curl_easy_error_empty",
            );
            emit_call_entry(emitter);                                           // elephc_curl_easy_error(id, out, cap, &len)

            // `w0`, NOT `x0`: the bridge returns a C `int32_t` and AAPCS64 leaves the
            // upper 32 bits of the return register unspecified. Branching on the full
            // 64-bit value could take the SUCCESS path on a failed call and then persist
            // whatever length the out-parameter holds — an out-of-bounds read past this
            // frame's 256-byte buffer straight into a PHP string.
            emitter.instruction("cbz w0, __rt_curl_easy_error_empty");          // unknown id or oversized message -> empty string

            emitter.instruction(&format!("ldr x2, [sp, #{CURL_ERROR_SIZE}]"));  // reload the message length

            emitter.instruction("mov x1, sp");                                  // the message bytes live in the stack buffer

            emitter.instruction("b __rt_curl_easy_error_persist");              // copy them into owned storage

            emitter.label("__rt_curl_easy_error_empty");
            emitter.instruction("mov x1, sp");                                  // a valid address so the zero-length copy has a source

            emitter.instruction("mov x2, #0");                                  // no message is an empty PHP string

            emitter.label("__rt_curl_easy_error_persist");
            emitter.instruction("bl __rt_str_persist");                         // own the bytes; the stack buffer dies with this frame

            emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", frame - 16)); // restore frame pointer and return address

            emitter.instruction(&format!("add sp, sp, #{frame}"));              // release the frame

            emitter.instruction("ret");                                         // return the owned message string

        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // preserve the caller frame pointer

            emitter.instruction("mov rbp, rsp");                                // establish the frame base

            emitter.instruction(&format!("sub rsp, {frame}"));                  // error buffer plus length slot, 16-byte aligned

            emitter.instruction(&format!("mov QWORD PTR [rbp - {}], 0", CURL_ERROR_SIZE + 8)); // clear the message length out-parameter

            emitter.instruction(&format!("lea rsi, [rbp - {CURL_ERROR_SIZE}]")); // C ABI out = the stack message buffer

            emitter.instruction(&format!("mov edx, {CURL_ERROR_SIZE}"));        // C ABI out_cap = CURL_ERROR_SIZE

            emitter.instruction(&format!("lea rcx, [rbp - {}]", CURL_ERROR_SIZE + 8)); // C ABI out_len = &message_length

            emit_load_entry_or_branch(
                emitter,
                "_elephc_curl_easy_error_fn",
                "__rt_curl_easy_error_empty_x86",
            );
            emit_call_entry(emitter);                                           // elephc_curl_easy_error(id, out, cap, &len)

            emitter.instruction("test eax, eax");                               // unknown id or oversized message?

            emitter.instruction("jz __rt_curl_easy_error_empty_x86");           // -> empty string

            emitter.instruction(&format!("mov rdx, QWORD PTR [rbp - {}]", CURL_ERROR_SIZE + 8)); // reload the message length

            emitter.instruction(&format!("lea rax, [rbp - {CURL_ERROR_SIZE}]")); // the message bytes live in the stack buffer

            emitter.instruction("jmp __rt_curl_easy_error_persist_x86");        // copy them into owned storage

            emitter.label("__rt_curl_easy_error_empty_x86");
            emitter.instruction("mov rax, rbp");                                // a valid address so the zero-length copy has a source

            emitter.instruction("xor edx, edx");                                // no message is an empty PHP string

            emitter.label("__rt_curl_easy_error_persist_x86");
            emitter.instruction("call __rt_str_persist");                       // own the bytes; the stack buffer dies with this frame

            emitter.instruction("mov rsp, rbp");                                // release the frame

            emitter.instruction("pop rbp");                                     // restore the caller frame pointer

            emitter.instruction("ret");                                         // return the owned message string

        }
    }
}
