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
/// php's `str_ireplace()` is case-insensitive in EVERY argument shape, not only the scalar one.
/// Both array loops used to call `__rt_str_replace` unconditionally, so
/// `str_ireplace(["A","N"], ["x","y"], "banana")` answered `"banana"` where php answers
/// `"bxyxyx"` — MEASURED on `php -n` 8.5.6. Each loop is emitted twice, once per case, because
/// the only difference is which inner helper it delegates to, and a flag threaded through a call
/// site is a thing a call site can forget.
pub fn emit_str_replace_search_array(emitter: &mut Emitter) {
    for (symbol, tag, inner) in [
        ("__rt_str_replace_search_array", "__rt_srsa", "__rt_str_replace"),
        ("__rt_str_ireplace_search_array", "__rt_isrsa", "__rt_str_ireplace"),
    ] {
        match emitter.target.arch {
            Arch::AArch64 => emit_aarch64(emitter, symbol, tag, inner),
            Arch::X86_64 => emit_x86_64(emitter, symbol, tag, inner),
        }
    }
}

/// The AArch64 loop.
fn emit_aarch64(emitter: &mut Emitter, symbol: &str, tag: &str, inner: &str) {
    emitter.blank();
    emitter.comment(&format!("--- runtime: {} ---", &symbol[5..]));
    emitter.label_global(symbol);
    // Frame: [0]=search arr [8]=replace arr [16]=replace ptr [24]=replace len
    //        [32]=subject ptr [40]=subject len [48]=index [56]=search count
    //        [64]=replacements so far.
    emitter.instruction("sub sp, sp, #96");                                     // reserve the loop frame, plus the replacement tally
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // the `$search` array
    emitter.instruction("str x1, [sp, #8]");                                    // the `$replace` array, or zero
    emitter.instruction("str x2, [sp, #16]");                                   // the scalar `$replace` pointer
    emitter.instruction("str x3, [sp, #24]");                                   // and its length
    emitter.instruction("str x4, [sp, #32]");                                   // the subject, which each pass replaces
    emitter.instruction("str x5, [sp, #40]");                                   // and its length
    emitter.instruction("str xzr, [sp, #48]");                                  // pair index
    emitter.instruction("ldr x9, [x0]");                                        // how many search terms there are
    emitter.instruction("str x9, [sp, #56]");
    // php counts every replacement across the WHOLE call, not per pass, so the tally is
    // accumulated here from what each inner call reports. The frame grew by one slot to hold it:
    // every register in this loop is reloaded from the frame on each pass, so there is none to
    // keep it in.
    emitter.instruction("str xzr, [sp, #64]");                                  // no replacement has fired yet

    emitter.label(&format!("{tag}_loop"));
    emitter.instruction("ldr x9, [sp, #48]");                                   // the pair index
    emitter.instruction("ldr x10, [sp, #56]");                                  // the search count
    emitter.instruction("cmp x9, x10");
    emitter.instruction(&format!("b.ge {tag}_done"));                                 // every pair applied

    // -- search[i] --
    emitter.instruction("ldr x11, [sp, #0]");                                   // the search array
    emitter.instruction("lsl x12, x9, #4");                                     // 16-byte slots
    emitter.instruction("add x12, x12, #24");                                   // past the header
    emitter.instruction("add x12, x11, x12");                                   // &search[i]
    emitter.instruction("ldr x1, [x12, #0]");                                   // search pointer
    emitter.instruction("ldr x2, [x12, #8]");                                   // search length

    // -- replace[i], the scalar, or the empty string --
    emitter.instruction("ldr x13, [sp, #8]");                                   // the replace array, or zero
    emitter.instruction(&format!("cbz x13, {tag}_scalar_replace"));
    emitter.instruction("ldr x14, [x13]");                                      // how many replacements there are
    emitter.instruction("cmp x9, x14");
    emitter.instruction(&format!("b.hs {tag}_empty_replace"));                        // php pairs the remainder with ""
    emitter.instruction("lsl x15, x9, #4");
    emitter.instruction("add x15, x15, #24");
    emitter.instruction("add x15, x13, x15");                                   // &replace[i]
    emitter.instruction("ldr x3, [x15, #0]");                                   // replacement pointer
    emitter.instruction("ldr x4, [x15, #8]");                                   // replacement length
    emitter.instruction(&format!("b {tag}_have_replace"));
    emitter.label(&format!("{tag}_empty_replace"));
    emitter.instruction("mov x3, #0");                                          // an empty replacement needs no bytes
    emitter.instruction("mov x4, #0");
    emitter.instruction(&format!("b {tag}_have_replace"));
    emitter.label(&format!("{tag}_scalar_replace"));
    emitter.instruction("ldr x3, [sp, #16]");                                   // the scalar replacement, used for every term
    emitter.instruction("ldr x4, [sp, #24]");
    emitter.label(&format!("{tag}_have_replace"));

    // -- apply this pair to the CURRENT subject, which is the previous pass's result --
    emitter.instruction("ldr x5, [sp, #32]");                                   // subject pointer
    emitter.instruction("ldr x6, [sp, #40]");                                   // subject length
    emitter.instruction(&format!("bl {inner}"));                                 // x1/x2 = the new subject, x0 = what this pass replaced
    emitter.instruction("ldr x9, [sp, #64]");                                   // add this pass to the running tally
    emitter.instruction("add x9, x9, x0");
    emitter.instruction("str x9, [sp, #64]");
    emitter.instruction("str x1, [sp, #32]");
    emitter.instruction("str x2, [sp, #40]");
    emitter.instruction("ldr x9, [sp, #48]");
    emitter.instruction("add x9, x9, #1");
    emitter.instruction("str x9, [sp, #48]");
    emitter.instruction(&format!("b {tag}_loop"));

    emitter.label(&format!("{tag}_done"));
    emitter.instruction("ldr x1, [sp, #32]");                                   // the final result pointer
    emitter.instruction("ldr x2, [sp, #40]");                                   // and its length
    emitter.instruction("ldr x0, [sp, #64]");                                   // the whole call's replacement count
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the loop frame
    emitter.instruction("ret");
}

/// The x86_64 loop.
fn emit_x86_64(emitter: &mut Emitter, symbol: &str, tag: &str, inner: &str) {
    emitter.blank();
    emitter.comment(&format!("--- runtime: {} ---", &symbol[5..]));
    emitter.label_global(symbol);
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the loop frame
    emitter.instruction("sub rsp, 96");                                         // reserve the loop slots, plus the replacement tally
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // the `$search` array
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // the `$replace` array, or zero
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // the scalar `$replace` pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // and its length
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // the subject, which each pass replaces
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // and its length
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // pair index
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // how many search terms there are
    emitter.instruction("mov QWORD PTR [rbp - 64], r10");
    emitter.instruction("mov QWORD PTR [rbp - 88], 0");                         // no replacement has fired yet

    emitter.label(&format!("{tag}_loop_x86"));
    emitter.instruction("mov r10, QWORD PTR [rbp - 56]");                       // the pair index
    emitter.instruction("cmp r10, QWORD PTR [rbp - 64]");                       // against the search count
    emitter.instruction(&format!("jge {tag}_done_x86"));                              // every pair applied

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
    emitter.instruction(&format!("jz {tag}_scalar_replace_x86"));
    emitter.instruction("mov rax, QWORD PTR [r11]");                            // how many replacements there are
    emitter.instruction("cmp r10, rax");
    emitter.instruction(&format!("jae {tag}_empty_replace_x86"));                     // php pairs the remainder with ""
    emitter.instruction("mov rax, r10");
    emitter.instruction("shl rax, 4");
    emitter.instruction("add rax, 24");
    emitter.instruction("add rax, r11");                                        // &replace[i]
    emitter.instruction("mov rcx, QWORD PTR [rax + 0]");                        // replacement pointer
    emitter.instruction("mov r8, QWORD PTR [rax + 8]");                         // replacement length
    emitter.instruction(&format!("jmp {tag}_have_replace_x86"));
    emitter.label(&format!("{tag}_empty_replace_x86"));
    emitter.instruction("xor rcx, rcx");                                        // an empty replacement needs no bytes
    emitter.instruction("xor r8, r8");
    emitter.instruction(&format!("jmp {tag}_have_replace_x86"));
    emitter.label(&format!("{tag}_scalar_replace_x86"));
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // the scalar replacement
    emitter.instruction("mov r8, QWORD PTR [rbp - 32]");
    emitter.label(&format!("{tag}_have_replace_x86"));

    // `__rt_str_replace` takes the register roles its LOWERING hands it — rax/rdx = search,
    // rdi/rsi = replace, rcx/r8 = subject. The doc comment above that helper names a different
    // set; the code and its caller agree with each other, and this follows them.
    emitter.instruction("mov rdi, rcx");                                        // replacement pointer
    emitter.instruction("mov rsi, r8");                                         // replacement length
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // search pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 80]");                       // search length
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // subject pointer
    emitter.instruction("mov r8, QWORD PTR [rbp - 48]");                        // subject length
    emitter.instruction(&format!("call {inner}"));
    emitter.instruction("add QWORD PTR [rbp - 88], rcx");                       // add this pass to the running tally
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // the new subject
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");
    emitter.instruction("mov r10, QWORD PTR [rbp - 56]");
    emitter.instruction("inc r10");
    emitter.instruction("mov QWORD PTR [rbp - 56], r10");
    emitter.instruction(&format!("jmp {tag}_loop_x86"));

    emitter.label(&format!("{tag}_done_x86"));
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // the final result pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // and its length
    emitter.instruction("mov rcx, QWORD PTR [rbp - 88]");                       // the whole call's replacement count
    emitter.instruction("add rsp, 96");                                         // release the loop frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");
}
