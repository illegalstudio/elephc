//! Purpose:
//! Emits `__rt_str_replace_subject_array`, php's `str_replace()` with an ARRAY `$subject`.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::strings`, when the subject operand is an array.
//!
//! Key details:
//! - php replaces inside EVERY element and answers an array: `str_replace("a", "X", ["abc","aaa"])`
//!   answers `["Xbc","XXX"]`. elephc refused the call outright, so a list could not be cleaned in
//!   one step.
//! - `$search` may itself be an array, and then each element goes through the same cascading pass
//!   `__rt_str_replace_search_array` performs. Passing a null search array selects the scalar form,
//!   so one loop serves both.
//! - Array layout, shared with `__rt_array_push_str`: length at `[arr]`, capacity at `[arr + 8]`,
//!   element size at `[arr + 16]`, and 16-byte `(pointer, length)` slots from `[arr + 24]`.
//! - The result is built with `__rt_array_push_str`, which keeps the dense index order a packed
//!   subject has. php PRESERVES the subject's keys, which for a packed array is exactly that
//!   order; a hash subject keeps its string or sparse keys and is not lowered here.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits `__rt_str_replace_subject_array`.
///
/// # Input
/// - `x0`/`rdi`: the `$search` array, or 0 when `$search` is a scalar
/// - `x1`/`rsi`, `x2`/`rdx`: the scalar `$search` pointer and length, ignored when the array is set
/// - `x3`/`rcx`: the `$replace` array, or 0 when `$replace` is a scalar
/// - `x4`/`r8`, `x5`/`r9`: the scalar `$replace` pointer and length
/// - `x6`/stack: the `$subject` array
///
/// # Output
/// - `x0`/`rax`: a fresh array of the replaced elements
/// php's `str_ireplace()` is case-insensitive in EVERY argument shape, not only the scalar one.
/// Both array loops used to call `__rt_str_replace` unconditionally, so
/// `str_ireplace(["A","N"], ["x","y"], "banana")` answered `"banana"` where php answers
/// `"bxyxyx"` — MEASURED on `php -n` 8.5.6. Each loop is emitted twice, once per case, because
/// the only difference is which inner helper it delegates to, and a flag threaded through a call
/// site is a thing a call site can forget.
pub fn emit_str_replace_subject_array(emitter: &mut Emitter) {
    for (symbol, tag, inner, inner_search_array) in [
        (
            "__rt_str_replace_subject_array",
            "__rt_srsu",
            "__rt_str_replace",
            "__rt_str_replace_search_array",
        ),
        (
            "__rt_str_ireplace_subject_array",
            "__rt_isrsu",
            "__rt_str_ireplace",
            "__rt_str_ireplace_search_array",
        ),
    ] {
        match emitter.target.arch {
            Arch::AArch64 => emit_aarch64(emitter, symbol, tag, inner, inner_search_array),
            Arch::X86_64 => emit_x86_64(emitter, symbol, tag, inner, inner_search_array),
        }
    }
}

/// The AArch64 map.
fn emit_aarch64(
    emitter: &mut Emitter,
    symbol: &str,
    tag: &str,
    inner: &str,
    inner_search_array: &str,
) {
    emitter.blank();
    emitter.comment("--- runtime: str_replace over an array subject ---");
    emitter.label_global(symbol);
    // Frame: [0]=search arr [8]=search ptr [16]=search len [24]=replace arr [32]=replace ptr
    //        [40]=replace len [48]=subject arr [56]=index [64]=count [72]=result arr
    //        [80]=replacements so far.
    emitter.instruction("sub sp, sp, #112");                                    // reserve the map frame, plus the replacement tally
    emitter.instruction("stp x29, x30, [sp, #96]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #96");                                    // establish the helper frame pointer
    emitter.instruction("str x0, [sp, #0]");
    emitter.instruction("str x1, [sp, #8]");
    emitter.instruction("str x2, [sp, #16]");
    emitter.instruction("str x3, [sp, #24]");
    emitter.instruction("str x4, [sp, #32]");
    emitter.instruction("str x5, [sp, #40]");
    emitter.instruction("str x6, [sp, #48]");                                   // the subject array
    emitter.instruction("str xzr, [sp, #56]");                                  // element index
    emitter.instruction("ldr x9, [x6]");                                        // how many elements it has
    emitter.instruction("str x9, [sp, #64]");
    // The result array is created up front: `__rt_array_push_str` resolves its argument through
    // `__rt_array_ensure_unique`, which has no null case. An empty subject therefore still answers
    // an ARRAY, which is what php answers.
    emitter.instruction("mov x0, #0");                                          // an empty array to grow into
    emitter.instruction("bl __rt_array_new");
    emitter.instruction("str x0, [sp, #72]");
    // php counts every replacement across the WHOLE call — all elements together — so the tally
    // is accumulated from what each inner call reports. It leaves in `x1`, because the result
    // ARRAY already occupies `x0` here; the scalar-subject helpers, which answer a string pair in
    // `x1`/`x2`, use `x0` for the same purpose.
    emitter.instruction("str xzr, [sp, #80]");                                  // no replacement has fired yet

    emitter.label(&format!("{tag}_loop"));
    emitter.instruction("ldr x9, [sp, #56]");
    emitter.instruction("ldr x10, [sp, #64]");
    emitter.instruction("cmp x9, x10");
    emitter.instruction(&format!("b.ge {tag}_done"));                                 // every element replaced

    // -- subject[i] --
    emitter.instruction("ldr x11, [sp, #48]");
    emitter.instruction("lsl x12, x9, #4");                                     // 16-byte slots
    emitter.instruction("add x12, x12, #24");                                   // past the header
    emitter.instruction("add x12, x11, x12");                                   // &subject[i]
    emitter.instruction("ldr x13, [x12, #0]");                                  // element pointer
    emitter.instruction("ldr x14, [x12, #8]");                                  // element length

    // -- replace inside it, by whichever form $search takes --
    emitter.instruction("ldr x0, [sp, #0]");                                    // the search array, or zero
    emitter.instruction(&format!("cbz x0, {tag}_scalar_search"));
    emitter.instruction("ldr x1, [sp, #24]");                                   // the replace array, or zero
    emitter.instruction("ldr x2, [sp, #32]");                                   // the scalar replacement pointer
    emitter.instruction("ldr x3, [sp, #40]");                                   // and its length
    emitter.instruction("mov x4, x13");                                         // this element is the subject
    emitter.instruction("mov x5, x14");
    emitter.instruction(&format!("bl {inner_search_array}"));                    // x1/x2 = the replaced element
    emitter.instruction(&format!("b {tag}_replaced"));
    emitter.label(&format!("{tag}_scalar_search"));
    emitter.instruction("ldr x1, [sp, #8]");                                    // the scalar search pointer
    emitter.instruction("ldr x2, [sp, #16]");                                   // and its length
    emitter.instruction("ldr x3, [sp, #32]");                                   // the scalar replacement pointer
    emitter.instruction("ldr x4, [sp, #40]");                                   // and its length
    emitter.instruction("mov x5, x13");                                         // this element is the subject
    emitter.instruction("mov x6, x14");
    emitter.instruction(&format!("bl {inner}"));                                 // x1/x2 = the replaced element
    emitter.label(&format!("{tag}_replaced"));
    emitter.instruction("ldr x9, [sp, #80]");                                   // add this element to the running tally
    emitter.instruction("add x9, x9, x0");                                      // before the result array takes x0 back
    emitter.instruction("str x9, [sp, #80]");

    // -- push it onto the result --
    emitter.instruction("ldr x0, [sp, #72]");                                   // the result array so far, or zero
    emitter.instruction("bl __rt_array_push_str");                              // x0 = the array, grown if it had to
    emitter.instruction("str x0, [sp, #72]");
    emitter.instruction("ldr x9, [sp, #56]");
    emitter.instruction("add x9, x9, #1");
    emitter.instruction("str x9, [sp, #56]");
    emitter.instruction(&format!("b {tag}_loop"));

    emitter.label(&format!("{tag}_done"));
    emitter.instruction("ldr x0, [sp, #72]");                                   // the completed array
    emitter.instruction("ldr x1, [sp, #80]");                                   // the whole call's replacement count
    emitter.instruction("ldp x29, x30, [sp, #96]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #112");                                    // release the map frame
    emitter.instruction("ret");
}

/// The x86_64 map.
fn emit_x86_64(
    emitter: &mut Emitter,
    symbol: &str,
    tag: &str,
    inner: &str,
    inner_search_array: &str,
) {
    emitter.blank();
    emitter.comment("--- runtime: str_replace over an array subject ---");
    emitter.label_global(symbol);
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the map frame
    emitter.instruction("sub rsp, 112");                                        // reserve the map slots, plus the replacement tally
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // the search array, or zero
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // the scalar search pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // and its length
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // the replace array, or zero
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // the scalar replacement pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // and its length
    emitter.instruction("mov r10, QWORD PTR [rbp + 16]");                       // the subject array, past the saved rbp and return address
    emitter.instruction("mov QWORD PTR [rbp - 56], r10");
    emitter.instruction("mov QWORD PTR [rbp - 64], 0");                         // element index
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // how many elements it has
    emitter.instruction("mov QWORD PTR [rbp - 72], r11");
    // See the AArch64 counterpart: the result array is created before the loop.
    emitter.instruction("xor edi, edi");                                        // an empty array to grow into
    emitter.instruction("call __rt_array_new");
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");
    emitter.instruction("mov QWORD PTR [rbp - 104], 0");                        // no replacement has fired yet

    emitter.label(&format!("{tag}_loop_x86"));
    emitter.instruction("mov r10, QWORD PTR [rbp - 64]");
    emitter.instruction("cmp r10, QWORD PTR [rbp - 72]");
    emitter.instruction(&format!("jge {tag}_done_x86"));                              // every element replaced

    emitter.instruction("mov r11, QWORD PTR [rbp - 56]");
    emitter.instruction("mov rax, r10");
    emitter.instruction("shl rax, 4");                                          // 16-byte slots
    emitter.instruction("add rax, 24");                                         // past the header
    emitter.instruction("add rax, r11");                                        // &subject[i]
    emitter.instruction("mov r10, QWORD PTR [rax + 0]");                        // element pointer
    emitter.instruction("mov r11, QWORD PTR [rax + 8]");                        // element length
    emitter.instruction("mov QWORD PTR [rbp - 88], r10");                       // both outlive the call setup
    emitter.instruction("mov QWORD PTR [rbp - 96], r11");

    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the search array, or zero
    emitter.instruction("test rdi, rdi");
    emitter.instruction(&format!("jz {tag}_scalar_search_x86"));
    emitter.instruction("mov rsi, QWORD PTR [rbp - 32]");                       // the replace array, or zero
    emitter.instruction("mov rdx, QWORD PTR [rbp - 40]");                       // the scalar replacement pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 48]");                       // and its length
    emitter.instruction("mov r8, QWORD PTR [rbp - 88]");                        // this element is the subject
    emitter.instruction("mov r9, QWORD PTR [rbp - 96]");
    emitter.instruction(&format!("call {inner_search_array}"));                  // rax/rdx = the replaced element
    emitter.instruction(&format!("jmp {tag}_replaced_x86"));
    emitter.label(&format!("{tag}_scalar_search_x86"));
    // `__rt_str_replace` takes rax/rdx = search, rdi/rsi = replace, rcx/r8 = subject.
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // the scalar replacement pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // and its length
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // the scalar search pointer
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // and its length
    emitter.instruction("mov rcx, QWORD PTR [rbp - 88]");                       // this element is the subject
    emitter.instruction("mov r8, QWORD PTR [rbp - 96]");
    emitter.instruction(&format!("call {inner}"));                               // rax/rdx = the replaced element
    emitter.label(&format!("{tag}_replaced_x86"));
    emitter.instruction("add QWORD PTR [rbp - 104], rcx");                      // add this element to the running tally

    emitter.instruction("mov rsi, rax");                                        // the replaced element pointer
    emitter.instruction("mov rdi, QWORD PTR [rbp - 80]");                       // the result array; push takes it in rdi
    emitter.instruction("call __rt_array_push_str");                            // rax = the array, grown if it had to
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");
    emitter.instruction("mov r10, QWORD PTR [rbp - 64]");
    emitter.instruction("inc r10");
    emitter.instruction("mov QWORD PTR [rbp - 64], r10");
    emitter.instruction(&format!("jmp {tag}_loop_x86"));

    emitter.label(&format!("{tag}_done_x86"));
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // the completed array
    emitter.instruction("mov rcx, QWORD PTR [rbp - 104]");                      // the whole call's replacement count
    emitter.instruction("add rsp, 112");                                        // release the map frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");
}

/// Silences the unused-import warning when neither arm references the ABI helper.
#[allow(dead_code)]
fn _abi_used(emitter: &mut Emitter) {
    let _ = abi::int_result_reg(emitter);
}
