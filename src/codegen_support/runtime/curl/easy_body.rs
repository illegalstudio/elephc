//! Purpose:
//! Emits `__rt_curl_easy_body`, the runtime helper behind the internal
//! `__elephc_curl_easy_body` builtin: takes the `CURLOPT_RETURNTRANSFER`-captured
//! response body from a handle and copies it into an owned PHP string.
//!
//! Called from:
//! - `crate::codegen_support::runtime::curl::emit_curl`.
//!
//! Key details:
//! - THE COPY IS MANDATORY, NOT AN OPTIMIZATION MISS. `elephc_curl_easy_take_body` hands
//!   back a pointer into the bridge's own buffer that stays valid only until the next
//!   `elephc_curl_*` call touching the same handle (the "borrowed until overwritten"
//!   convention the whole bridge family uses). PHP strings outlive the handle, so the
//!   bytes go through `__rt_str_persist` immediately, before this helper returns.
//! - THE `ptr` OUT-PARAMETER IS ONLY WRITTEN ON SUCCESS. For an unknown/already-freed id
//!   the bridge writes `len = 0` and returns `0` without touching `ptr`, so the length is
//!   what this helper branches on and the pointer is never read with a nonzero length it
//!   did not come with. The zero-length path hands `__rt_str_persist` a valid stack
//!   address with length `0`, which yields an owned empty block — the same representation
//!   every other empty PHP string uses, never a synthesized null pair.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

use super::slots::{emit_call_entry, emit_load_entry_or_branch};

/// `__rt_curl_easy_body` — in: handle id in `x0`/`rdi`. Out: PHP string (AArch64 `x1`/`x2`,
/// x86_64 `rax`/`rdx`).
pub(crate) fn emit_curl_easy_body(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: curl_easy_body (take the captured response body) ---");
    emitter.label_global("__rt_curl_easy_body");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #32");                             // out-parameter slots plus the frame

            emitter.instruction("stp x29, x30, [sp, #16]");                     // save frame pointer and return address

            emitter.instruction("add x29, sp, #16");                            // set the frame pointer

            emitter.instruction("str xzr, [sp]");                               // clear the body pointer out-parameter

            emitter.instruction("str xzr, [sp, #8]");                           // clear the body length out-parameter

            emitter.instruction("mov x1, sp");                                  // C ABI ptr = &body_pointer

            emitter.instruction("add x2, sp, #8");                              // C ABI len = &body_length

            emit_load_entry_or_branch(
                emitter,
                "_elephc_curl_easy_take_body_fn",
                "__rt_curl_easy_body_empty",
            );
            emit_call_entry(emitter);                                           // elephc_curl_easy_take_body(id, &ptr, &len)

            emitter.instruction("ldr x2, [sp, #8]");                            // reload the captured body length

            emitter.instruction("cbz x2, __rt_curl_easy_body_empty");           // nothing captured -> owned empty string

            emitter.instruction("ldr x1, [sp]");                                // reload the borrowed body pointer

            emitter.instruction("b __rt_curl_easy_body_persist");               // copy the borrowed bytes into owned storage

            emitter.label("__rt_curl_easy_body_empty");
            emitter.instruction("mov x1, sp");                                  // a valid address so the zero-length copy has a source

            emitter.instruction("mov x2, #0");                                  // an empty captured body is an empty PHP string

            emitter.label("__rt_curl_easy_body_persist");
            emitter.instruction("bl __rt_str_persist");                         // own the bytes before the bridge can overwrite them

            emitter.instruction("ldp x29, x30, [sp, #16]");                     // restore frame pointer and return address

            emitter.instruction("add sp, sp, #32");                             // release the frame

            emitter.instruction("ret");                                         // return the owned body string

        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // preserve the caller frame pointer

            emitter.instruction("mov rbp, rsp");                                // establish the frame base

            emitter.instruction("sub rsp, 32");                                 // out-parameter slots, 16-byte aligned

            emitter.instruction("mov QWORD PTR [rbp - 8], 0");                  // clear the body pointer out-parameter

            emitter.instruction("mov QWORD PTR [rbp - 16], 0");                 // clear the body length out-parameter

            emitter.instruction("lea rsi, [rbp - 8]");                          // C ABI ptr = &body_pointer

            emitter.instruction("lea rdx, [rbp - 16]");                         // C ABI len = &body_length

            emit_load_entry_or_branch(
                emitter,
                "_elephc_curl_easy_take_body_fn",
                "__rt_curl_easy_body_empty_x86",
            );
            emit_call_entry(emitter);                                           // elephc_curl_easy_take_body(id, &ptr, &len)

            emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");               // reload the captured body length

            emitter.instruction("test rdx, rdx");                               // nothing captured?

            emitter.instruction("jz __rt_curl_easy_body_empty_x86");            // -> owned empty string

            emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                // reload the borrowed body pointer

            emitter.instruction("jmp __rt_curl_easy_body_persist_x86");         // copy the borrowed bytes into owned storage

            emitter.label("__rt_curl_easy_body_empty_x86");
            emitter.instruction("mov rax, rbp");                                // a valid address so the zero-length copy has a source

            emitter.instruction("xor edx, edx");                                // an empty captured body is an empty PHP string

            emitter.label("__rt_curl_easy_body_persist_x86");
            emitter.instruction("call __rt_str_persist");                       // own the bytes before the bridge can overwrite them

            emitter.instruction("mov rsp, rbp");                                // release the frame

            emitter.instruction("pop rbp");                                     // restore the caller frame pointer

            emitter.instruction("ret");                                         // return the owned body string

        }
    }
}
