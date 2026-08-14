//! Purpose:
//! Emits `__rt_sprintf_pack_mixed`, which turns one boxed Mixed cell into the 16-byte tagged
//! record `__rt_sprintf` consumes. Shared by the `sprintf()`/`printf()` argument packer and by
//! `__rt_vsprintf`'s per-element loop.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::strings`.
//! - `crate::codegen::lower_inst::builtins::strings::printf` for a Mixed operand whose
//!   conversion category is not known at compile time.
//!
//! Key details:
//! - Record tags match `__rt_sprintf`: int = 0, string = 1 | (len << 8), float = 2, bool = 3.
//!   `__rt_sprintf` COERCES a record whose tag disagrees with the conversion character, so
//!   packing the cell's real runtime type is enough — the caller does not need to know whether
//!   the format asks for `%d`, `%s` or `%f`.
//! - Boxed PHP null (cell tag 8) becomes a ZERO-LENGTH STRING record, not an integer. That is
//!   what makes `%s` render `""` and `%d` render `0`, matching PHP on both: `__rt_sprintf`
//!   already guards a null string pointer on all three conversion paths ("treat a null string
//!   pointer as empty" for `%s`, "a null pointer parses as zero" for the int and float paths).
//!   Packing null as an integer instead would print the cell's raw low word — which is the
//!   null SENTINEL, and is exactly how `vsprintf("%s", [null])` used to answer
//!   `9223372036854775806`.
//! - Cell tags this does not model — array (4/5), object (6), resource (9), callable (10) —
//!   fall through to an integer record carrying the raw low word, which is what the inline
//!   ladder in `__rt_vsprintf` did before this helper existed. PHP renders those as `"Array"`
//!   with a notice, or through `__toString()`; neither is modelled here and neither regressed.
//! - Leaf function: no frame, no calls, so callers may invoke it with loop state live in
//!   memory and `x29`/`rbp` untouched.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits `__rt_sprintf_pack_mixed(boxed_cell) -> (payload, tag)`.
///
/// Inputs: `x0` = boxed Mixed pointer, possibly null (AArch64); `rdi` (x86_64).
/// Outputs: `x0`/`rax` = record payload word, `x1`/`rdx` = record tag/metadata word.
pub fn emit_sprintf_pack_mixed(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_sprintf_pack_mixed_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: sprintf_pack_mixed ---");
    emitter.label_global("__rt_sprintf_pack_mixed");

    emitter.instruction("cbz x0, __rt_spm_null");                               // a null cell pointer packs as PHP null
    emitter.instruction("ldr x9, [x0]");                                        // cell runtime tag
    emitter.instruction("ldr x10, [x0, #8]");                                   // cell low payload word
    emitter.instruction("cmp x9, #1");                                          // string cell?
    emitter.instruction("b.eq __rt_spm_str");                                   // build a string record
    emitter.instruction("cmp x9, #2");                                          // float cell?
    emitter.instruction("b.eq __rt_spm_float");                                 // build a float record
    emitter.instruction("cmp x9, #3");                                          // bool cell?
    emitter.instruction("b.eq __rt_spm_bool");                                  // build a bool record
    emitter.instruction("cmp x9, #8");                                          // canonical boxed PHP null?
    emitter.instruction("b.eq __rt_spm_null");                                  // → empty-string record, never the raw sentinel
    emitter.instruction("mov x0, x10");                                         // anything else → integer payload
    emitter.instruction("mov x1, #0");                                          // tag 0 = integer operand
    emitter.instruction("ret");                                                 // return payload/tag

    emitter.label("__rt_spm_str");
    emitter.instruction("ldr x1, [x0, #16]");                                   // cell high word = string byte length
    emitter.instruction("mov x0, x10");                                         // payload = the string pointer
    emitter.instruction("lsl x1, x1, #8");                                      // pack the length above the tag byte
    emitter.instruction("orr x1, x1, #1");                                      // tag 1 = string operand
    emitter.instruction("ret");                                                 // return payload/tag

    emitter.label("__rt_spm_float");
    emitter.instruction("mov x0, x10");                                         // payload = the double's bit pattern
    emitter.instruction("mov x1, #2");                                          // tag 2 = float operand
    emitter.instruction("ret");                                                 // return payload/tag

    emitter.label("__rt_spm_bool");
    emitter.instruction("mov x0, x10");                                         // payload = the boolean value
    emitter.instruction("mov x1, #3");                                          // tag 3 = bool operand
    emitter.instruction("ret");                                                 // return payload/tag

    emitter.label("__rt_spm_null");
    emitter.instruction("mov x0, #0");                                          // null string pointer, which every conversion guards
    emitter.instruction("mov x1, #1");                                          // (0 << 8) | 1 = a zero-length string record
    emitter.instruction("ret");                                                 // return payload/tag
}

/// Emits the Linux x86_64 string runtime helper for sprintf_pack_mixed.
fn emit_sprintf_pack_mixed_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: sprintf_pack_mixed ---");
    emitter.label_global("__rt_sprintf_pack_mixed");

    emitter.instruction("test rdi, rdi");                                       // a null cell pointer packs as PHP null
    emitter.instruction("jz __rt_spm_null");                                    // → empty-string record
    emitter.instruction("mov r9, QWORD PTR [rdi]");                             // cell runtime tag
    emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                        // cell low payload word
    emitter.instruction("cmp r9, 1");                                           // string cell?
    emitter.instruction("je __rt_spm_str");                                     // build a string record
    emitter.instruction("cmp r9, 2");                                           // float cell?
    emitter.instruction("je __rt_spm_float");                                   // build a float record
    emitter.instruction("cmp r9, 3");                                           // bool cell?
    emitter.instruction("je __rt_spm_bool");                                    // build a bool record
    emitter.instruction("cmp r9, 8");                                           // canonical boxed PHP null?
    emitter.instruction("je __rt_spm_null");                                    // → empty-string record, never the raw sentinel
    emitter.instruction("mov rax, r10");                                        // anything else → integer payload
    emitter.instruction("mov rdx, 0");                                          // tag 0 = integer operand
    emitter.instruction("ret");                                                 // return payload/tag

    emitter.label("__rt_spm_str");
    emitter.instruction("mov rdx, QWORD PTR [rdi + 16]");                       // cell high word = string byte length
    emitter.instruction("mov rax, r10");                                        // payload = the string pointer
    emitter.instruction("shl rdx, 8");                                          // pack the length above the tag byte
    emitter.instruction("or rdx, 1");                                           // tag 1 = string operand
    emitter.instruction("ret");                                                 // return payload/tag

    emitter.label("__rt_spm_float");
    emitter.instruction("mov rax, r10");                                        // payload = the double's bit pattern
    emitter.instruction("mov rdx, 2");                                          // tag 2 = float operand
    emitter.instruction("ret");                                                 // return payload/tag

    emitter.label("__rt_spm_bool");
    emitter.instruction("mov rax, r10");                                        // payload = the boolean value
    emitter.instruction("mov rdx, 3");                                          // tag 3 = bool operand
    emitter.instruction("ret");                                                 // return payload/tag

    emitter.label("__rt_spm_null");
    emitter.instruction("mov rax, 0");                                          // null string pointer, which every conversion guards
    emitter.instruction("mov rdx, 1");                                          // (0 << 8) | 1 = a zero-length string record
    emitter.instruction("ret");                                                 // return payload/tag
}
