//! Purpose:
//! Emits `__rt_str_replace_search_array`, php's `str_replace()` with an ARRAY `$search`.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::strings`, when the search operand is an array.
//!
//! Key details:
//! - `str_replace(["a","b"], ["1","2"], $s)` is the idiomatic form and elephc refused it outright:
//!   the EIR backend reported `str_replace string coercion for PHP type Array(Str)`, so the call
//!   did not compile at all.
//! - php applies the pairs IN ORDER, each to the result of the last — not to the original subject.
//!   That is observable: `str_replace(["a","b"], ["b","c"], "a")` answers `"c"`, because the `a`
//!   became a `b` and the second pair then rewrote it. The loop below therefore feeds each result
//!   back in as the next subject.
//! - A `$replace` array SHORTER than `$search` pairs the remainder with the empty string:
//!   measured on `php -n` 8.5.6, `str_replace(["a","b"], ["1"], "abc")` answers `"1c"`, not
//!   `"1bc"`. A scalar `$replace` is used for every search term.
//! - Array layout, shared with `__rt_array_push_str`: length at `[arr]`, capacity at `[arr + 8]`,
//!   element size at `[arr + 16]`, and 16-byte `(pointer, length)` slots from `[arr + 24]`.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits `__rt_str_replace_search_array`.
///
/// # Input
/// - `x0`/`rdi`: the `$search` array
/// - `x1`/`rsi`: the `$replace` array, or 0 when `$replace` is a scalar
/// - `x2`/`rdx`, `x3`/`rcx`: the scalar `$replace` pointer and length, ignored when the array is set
/// - `x4`/`r8`, `x5`/`r9`: the `$subject` pointer and length
///
/// # Output
/// - `x1`/`rax`, `x2`/`rdx`: the result pointer and length, in the same form `__rt_str_replace`
///   answers, because each pass IS a `__rt_str_replace` call.
pub fn emit_str_replace_search_array(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// The AArch64 loop.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_replace with an array search ---");
    emitter.label_global("__rt_str_replace_search_array");
    // Frame: [0]=search arr [8]=replace arr [16]=replace ptr [24]=replace len
    //        [32]=subject ptr [40]=subject len [48]=index [56]=search count.
    emitter.instruction("sub sp, sp, #80");                                     // reserve the loop frame
    emitter.instruction("stp x29, x30, [sp, #64]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #64");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // the `$search` array
    emitter.instruction("str x1, [sp, #8]");                                    // the `$replace` array, or zero
    emitter.instruction("str x2, [sp, #16]");                                   // the scalar `$replace` pointer
    emitter.instruction("str x3, [sp, #24]");                                   // and its length
    emitter.instruction("str x4, [sp, #32]");                                   // the subject, which each pass replaces
    emitter.instruction("str x5, [sp, #40]");                                   // and its length
    emitter.instruction("str xzr, [sp, #48]");                                  // pair index
    emitter.instruction("ldr x9, [x0]");                                        // how many search terms there are
    emitter.instruction("str x9, [sp, #56]");

    emitter.label("__rt_srsa_loop");
    emitter.instruction("ldr x9, [sp, #48]");                                   // the pair index
    emitter.instruction("ldr x10, [sp, #56]");                                  // the search count
    emitter.instruction("cmp x9, x10");
    emitter.instruction("b.ge __rt_srsa_done");                                 // every pair applied

    // -- search[i] --
    emitter.instruction("ldr x11, [sp, #0]");                                   // the search array
    emitter.instruction("lsl x12, x9, #4");                                     // 16-byte slots
    emitter.instruction("add x12, x12, #24");                                   // past the header
    emitter.instruction("add x12, x11, x12");                                   // &search[i]
    emitter.instruction("ldr x1, [x12, #0]");                                   // search pointer
    emitter.instruction("ldr x2, [x12, #8]");                                   // search length

    // -- replace[i], the scalar, or the empty string --
    emitter.instruction("ldr x13, [sp, #8]");                                   // the replace array, or zero
    emitter.instruction("cbz x13, __rt_srsa_scalar_replace");
    emitter.instruction("ldr x14, [x13]");                                      // how many replacements there are
    emitter.instruction("cmp x9, x14");
    emitter.instruction("b.hs __rt_srsa_empty_replace");                        // php pairs the remainder with ""
    emitter.instruction("lsl x15, x9, #4");
    emitter.instruction("add x15, x15, #24");
    emitter.instruction("add x15, x13, x15");                                   // &replace[i]
    emitter.instruction("ldr x3, [x15, #0]");                                   // replacement pointer
    emitter.instruction("ldr x4, [x15, #8]");                                   // replacement length
    emitter.instruction("b __rt_srsa_have_replace");
    emitter.label("__rt_srsa_empty_replace");
    emitter.instruction("mov x3, #0");                                          // an empty replacement needs no bytes
    emitter.instruction("mov x4, #0");
    emitter.instruction("b __rt_srsa_have_replace");
    emitter.label("__rt_srsa_scalar_replace");
    emitter.instruction("ldr x3, [sp, #16]");                                   // the scalar replacement, used for every term
    emitter.instruction("ldr x4, [sp, #24]");
    emitter.label("__rt_srsa_have_replace");

    // -- apply this pair to the CURRENT subject, which is the previous pass's result --
    emitter.instruction("ldr x5, [sp, #32]");                                   // subject pointer
    emitter.instruction("ldr x6, [sp, #40]");                                   // subject length
    emitter.instruction("bl __rt_str_replace");                                 // x1/x2 = the new subject
    emitter.instruction("str x1, [sp, #32]");
    emitter.instruction("str x2, [sp, #40]");
    emitter.instruction("ldr x9, [sp, #48]");
    emitter.instruction("add x9, x9, #1");
    emitter.instruction("str x9, [sp, #48]");
    emitter.instruction("b __rt_srsa_loop");

    emitter.label("__rt_srsa_done");
    emitter.instruction("ldr x1, [sp, #32]");                                   // the final result pointer
    emitter.instruction("ldr x2, [sp, #40]");                                   // and its length
    emitter.instruction("ldp x29, x30, [sp, #64]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #80");                                     // release the loop frame
    emitter.instruction("ret");
}

/// The x86_64 loop.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: str_replace with an array search ---");
    emitter.label_global("__rt_str_replace_search_array");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the loop frame
    emitter.instruction("sub rsp, 80");                                         // reserve the loop slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // the `$search` array
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // the `$replace` array, or zero
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // the scalar `$replace` pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // and its length
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // the subject, which each pass replaces
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // and its length
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // pair index
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // how many search terms there are
    emitter.instruction("mov QWORD PTR [rbp - 64], r10");

    emitter.label("__rt_srsa_loop_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 56]");                       // the pair index
    emitter.instruction("cmp r10, QWORD PTR [rbp - 64]");                       // against the search count
    emitter.instruction("jge __rt_srsa_done_x86");                              // every pair applied

    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // the search array
    emitter.instruction("mov rax, r10");
    emitter.instruction("shl rax, 4");                                          // 16-byte slots
    emitter.instruction("add rax, 24");                                         // past the header
    emitter.instruction("add rax, r11");                                        // &search[i]
    emitter.instruction("mov rsi, QWORD PTR [rax + 0]");                        // search pointer
    emitter.instruction("mov rdx, QWORD PTR [rax + 8]");                        // search length
    emitter.instruction("mov QWORD PTR [rbp - 72], rsi");                       // both outlive the replacement probe
    emitter.instruction("mov QWORD PTR [rbp - 80], rdx");

    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // the replace array, or zero
    emitter.instruction("test r11, r11");
    emitter.instruction("jz __rt_srsa_scalar_replace_x86");
    emitter.instruction("mov rax, QWORD PTR [r11]");                            // how many replacements there are
    emitter.instruction("cmp r10, rax");
    emitter.instruction("jae __rt_srsa_empty_replace_x86");                     // php pairs the remainder with ""
    emitter.instruction("mov rax, r10");
    emitter.instruction("shl rax, 4");
    emitter.instruction("add rax, 24");
    emitter.instruction("add rax, r11");                                        // &replace[i]
    emitter.instruction("mov rcx, QWORD PTR [rax + 0]");                        // replacement pointer
    emitter.instruction("mov r8, QWORD PTR [rax + 8]");                         // replacement length
    emitter.instruction("jmp __rt_srsa_have_replace_x86");
    emitter.label("__rt_srsa_empty_replace_x86");
    emitter.instruction("xor rcx, rcx");                                        // an empty replacement needs no bytes
    emitter.instruction("xor r8, r8");
    emitter.instruction("jmp __rt_srsa_have_replace_x86");
    emitter.label("__rt_srsa_scalar_replace_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // the scalar replacement
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");
    emitter.label("__rt_srsa_have_replace_x86");

    // `__rt_str_replace` takes the register roles its LOWERING hands it — rax/rdx = search,
    // rdi/rsi = replace, rcx/r8 = subject. The doc comment above that helper names a different
    // set; the code and its caller agree with each other, and this follows them.
    emitter.instruction("mov rdi, rcx");                                        // replacement pointer
    emitter.instruction("mov rsi, r8");                                         // replacement length
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // search pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 80]");                       // search length
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // subject pointer
    emitter.instruction("mov r8, QWORD PTR [rbp - 48]");                        // subject length
    emitter.instruction("call __rt_str_replace");
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // the new subject
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");
    emitter.instruction("mov r10, QWORD PTR [rbp - 56]");
    emitter.instruction("inc r10");
    emitter.instruction("mov QWORD PTR [rbp - 56], r10");
    emitter.instruction("jmp __rt_srsa_loop_x86");

    emitter.label("__rt_srsa_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // the final result pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // and its length
    emitter.instruction("add rsp, 80");                                         // release the loop frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");
}
