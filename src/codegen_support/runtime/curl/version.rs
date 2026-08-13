//! Purpose:
//! Emits `__rt_curl_version`, the runtime helper behind the internal
//! `__elephc_curl_version` builtin: reads the linked libcurl's `curl_version_info` data
//! out of the bridge as a JSON blob and hands it back as an owned PHP string.
//!
//! Called from:
//! - `crate::codegen_support::runtime::curl::emit_curl`.
//!
//! Key details:
//! - TWO PASSES, NOT A FIXED BUFFER. The blob's size depends on which protocols and
//!   sub-libraries this libcurl build reports, and unlike the error buffer there is no
//!   documented upper bound. The first call passes a null buffer with capacity `0` purely
//!   to learn the required length (the bridge always reports it through its out-parameter,
//!   even on the `0` "too small" return — that is the whole point of that convention), and
//!   the second call fills a stack buffer of exactly that size. A fixed buffer would either
//!   waste a kilobyte on every program or silently truncate a future libcurl's blob.
//! - THE BUFFER IS A STACK ALLOCA, rounded up to the 16-byte alignment both ABIs require.
//!   `sp`/`rsp` is restored from the frame pointer on EVERY exit path, including the
//!   fail-closed ones taken before the alloca happened, so the pointer arithmetic cannot
//!   leak stack.
//! - THE ANSWER IS READ AT RUN TIME, from the library that is actually linked. Nothing
//!   here is baked in at compile time, which is what makes the pinned-version assertion in
//!   `tests/codegen/curl/easy_handle.rs` a real check on the managed native package rather
//!   than a tautology.
//! - `""` MEANS "no blob", and the prelude turns it into PHP's `false`. The three ways to
//!   get there — bridge not linked, a zero-length blob, a second call that still failed —
//!   are deliberately indistinguishable to PHP, because all three mean the same thing:
//!   this binary cannot describe its libcurl.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

use super::slots::{emit_call_entry, emit_load_entry_or_branch};

/// `__rt_curl_version` — in: nothing. Out: PHP string (AArch64 `x1`/`x2`, x86_64
/// `rax`/`rdx`) holding the JSON blob, or empty when it cannot be produced.
pub(crate) fn emit_curl_version(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: curl_version (read the linked libcurl's version info) ---");
    emitter.label_global("__rt_curl_version");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #32");                             // length out-parameter plus the frame

            emitter.instruction("stp x29, x30, [sp, #16]");                     // save frame pointer and return address

            emitter.instruction("add x29, sp, #16");                            // set the frame pointer; the buffer is allocated below it

            emitter.instruction("str xzr, [x29, #-16]");                        // clear the blob length out-parameter

            emitter.instruction("mov x0, #0");                                  // C ABI out_json = null: this pass only measures

            emitter.instruction("mov x1, #0");                                  // C ABI cap = 0

            emitter.instruction("sub x2, x29, #16");                            // C ABI len = &blob_length

            emit_load_entry_or_branch(
                emitter,
                "_elephc_curl_global_info_fn",
                "__rt_curl_version_empty",
            );
            emit_call_entry(emitter);                                           // measure the blob; the length is reported even on the 0 return

            emitter.instruction("ldr x1, [x29, #-16]");                         // reload the required byte length

            emitter.instruction("cbz x1, __rt_curl_version_empty");             // no blob to report

            emitter.instruction("add x9, x1, #15");                             // round the buffer size up to the stack alignment

            emitter.instruction("lsr x9, x9, #4");                              // (len + 15) / 16

            emitter.instruction("lsl x9, x9, #4");                              // * 16 — sp must stay 16-byte aligned

            emitter.instruction("sub sp, sp, x9");                              // allocate the exact-size blob buffer

            emitter.instruction("mov x0, sp");                                  // C ABI out_json = the blob buffer

            emitter.instruction("sub x2, x29, #16");                            // C ABI len = &blob_length

            // The entry pointer was reloaded into the scratch register by the size
            // rounding above, so the slot is probed again rather than assumed live.
            emit_load_entry_or_branch(
                emitter,
                "_elephc_curl_global_info_fn",
                "__rt_curl_version_empty",
            );
            emit_call_entry(emitter);                                           // fill the buffer with the version JSON

            emitter.instruction("cbz x0, __rt_curl_version_empty");             // the bridge could not fill the buffer

            emitter.instruction("ldr x2, [x29, #-16]");                         // reload the written byte length

            emitter.instruction("mov x1, sp");                                  // the blob lives in the stack buffer

            emitter.instruction("b __rt_curl_version_persist");                 // copy it into owned storage

            emitter.label("__rt_curl_version_empty");
            emitter.instruction("mov x1, x29");                                 // a valid address so the zero-length copy has a source

            emitter.instruction("mov x2, #0");                                  // no blob is an empty PHP string

            emitter.label("__rt_curl_version_persist");
            emitter.instruction("bl __rt_str_persist");                         // own the bytes; the stack buffer dies with this frame

            emitter.instruction("sub sp, x29, #16");                            // undo the blob alloca, whether or not it happened

            emitter.instruction("ldp x29, x30, [sp, #16]");                     // restore frame pointer and return address

            emitter.instruction("add sp, sp, #32");                             // release the frame

            emitter.instruction("ret");                                         // return the owned JSON string

        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // preserve the caller frame pointer

            emitter.instruction("mov rbp, rsp");                                // establish the frame base; the buffer is allocated below it

            emitter.instruction("sub rsp, 32");                                 // length out-parameter, 16-byte aligned

            emitter.instruction("mov QWORD PTR [rbp - 8], 0");                  // clear the blob length out-parameter

            emitter.instruction("xor edi, edi");                                // C ABI out_json = null: this pass only measures

            emitter.instruction("xor esi, esi");                                // C ABI cap = 0

            emitter.instruction("lea rdx, [rbp - 8]");                          // C ABI len = &blob_length

            emit_load_entry_or_branch(
                emitter,
                "_elephc_curl_global_info_fn",
                "__rt_curl_version_empty_x86",
            );
            emit_call_entry(emitter);                                           // measure the blob; the length is reported even on the 0 return

            emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                // reload the required byte length as the next call's capacity

            emitter.instruction("test rsi, rsi");                               // no blob to report?

            emitter.instruction("jz __rt_curl_version_empty_x86");              // -> empty string

            emitter.instruction("mov r10, rsi");                                // round the buffer size up to the stack alignment

            emitter.instruction("add r10, 15");                                 // (len + 15)

            emitter.instruction("shr r10, 4");                                  // / 16

            emitter.instruction("shl r10, 4");                                  // * 16 — rsp must stay 16-byte aligned at every call

            emitter.instruction("sub rsp, r10");                                // allocate the exact-size blob buffer

            emitter.instruction("mov rdi, rsp");                                // C ABI out_json = the blob buffer

            emitter.instruction("lea rdx, [rbp - 8]");                          // C ABI len = &blob_length

            emit_load_entry_or_branch(
                emitter,
                "_elephc_curl_global_info_fn",
                "__rt_curl_version_empty_x86",
            );
            emit_call_entry(emitter);                                           // fill the buffer with the version JSON

            emitter.instruction("test eax, eax");                               // the bridge could not fill the buffer?

            emitter.instruction("jz __rt_curl_version_empty_x86");              // -> empty string

            emitter.instruction("mov rdx, QWORD PTR [rbp - 8]");                // reload the written byte length

            emitter.instruction("mov rax, rsp");                                // the blob lives in the stack buffer

            emitter.instruction("jmp __rt_curl_version_persist_x86");           // copy it into owned storage

            emitter.label("__rt_curl_version_empty_x86");
            emitter.instruction("mov rax, rbp");                                // a valid address so the zero-length copy has a source

            emitter.instruction("xor edx, edx");                                // no blob is an empty PHP string

            emitter.label("__rt_curl_version_persist_x86");
            emitter.instruction("call __rt_str_persist");                       // own the bytes; the stack buffer dies with this frame

            emitter.instruction("mov rsp, rbp");                                // undo the blob alloca and release the frame

            emitter.instruction("pop rbp");                                     // restore the caller frame pointer

            emitter.instruction("ret");                                         // return the owned JSON string

        }
    }
}
