//! Purpose:
//! Emits `__rt_curl_easy_free`, the single destructor for a libcurl easy handle: it runs
//! `curl_easy_cleanup` through the bridge when a resource-kind-6 Mixed cell is released.
//!
//! Called from:
//! - `crate::codegen_support::runtime::curl::emit_curl`.
//! - `__rt_mixed_free_deep`'s resource-kind ladder (kind 6), which is the ONLY caller
//!   that matters: it is what makes a `CurlHandle` object's ordinary teardown close the
//!   transfer's socket and free libcurl's state.
//!
//! Key details:
//! - THERE IS EXACTLY ONE FREE PATH. `curl_close()` is a no-op in PHP 8 and is a no-op in
//!   `crate::curl_prelude` too, and `CurlHandle` deliberately has no `__destruct`, so the
//!   Mixed cell that boxes the handle id is the only owner and this helper is the only
//!   release. Adding a second one would double-free.
//! - A `0` PAYLOAD IS SKIPPED. Id `0` is the bridge's "allocation failed" answer, which
//!   `__rt_curl_easy_init` turns into PHP `false` rather than a boxed handle, so a
//!   kind-6 cell should never carry it; the check costs one instruction and keeps a
//!   malformed cell from reaching the bridge.
//! - THE BRIDGE ITSELF TOLERATES A REPEATED FREE (ids are never reused, so an unknown id
//!   is a no-op rather than a double `curl_easy_cleanup`), which is what makes this
//!   helper safe to reach from any release path without extra bookkeeping.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

use super::slots::{emit_call_entry, emit_load_entry_or_branch};

/// `__rt_curl_easy_free` — in: handle id in `x0`/`rdi`. Out: nothing.
pub(crate) fn emit_curl_easy_free(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: curl_easy_free (release a libcurl easy handle) ---");
    emitter.label_global("__rt_curl_easy_free");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #16");                             // frame to preserve the link register across the call

            emitter.instruction("stp x29, x30, [sp]");                          // save frame pointer and return address

            emitter.instruction("mov x29, sp");                                 // set the frame pointer

            emitter.instruction("cbz x0, __rt_curl_easy_free_done");            // id 0 never names a live handle

            emit_load_entry_or_branch(
                emitter,
                "_elephc_curl_easy_free_fn",
                "__rt_curl_easy_free_done",
            );
            emit_call_entry(emitter);                                           // elephc_curl_easy_free(id) — curl_easy_cleanup

            emitter.label("__rt_curl_easy_free_done");
            emitter.instruction("ldp x29, x30, [sp]");                          // restore frame pointer and return address

            emitter.instruction("add sp, sp, #16");                             // release the frame

            emitter.instruction("ret");                                         // return

        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // preserve the caller frame pointer

            emitter.instruction("mov rbp, rsp");                                // establish the frame base

            emitter.instruction("sub rsp, 16");                                 // keep the nested call 16-byte aligned

            emitter.instruction("test rdi, rdi");                               // id 0 never names a live handle

            emitter.instruction("jz __rt_curl_easy_free_done_x86");             // skip the release for a malformed cell

            emit_load_entry_or_branch(
                emitter,
                "_elephc_curl_easy_free_fn",
                "__rt_curl_easy_free_done_x86",
            );
            emit_call_entry(emitter);                                           // elephc_curl_easy_free(id) — curl_easy_cleanup

            emitter.label("__rt_curl_easy_free_done_x86");
            emitter.instruction("mov rsp, rbp");                                // release the frame

            emitter.instruction("pop rbp");                                     // restore the caller frame pointer

            emitter.instruction("ret");                                         // return

        }
    }
}
