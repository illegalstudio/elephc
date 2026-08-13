//! Purpose:
//! Emits `__rt_curl_easy_init`, the runtime helper behind the internal
//! `__elephc_curl_easy_init` builtin: allocates a libcurl easy handle and boxes its id as
//! a resource-kind-6 Mixed cell, or hands back PHP `false`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::curl::emit_curl`.
//!
//! Key details:
//! - RESOURCE KIND 6 IS WHAT OWNS THE HANDLE. The boxed cell is `tag 9` (resource) with
//!   the bridge's `i64` id in the low payload word and `6` in the high one, which is the
//!   key `__rt_mixed_free_deep` dispatches on to reach `__rt_curl_easy_free`. Kind 6 is
//!   also excluded from resource-id binding in `__rt_mixed_from_value`, because PHP 8
//!   counts a `CurlHandle` in the OBJECT handle space: binding an id here would shift
//!   every `fopen()` id in a curl-using program by one.
//! - `false` IS A REAL PHP FALSE, boxed as `tag 3` with a zero payload, so the prelude's
//!   `$raw === false` identity check works without any special-casing. This is the
//!   failure answer PHP's own `curl_init()` gives.
//! - The bridge already installs its write callback and error buffer inside
//!   `elephc_curl_easy_init`, so a handle is fully usable the moment this returns; there
//!   is deliberately no second setup call to forget.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

use super::slots::{emit_call_entry, emit_load_entry_or_branch};

/// `__rt_curl_easy_init` — in: nothing. Out: Mixed in `x0`/`rax` (a boxed resource-kind-6
/// handle, or boxed PHP `false`).
pub(crate) fn emit_curl_easy_init(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: curl_easy_init (allocate a libcurl easy handle) ---");
    emitter.label_global("__rt_curl_easy_init");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #16");                             // frame to preserve the link register across calls

            emitter.instruction("stp x29, x30, [sp]");                          // save frame pointer and return address

            emitter.instruction("mov x29, sp");                                 // set the frame pointer

            emit_load_entry_or_branch(
                emitter,
                "_elephc_curl_easy_init_fn",
                "__rt_curl_easy_init_false",
            );
            emit_call_entry(emitter);                                           // elephc_curl_easy_init(), x0 = handle id (0 on failure)

            emitter.instruction("cbz x0, __rt_curl_easy_init_false");           // id 0 = libcurl could not allocate -> PHP false

            emitter.instruction("mov x1, x0");                                  // Mixed payload = the bridge handle id

            emitter.instruction("mov x2, #6");                                  // resource kind 6 = CurlHandle (high payload word)

            emitter.instruction("mov x0, #9");                                  // runtime tag 9 = resource

            emitter.instruction("bl __rt_mixed_from_value");                    // box the id as the cell that owns the handle

            emitter.instruction("ldp x29, x30, [sp]");                          // restore frame pointer and return address

            emitter.instruction("add sp, sp, #16");                             // release the frame

            emitter.instruction("ret");                                         // return the boxed CurlHandle payload

            emitter.label("__rt_curl_easy_init_false");
            emitter.instruction("mov x1, #0");                                  // boolean payload 0 = PHP false

            emitter.instruction("mov x2, #0");                                  // booleans carry no high payload word

            emitter.instruction("mov x0, #3");                                  // runtime tag 3 = boolean

            emitter.instruction("bl __rt_mixed_from_value");                    // box PHP false, the failure answer curl_init() gives

            emitter.instruction("ldp x29, x30, [sp]");                          // restore frame pointer and return address

            emitter.instruction("add sp, sp, #16");                             // release the frame

            emitter.instruction("ret");                                         // return boxed PHP false

        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // preserve the caller frame pointer

            emitter.instruction("mov rbp, rsp");                                // establish the frame base

            emitter.instruction("sub rsp, 16");                                 // keep nested calls 16-byte aligned

            emit_load_entry_or_branch(
                emitter,
                "_elephc_curl_easy_init_fn",
                "__rt_curl_easy_init_false_x86",
            );
            emit_call_entry(emitter);                                           // elephc_curl_easy_init(), rax = handle id (0 on failure)

            emitter.instruction("test rax, rax");                               // id 0 = libcurl could not allocate

            emitter.instruction("jz __rt_curl_easy_init_false_x86");            // -> PHP false

            emitter.instruction("mov rdi, rax");                                // Mixed payload = the bridge handle id

            emitter.instruction("mov esi, 6");                                  // resource kind 6 = CurlHandle (high payload word)

            emitter.instruction("mov eax, 9");                                  // runtime tag 9 = resource

            emitter.instruction("call __rt_mixed_from_value");                  // box the id as the cell that owns the handle

            emitter.instruction("mov rsp, rbp");                                // release the frame

            emitter.instruction("pop rbp");                                     // restore the caller frame pointer

            emitter.instruction("ret");                                         // return the boxed CurlHandle payload

            emitter.label("__rt_curl_easy_init_false_x86");
            emitter.instruction("xor edi, edi");                                // boolean payload 0 = PHP false

            emitter.instruction("xor esi, esi");                                // booleans carry no high payload word

            emitter.instruction("mov eax, 3");                                  // runtime tag 3 = boolean

            emitter.instruction("call __rt_mixed_from_value");                  // box PHP false, the failure answer curl_init() gives

            emitter.instruction("mov rsp, rbp");                                // release the frame

            emitter.instruction("pop rbp");                                     // restore the caller frame pointer

            emitter.instruction("ret");                                         // return boxed PHP false

        }
    }
}
