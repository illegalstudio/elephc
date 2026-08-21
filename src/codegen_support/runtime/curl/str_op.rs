//! Purpose:
//! Emits `__rt_curl_easy_str_op`, the runtime helper behind the internal
//! `__elephc_curl_easy_str_op` builtin: runs one of the bridge's string-producing
//! easy-handle operations and copies the bytes it parked into an owned PHP string, or
//! hands back PHP `false`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::curl::emit_curl`.
//!
//! Key details:
//! - IT MAKES TWO BRIDGE CALLS, and the handle id has to survive the first one, so the id
//!   is saved to the frame before anything else. Every argument register is caller-saved
//!   on both ABIs, so re-reading `x0`/`rdi` after the first call would read garbage.
//! - THE SECOND CALL'S RESULT IS BORROWED. `elephc_curl_easy_take_scratch` hands back a
//!   pointer into the bridge's own buffer that stays valid only until the next
//!   `elephc_curl_*` call on the same handle, so the bytes go straight into
//!   `__rt_mixed_from_value` (tag 1 = string), which persists a copy — the same reason
//!   `__rt_curl_easy_body` copies rather than aliasing.
//! - BOTH STATUS RETURNS ARE C `int32_t`, so both failure branches test the LOW 32 BITS
//!   (`cbz w0` / `test eax, eax`). Branching on the full register would trust an
//!   unspecified upper half and could take the success path on a genuine failure,
//!   persisting whatever the zeroed out-parameter slots happened to hold.
//! - A ZERO-LENGTH RESULT IS A REAL EMPTY STRING, not `false`: a `CURLINFO_*` field
//!   libcurl reports as NULL is `""` in PHP too. The zero-length branch hands
//!   `__rt_mixed_from_value` a valid stack address so the copy has a source.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

use super::slots::{emit_call_entry, emit_load_entry_or_branch};

/// `__rt_curl_easy_str_op` — in: handle id in `x0`/`rdi`, operation in `x1`/`rsi`, the
/// operation's string argument in `x2`/`x3` (`rdx`/`rcx`) and its integer argument in
/// `x4`/`r8`. Out: boxed Mixed in `x0`/`rax` (a `string`, or boxed PHP `false`).
pub(crate) fn emit_curl_easy_str_op(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: curl_easy_str_op (run a string-producing curl operation) ---");
    emitter.label_global("__rt_curl_easy_str_op");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #48");                             // saved id, two out-parameter slots, and the frame

            emitter.instruction("stp x29, x30, [sp, #32]");                     // save frame pointer and return address

            emitter.instruction("add x29, sp, #32");                            // set the frame pointer

            emitter.instruction("str x0, [sp]");                                // the handle id must outlive the first call

            emit_load_entry_or_branch(
                emitter,
                "_elephc_curl_easy_str_op_fn",
                "__rt_curl_easy_str_op_false",
            );
            emit_call_entry(emitter);                                           // elephc_curl_easy_str_op(id, op, ptr, len, number)

            // `w0`, not `x0`: the bridge returns a C `int32_t`.
            emitter.instruction("cbz w0, __rt_curl_easy_str_op_false");         // the operation could not be answered -> PHP false

            emitter.instruction("str xzr, [sp, #8]");                           // clear the scratch pointer out-parameter

            emitter.instruction("str xzr, [sp, #16]");                          // clear the scratch length out-parameter

            emitter.instruction("ldr x0, [sp]");                                // C ABI id = the saved handle id

            emitter.instruction("add x1, sp, #8");                              // C ABI ptr = &scratch_pointer

            emitter.instruction("add x2, sp, #16");                             // C ABI len = &scratch_length

            emit_load_entry_or_branch(
                emitter,
                "_elephc_curl_easy_take_scratch_fn",
                "__rt_curl_easy_str_op_false",
            );
            emit_call_entry(emitter);                                           // elephc_curl_easy_take_scratch(id, &ptr, &len)

            emitter.instruction("cbz w0, __rt_curl_easy_str_op_false");         // unknown id -> PHP false

            emitter.instruction("ldr x2, [sp, #16]");                           // reload the produced byte length

            emitter.instruction("ldr x1, [sp, #8]");                            // reload the borrowed byte pointer

            emitter.instruction("cbnz x2, __rt_curl_easy_str_op_box");          // nonempty -> copy from the bridge's buffer

            emitter.instruction("mov x1, sp");                                  // a valid address so the zero-length copy has a source

            emitter.label("__rt_curl_easy_str_op_box");
            emitter.instruction("mov x0, #1");                                  // runtime tag 1 = string

            emitter.instruction("bl __rt_mixed_from_value");                    // own the bytes; the bridge may overwrite them next call

            emitter.instruction("ldp x29, x30, [sp, #32]");                     // restore frame pointer and return address

            emitter.instruction("add sp, sp, #48");                             // release the frame

            emitter.instruction("ret");                                         // return the boxed string

            emitter.label("__rt_curl_easy_str_op_false");
            emitter.instruction("mov x1, #0");                                  // boolean payload 0 = PHP false

            emitter.instruction("mov x2, #0");                                  // booleans carry no high payload word

            emitter.instruction("mov x0, #3");                                  // runtime tag 3 = boolean

            emitter.instruction("bl __rt_mixed_from_value");                    // box PHP false, curl_getinfo()'s failure answer

            emitter.instruction("ldp x29, x30, [sp, #32]");                     // restore frame pointer and return address

            emitter.instruction("add sp, sp, #48");                             // release the frame

            emitter.instruction("ret");                                         // return boxed PHP false

        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // preserve the caller frame pointer

            emitter.instruction("mov rbp, rsp");                                // establish the frame base

            emitter.instruction("sub rsp, 48");                                 // saved id plus two out-parameter slots, 16-byte aligned

            emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                // the handle id must outlive the first call

            emit_load_entry_or_branch(
                emitter,
                "_elephc_curl_easy_str_op_fn",
                "__rt_curl_easy_str_op_false_x86",
            );
            emit_call_entry(emitter);                                           // elephc_curl_easy_str_op(id, op, ptr, len, number)

            emitter.instruction("test eax, eax");                               // the operation could not be answered?

            emitter.instruction("jz __rt_curl_easy_str_op_false_x86");          // -> PHP false

            emitter.instruction("mov QWORD PTR [rbp - 16], 0");                 // clear the scratch pointer out-parameter

            emitter.instruction("mov QWORD PTR [rbp - 24], 0");                 // clear the scratch length out-parameter

            emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                // C ABI id = the saved handle id

            emitter.instruction("lea rsi, [rbp - 16]");                         // C ABI ptr = &scratch_pointer

            emitter.instruction("lea rdx, [rbp - 24]");                         // C ABI len = &scratch_length

            emit_load_entry_or_branch(
                emitter,
                "_elephc_curl_easy_take_scratch_fn",
                "__rt_curl_easy_str_op_false_x86",
            );
            emit_call_entry(emitter);                                           // elephc_curl_easy_take_scratch(id, &ptr, &len)

            emitter.instruction("test eax, eax");                               // unknown id?

            emitter.instruction("jz __rt_curl_easy_str_op_false_x86");          // -> PHP false

            emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");               // reload the produced byte length

            emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");               // reload the borrowed byte pointer

            emitter.instruction("test rsi, rsi");                               // nonempty?

            emitter.instruction("jnz __rt_curl_easy_str_op_box_x86");           // -> copy from the bridge's buffer

            emitter.instruction("mov rdi, rbp");                                // a valid address so the zero-length copy has a source

            emitter.label("__rt_curl_easy_str_op_box_x86");
            emitter.instruction("mov eax, 1");                                  // runtime tag 1 = string

            emitter.instruction("call __rt_mixed_from_value");                  // own the bytes; the bridge may overwrite them next call

            emitter.instruction("mov rsp, rbp");                                // release the frame

            emitter.instruction("pop rbp");                                     // restore the caller frame pointer

            emitter.instruction("ret");                                         // return the boxed string

            emitter.label("__rt_curl_easy_str_op_false_x86");
            emitter.instruction("xor edi, edi");                                // boolean payload 0 = PHP false

            emitter.instruction("xor esi, esi");                                // booleans carry no high payload word

            emitter.instruction("mov eax, 3");                                  // runtime tag 3 = boolean

            emitter.instruction("call __rt_mixed_from_value");                  // box PHP false, curl_getinfo()'s failure answer

            emitter.instruction("mov rsp, rbp");                                // release the frame

            emitter.instruction("pop rbp");                                     // restore the caller frame pointer

            emitter.instruction("ret");                                         // return boxed PHP false

        }
    }
}
