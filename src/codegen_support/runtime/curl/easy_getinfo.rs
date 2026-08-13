//! Purpose:
//! Emits `__rt_curl_easy_getinfo_long`, the runtime helper behind the internal
//! `__elephc_curl_easy_getinfo_long` builtin: reads a `CURLINFO_LONG`-typed info field
//! (`CURLINFO_HTTP_CODE` only, forwarded by `curl_getinfo()`) into a boxed PHP `int`, or
//! boxed `false` when the bridge could not answer.
//!
//! Called from:
//! - `crate::codegen_support::runtime::curl::emit_curl`.
//!
//! Key details:
//! - THE OUT-PARAMETER LIVES ON THE STACK, not in a register: `elephc_curl_easy_getinfo_long`
//!   writes the fetched `long` through a caller-owned `*mut i64`, the same out-param shape
//!   `elephc_curl_easy_take_body`/`_error` already use, so this helper reserves one 8-byte
//!   slot, computes its address as the THIRD C argument itself (the lowering only marshals
//!   the handle id and the `CURLINFO_*` option into the first two), and reloads it only
//!   after a successful call.
//! - THE STATUS RETURN IS A C `int32_t`, so the failure branch reads the low 32 bits
//!   (`cbz w0` / `test eax, eax`), exactly like `__rt_curl_easy_error`/`__rt_curl_version` —
//!   branching on the full 64-bit register would trust an unspecified upper half and could
//!   take the success path on a genuine failure, persisting whatever garbage the
//!   zero-initialized out-parameter slot happened to hold.
//! - `false` ON FAILURE, an `int` ON SUCCESS: both are boxed through `__rt_mixed_from_value`
//!   (tag 0 = int, tag 3 = boolean), matching `curl_getinfo()`'s documented `int|false`
//!   answer for `CURLINFO_HTTP_CODE`.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

use super::slots::{emit_call_entry, emit_load_entry_or_branch};

/// `__rt_curl_easy_getinfo_long` — in: handle id in `x0`/`rdi`, `CURLINFO_*` option in
/// `x1`/`rsi`. Out: boxed Mixed in `x0`/`rax` (an `int`, or boxed PHP `false`).
pub(crate) fn emit_curl_easy_getinfo_long(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: curl_easy_getinfo_long (read a long-typed CURLINFO field) ---");
    emitter.label_global("__rt_curl_easy_getinfo_long");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #32");                             // out-parameter slot plus the frame

            emitter.instruction("stp x29, x30, [sp, #16]");                     // save frame pointer and return address

            emitter.instruction("add x29, sp, #16");                            // set the frame pointer

            emitter.instruction("str xzr, [sp]");                               // clear the out-parameter slot

            emitter.instruction("mov x2, sp");                                  // C ABI out = &value

            emit_load_entry_or_branch(
                emitter,
                "_elephc_curl_easy_getinfo_long_fn",
                "__rt_curl_easy_getinfo_long_false",
            );
            emit_call_entry(emitter);                                           // elephc_curl_easy_getinfo_long(id, info, &value)

            // `w0`, not `x0`: the bridge returns a C `int32_t` and AAPCS64 leaves the
            // upper 32 bits of the return register unspecified.
            emitter.instruction("cbz w0, __rt_curl_easy_getinfo_long_false");    // unknown id, wrong info type, or getinfo failure -> PHP false

            emitter.instruction("ldr x1, [sp]");                                // Mixed payload = the fetched long value

            emitter.instruction("mov x2, #0");                                  // ints carry no high payload word

            emitter.instruction("mov x0, #0");                                  // runtime tag 0 = int

            emitter.instruction("bl __rt_mixed_from_value");                    // box the fetched value

            emitter.instruction("ldp x29, x30, [sp, #16]");                     // restore frame pointer and return address

            emitter.instruction("add sp, sp, #32");                             // release the frame

            emitter.instruction("ret");                                         // return the boxed int

            emitter.label("__rt_curl_easy_getinfo_long_false");
            emitter.instruction("mov x1, #0");                                  // boolean payload 0 = PHP false

            emitter.instruction("mov x2, #0");                                  // booleans carry no high payload word

            emitter.instruction("mov x0, #3");                                  // runtime tag 3 = boolean

            emitter.instruction("bl __rt_mixed_from_value");                    // box PHP false, the failure answer curl_getinfo() gives

            emitter.instruction("ldp x29, x30, [sp, #16]");                     // restore frame pointer and return address

            emitter.instruction("add sp, sp, #32");                             // release the frame

            emitter.instruction("ret");                                         // return boxed PHP false

        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // preserve the caller frame pointer

            emitter.instruction("mov rbp, rsp");                                // establish the frame base

            emitter.instruction("sub rsp, 16");                                 // out-parameter slot, 16-byte aligned

            emitter.instruction("mov QWORD PTR [rbp - 8], 0");                  // clear the out-parameter slot

            emitter.instruction("lea rdx, [rbp - 8]");                          // C ABI out = &value

            emit_load_entry_or_branch(
                emitter,
                "_elephc_curl_easy_getinfo_long_fn",
                "__rt_curl_easy_getinfo_long_false_x86",
            );
            emit_call_entry(emitter);                                           // elephc_curl_easy_getinfo_long(id, info, &value)

            emitter.instruction("test eax, eax");                               // unknown id, wrong info type, or getinfo failure?

            emitter.instruction("jz __rt_curl_easy_getinfo_long_false_x86");    // -> PHP false

            emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                // Mixed payload = the fetched long value

            emitter.instruction("xor esi, esi");                                // ints carry no high payload word

            emitter.instruction("mov eax, 0");                                  // runtime tag 0 = int

            emitter.instruction("call __rt_mixed_from_value");                  // box the fetched value

            emitter.instruction("mov rsp, rbp");                                // release the frame

            emitter.instruction("pop rbp");                                     // restore the caller frame pointer

            emitter.instruction("ret");                                         // return the boxed int

            emitter.label("__rt_curl_easy_getinfo_long_false_x86");
            emitter.instruction("xor edi, edi");                                // boolean payload 0 = PHP false

            emitter.instruction("xor esi, esi");                                // booleans carry no high payload word

            emitter.instruction("mov eax, 3");                                  // runtime tag 3 = boolean

            emitter.instruction("call __rt_mixed_from_value");                  // box PHP false, the failure answer curl_getinfo() gives

            emitter.instruction("mov rsp, rbp");                                // release the frame

            emitter.instruction("pop rbp");                                     // restore the caller frame pointer

            emitter.instruction("ret");                                         // return boxed PHP false

        }
    }
}
