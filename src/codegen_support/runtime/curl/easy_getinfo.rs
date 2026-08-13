//! Purpose:
//! Emits the two typed `curl_getinfo()` runtime helpers behind the internal
//! `__elephc_curl_easy_getinfo_long` / `__elephc_curl_easy_getinfo_double` builtins: each
//! reads its own `CURLINFO_*` type range into a boxed PHP `int`/`float`, or boxed `false`
//! when the bridge could not answer.
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
//! - `false` ON FAILURE, the value ON SUCCESS: both are boxed through
//!   `__rt_mixed_from_value` (tag 0 = int, tag 2 = float, tag 3 = boolean), matching
//!   `curl_getinfo()`'s documented `int|float|false` answers.
//! - THE DOUBLE HELPER IS THE SAME CODE WITH A DIFFERENT TAG. `__rt_mixed_from_value`
//!   takes a float's payload as its raw 64-BIT PATTERN in the same register an `int`
//!   payload uses, so reloading the out-parameter slot as an integer and changing the tag
//!   from 0 to 2 is the entire difference — no floating-point register is involved.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

use super::slots::{emit_call_entry, emit_load_entry_or_branch};

/// `__rt_curl_easy_getinfo_long` — in: handle id in `x0`/`rdi`, `CURLINFO_*` option in
/// `x1`/`rsi`. Out: boxed Mixed in `x0`/`rax` (an `int`, or boxed PHP `false`).
pub(crate) fn emit_curl_easy_getinfo_long(emitter: &mut Emitter) {
    emit_typed_getinfo(
        emitter,
        "__rt_curl_easy_getinfo_long",
        "_elephc_curl_easy_getinfo_long_fn",
        "curl_easy_getinfo_long (read a long-typed CURLINFO field)",
        0,
    );
}

/// `__rt_curl_easy_getinfo_double` — in: handle id in `x0`/`rdi`, `CURLINFO_*` option in
/// `x1`/`rsi`. Out: boxed Mixed in `x0`/`rax` (a `float`, or boxed PHP `false`).
pub(crate) fn emit_curl_easy_getinfo_double(emitter: &mut Emitter) {
    emit_typed_getinfo(
        emitter,
        "__rt_curl_easy_getinfo_double",
        "_elephc_curl_easy_getinfo_double_fn",
        "curl_easy_getinfo_double (read a double-typed CURLINFO field)",
        2,
    );
}

/// Emits one typed getinfo helper. `tag` is the `__rt_mixed_from_value` tag for the
/// fetched value (0 = int, 2 = float); the out-parameter's 8 bytes are reloaded as a raw
/// payload either way.
fn emit_typed_getinfo(
    emitter: &mut Emitter,
    label: &str,
    slot: &str,
    description: &str,
    tag: u8,
) {
    let false_label = format!("{label}_false");
    let false_label_x86 = format!("{label}_false_x86");
    emitter.blank();
    emitter.comment(&format!("--- runtime: {description} ---"));
    emitter.label_global(label);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #32");                             // out-parameter slot plus the frame

            emitter.instruction("stp x29, x30, [sp, #16]");                     // save frame pointer and return address

            emitter.instruction("add x29, sp, #16");                            // set the frame pointer

            emitter.instruction("str xzr, [sp]");                               // clear the out-parameter slot

            emitter.instruction("mov x2, sp");                                  // C ABI out = &value

            emit_load_entry_or_branch(emitter, slot, &false_label);
            emit_call_entry(emitter);                                           // elephc_curl_easy_getinfo_long(id, info, &value)

            // `w0`, not `x0`: the bridge returns a C `int32_t` and AAPCS64 leaves the
            // upper 32 bits of the return register unspecified.
            emitter.instruction(&format!("cbz w0, {false_label}"));             // unknown id, wrong info type, or getinfo failure -> PHP false

            emitter.instruction("ldr x1, [sp]");                                // Mixed payload = the fetched long value

            emitter.instruction("mov x2, #0");                                  // scalars carry no high payload word

            emitter.instruction(&format!("mov x0, #{tag}"));                    // runtime tag 0 = int, 2 = float

            emitter.instruction("bl __rt_mixed_from_value");                    // box the fetched value

            emitter.instruction("ldp x29, x30, [sp, #16]");                     // restore frame pointer and return address

            emitter.instruction("add sp, sp, #32");                             // release the frame

            emitter.instruction("ret");                                         // return the boxed int

            emitter.label(&false_label);
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

            emit_load_entry_or_branch(emitter, slot, &false_label_x86);
            emit_call_entry(emitter);                                           // elephc_curl_easy_getinfo_long(id, info, &value)

            emitter.instruction("test eax, eax");                               // unknown id, wrong info type, or getinfo failure?

            emitter.instruction(&format!("jz {false_label_x86}"));              // -> PHP false

            emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                // Mixed payload = the fetched long value

            emitter.instruction("xor esi, esi");                                // scalars carry no high payload word

            emitter.instruction(&format!("mov eax, {tag}"));                    // runtime tag 0 = int, 2 = float

            emitter.instruction("call __rt_mixed_from_value");                  // box the fetched value

            emitter.instruction("mov rsp, rbp");                                // release the frame

            emitter.instruction("pop rbp");                                     // restore the caller frame pointer

            emitter.instruction("ret");                                         // return the boxed int

            emitter.label(&false_label_x86);
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
