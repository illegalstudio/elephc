//! Purpose:
//! Emits `__rt_mixed_min_max`, the runtime selection behind `Op::MixedMinMax`: PHP's
//! variadic `min()` / `max()` between two operands whose runtime types are not both
//! statically numeric.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::arrays`.
//! - Generated code, through `crate::codegen::lower_inst::builtins::math::mixed_min_max`.
//!
//! Key details:
//! - Both operands are **borrowed** Mixed-shaped cells (`[tag][lo][hi]`). A caller may pass a
//!   heap cell, a frame-local numeric cell or a temporary stack cell built from a raw scalar:
//!   the helper reads only those three words and never retains or frees an operand.
//! - The result is an **owned** cell from `__rt_mixed_clone`, a detached copy of the winner
//!   that keeps its original runtime tag, so `max(5.5, 0)` stays a float and
//!   `max("10", 9)` stays a string.
//! - Ordering is `__rt_php_compare`, so the variadic form agrees with the single-array
//!   reductions and with `<`/`>` on PHP 8's comparison table.
//! - The rule argument is `crate::ir::MinMaxRule`. Pair rules compare `left` with `right`
//!   and send every unordered (`NAN`) comparison to the right operand, as php-src's
//!   two-argument `a >= b ? a : b` / `a < b ? a : b` do. Fold rules compare the candidate
//!   (`right`) with the running result (`left`) and replace it only on a strict, ordered
//!   win, so ties and `NAN` keep the earlier argument.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits `__rt_mixed_min_max` for the active target.
///
/// Input: AArch64 `x0` = left cell, `x1` = right cell, `x2` = rule; x86_64 `rdi` = left
/// cell, `rsi` = right cell, `rdx` = rule. Output: `x0` / `rax` = an owned Mixed cell
/// holding a copy of the selected operand.
pub fn emit_mixed_min_max(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_min_max_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: mixed_min_max ---");
    emitter.label_global("__rt_mixed_min_max");

    // Frame (96 bytes): [0]=left cell [8]=right cell [16]=rule
    //                   [24..40]=left tag/lo/hi [48..64]=right tag/lo/hi [80]=x29/x30
    emitter.instruction("sub sp, sp, #96");                                     // allocate the selection frame
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish the selection frame pointer
    emitter.instruction("stp x0, x1, [sp, #0]");                                // save the borrowed left and right cells
    emitter.instruction("str x2, [sp, #16]");                                   // save the min/max rule
    emitter.instruction("bl __rt_mixed_unbox");                                 // peel the left cell into a concrete tag/lo/hi triple
    emitter.instruction("stp x0, x1, [sp, #24]");                               // save the left runtime tag and low payload word
    emitter.instruction("str x2, [sp, #40]");                                   // save the left high payload word
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the borrowed right cell
    emitter.instruction("bl __rt_mixed_unbox");                                 // peel the right cell into a concrete tag/lo/hi triple
    emitter.instruction("stp x0, x1, [sp, #48]");                               // save the right runtime tag and low payload word
    emitter.instruction("str x2, [sp, #64]");                                   // save the right high payload word
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the min/max rule
    emitter.instruction("tbnz x9, #0, __rt_mmx_fold");                          // odd rules fold a candidate into a running result

    // -- pair rules: order left against right --
    emitter.instruction("ldp x0, x1, [sp, #24]");                               // pass the left tag and low word as the first operand
    emitter.instruction("ldr x2, [sp, #40]");                                   // pass the left high word
    emitter.instruction("ldp x3, x4, [sp, #48]");                               // pass the right tag and low word as the second operand
    emitter.instruction("ldr x5, [sp, #64]");                                   // pass the right high word
    emitter.instruction("bl __rt_php_compare");                                 // apply PHP 8's ordering table to left versus right
    emitter.instruction("cbnz x1, __rt_mmx_right");                             // an unordered pair fails both `>=` and `<`, selecting the right operand
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the min/max rule
    emitter.instruction("cbnz x9, __rt_mmx_min_pair");                          // rule 2 is the two-argument min()
    emitter.instruction("cmp x0, #0");                                          // is the left operand greater than or equal to the right one?
    emitter.instruction("b.ge __rt_mmx_left");                                  // max() keeps the left operand on a win or a tie
    emitter.instruction("b __rt_mmx_right");                                    // otherwise the right operand is the maximum
    emitter.label("__rt_mmx_min_pair");
    emitter.instruction("cmp x0, #0");                                          // is the left operand strictly smaller than the right one?
    emitter.instruction("b.lt __rt_mmx_left");                                  // two-argument min() keeps the left operand only on a strict win
    emitter.instruction("b __rt_mmx_right");                                    // a tie returns the right operand, as php-src does

    // -- fold rules: order the candidate against the running result --
    emitter.label("__rt_mmx_fold");
    emitter.instruction("ldp x0, x1, [sp, #48]");                               // pass the candidate tag and low word as the first operand
    emitter.instruction("ldr x2, [sp, #64]");                                   // pass the candidate high word
    emitter.instruction("ldp x3, x4, [sp, #24]");                               // pass the running tag and low word as the second operand
    emitter.instruction("ldr x5, [sp, #40]");                                   // pass the running high word
    emitter.instruction("bl __rt_php_compare");                                 // apply PHP 8's ordering table to candidate versus result
    emitter.instruction("cbnz x1, __rt_mmx_left");                              // an unordered candidate never replaces the running result
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the min/max rule
    emitter.instruction("cmp x9, #1");                                          // is this a max() fold step?
    emitter.instruction("b.ne __rt_mmx_min_fold");                              // rule 3 is the min() fold step
    emitter.instruction("cmp x0, #0");                                          // did the candidate compare greater than the result?
    emitter.instruction("b.gt __rt_mmx_right");                                 // max() replaces only on a strict win
    emitter.instruction("b __rt_mmx_left");                                     // ties keep the earlier argument
    emitter.label("__rt_mmx_min_fold");
    emitter.instruction("cmp x0, #0");                                          // did the candidate compare smaller than the result?
    emitter.instruction("b.lt __rt_mmx_right");                                 // min() replaces only on a strict win

    emitter.label("__rt_mmx_left");
    emitter.instruction("ldr x0, [sp, #0]");                                    // select the borrowed left cell
    emitter.instruction("b __rt_mmx_clone");                                    // copy the winner into an owned cell
    emitter.label("__rt_mmx_right");
    emitter.instruction("ldr x0, [sp, #8]");                                    // select the borrowed right cell
    emitter.label("__rt_mmx_clone");
    emitter.instruction("bl __rt_mixed_clone");                                 // detach the winner into an owned Mixed cell
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the selection frame
    emitter.instruction("ret");                                                 // return the owned winner
}

/// Emits the Linux x86_64 implementation of `__rt_mixed_min_max`.
fn emit_mixed_min_max_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_min_max ---");
    emitter.label_global("__rt_mixed_min_max");

    // Frame (80 bytes below rbp): [-8]=left cell [-16]=right cell [-24]=rule
    //                            [-32..-48]=left tag/lo/hi [-56..-72]=right tag/lo/hi
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the selection frame pointer
    emitter.instruction("sub rsp, 80");                                         // allocate the aligned selection frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the borrowed left cell
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the borrowed right cell
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the min/max rule
    emitter.instruction("mov rax, rdi");                                        // pass the left cell to the unboxing helper
    emitter.instruction("call __rt_mixed_unbox");                               // peel the left cell into a concrete tag/lo/hi triple
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // save the left runtime tag
    emitter.instruction("mov QWORD PTR [rbp - 40], rdi");                       // save the left low payload word
    emitter.instruction("mov QWORD PTR [rbp - 48], rdx");                       // save the left high payload word
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // pass the right cell to the unboxing helper
    emitter.instruction("call __rt_mixed_unbox");                               // peel the right cell into a concrete tag/lo/hi triple
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save the right runtime tag
    emitter.instruction("mov QWORD PTR [rbp - 64], rdi");                       // save the right low payload word
    emitter.instruction("mov QWORD PTR [rbp - 72], rdx");                       // save the right high payload word
    emitter.instruction("test QWORD PTR [rbp - 24], 1");                        // odd rules fold a candidate into a running result
    emitter.instruction("jnz __rt_mmx_fold_x86");                               // take the fold comparison for rules 1 and 3

    // -- pair rules: order left against right --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // pass the left runtime tag as the first operand
    emitter.instruction("mov rsi, QWORD PTR [rbp - 40]");                       // pass the left low payload word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 48]");                       // pass the left high payload word
    emitter.instruction("mov rcx, QWORD PTR [rbp - 56]");                       // pass the right runtime tag as the second operand
    emitter.instruction("mov r8, QWORD PTR [rbp - 64]");                        // pass the right low payload word
    emitter.instruction("mov r9, QWORD PTR [rbp - 72]");                        // pass the right high payload word
    emitter.instruction("call __rt_php_compare");                               // apply PHP 8's ordering table to left versus right
    emitter.instruction("test rdx, rdx");                                       // did the comparison meet an unordered NAN?
    emitter.instruction("jnz __rt_mmx_right_x86");                              // an unordered pair fails both `>=` and `<`, selecting the right operand
    emitter.instruction("cmp QWORD PTR [rbp - 24], 0");                         // is this the two-argument max()?
    emitter.instruction("jne __rt_mmx_min_pair_x86");                           // rule 2 is the two-argument min()
    emitter.instruction("cmp rax, 0");                                          // is the left operand greater than or equal to the right one?
    emitter.instruction("jge __rt_mmx_left_x86");                               // max() keeps the left operand on a win or a tie
    emitter.instruction("jmp __rt_mmx_right_x86");                              // otherwise the right operand is the maximum
    emitter.label("__rt_mmx_min_pair_x86");
    emitter.instruction("cmp rax, 0");                                          // is the left operand strictly smaller than the right one?
    emitter.instruction("jl __rt_mmx_left_x86");                                // two-argument min() keeps the left operand only on a strict win
    emitter.instruction("jmp __rt_mmx_right_x86");                              // a tie returns the right operand, as php-src does

    // -- fold rules: order the candidate against the running result --
    emitter.label("__rt_mmx_fold_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // pass the candidate runtime tag as the first operand
    emitter.instruction("mov rsi, QWORD PTR [rbp - 64]");                       // pass the candidate low payload word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 72]");                       // pass the candidate high payload word
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // pass the running runtime tag as the second operand
    emitter.instruction("mov r8, QWORD PTR [rbp - 40]");                        // pass the running low payload word
    emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                        // pass the running high payload word
    emitter.instruction("call __rt_php_compare");                               // apply PHP 8's ordering table to candidate versus result
    emitter.instruction("test rdx, rdx");                                       // did the comparison meet an unordered NAN?
    emitter.instruction("jnz __rt_mmx_left_x86");                               // an unordered candidate never replaces the running result
    emitter.instruction("cmp QWORD PTR [rbp - 24], 1");                         // is this a max() fold step?
    emitter.instruction("jne __rt_mmx_min_fold_x86");                           // rule 3 is the min() fold step
    emitter.instruction("cmp rax, 0");                                          // did the candidate compare greater than the result?
    emitter.instruction("jg __rt_mmx_right_x86");                               // max() replaces only on a strict win
    emitter.instruction("jmp __rt_mmx_left_x86");                               // ties keep the earlier argument
    emitter.label("__rt_mmx_min_fold_x86");
    emitter.instruction("cmp rax, 0");                                          // did the candidate compare smaller than the result?
    emitter.instruction("jl __rt_mmx_right_x86");                               // min() replaces only on a strict win

    emitter.label("__rt_mmx_left_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // select the borrowed left cell
    emitter.instruction("jmp __rt_mmx_clone_x86");                              // copy the winner into an owned cell
    emitter.label("__rt_mmx_right_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // select the borrowed right cell
    emitter.label("__rt_mmx_clone_x86");
    emitter.instruction("call __rt_mixed_clone");                               // detach the winner into an owned Mixed cell
    emitter.instruction("mov rsp, rbp");                                        // release the selection frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the owned winner
}
