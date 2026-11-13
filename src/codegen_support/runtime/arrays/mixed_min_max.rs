//! Purpose:
//! Emits `__rt_mixed_min_max_select` and `__rt_mixed_min_max_own`, the runtime selection
//! behind `Op::MixedMinMax`: PHP's variadic `min()` / `max()` over operands whose runtime
//! types are not all statically `int` or all statically `float`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::arrays`.
//! - Generated code, through `crate::codegen::lower_inst::arithmetic::min_max`.
//!
//! Key details:
//! - The operands arrive as an array of pointers to **borrowed** Mixed-shaped cells
//!   (`[tag][lo][hi]`). A caller may pass a heap cell, a frame-local numeric cell or a
//!   temporary stack cell built from a raw scalar: the helper reads only those three words
//!   and never retains or frees an operand.
//! - The result is the **borrowed** pointer of the winning operand. The caller copies its
//!   tag and payload into a frame cell or passes it to `__rt_mixed_min_max_own`, so the
//!   result keeps the winner's runtime type: `max(5.5, 0)` stays a float and `max("10", 9)`
//!   a string.
//! - `__rt_mixed_min_max_own` turns that borrowed pointer into an owned result without
//!   allocating when it can: a winner that is a managed heap cell is retained and returned
//!   itself, exactly as returning the argument would. Only a winner outside the heap (a
//!   temporary stack cell, a frame-local cell, static data) is copied by `__rt_mixed_clone`.
//! - The comparison mode is php-src's. A call starts in integer mode when its first operand
//!   is an `int` and in double mode when it is a `float`, and compares numbers directly. A
//!   float meeting an integer result that a double cannot represent, such an integer meeting
//!   a float, or any non-numeric operand switches the rest of the call to the generic
//!   comparison, `__rt_php_compare` with every unordered (`NAN`) pair counted as "greater",
//!   as `zend_compare` reports it.
//! - The rule argument is `crate::ir::MinMaxRule`. A two-argument `max()` keeps its left
//!   operand for `left >= right`, a two-argument `min()` only for `left < right`; a longer
//!   call replaces its running result only on a strict win. An unordered numeric pair
//!   therefore returns the right operand of a two-argument call and keeps the running
//!   result of a longer one, while in generic mode a two-argument `max()` keeps its left
//!   operand and a longer `max()` takes the candidate. `min()` is the same in both modes.
//! - Integer and float cells are read inline, so two numbers never call another helper.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits `__rt_mixed_min_max_select` and `__rt_mixed_min_max_own` for the active target.
///
/// Select input: AArch64 `x0` = array of operand cell pointers, `x1` = operand count (at
/// least two), `x2` = rule; x86_64 `rdi`, `rsi`, `rdx` in the same order. Output: `x0` /
/// `rax` = the borrowed cell pointer of the selected operand.
///
/// Own input: `x0` / `rax` = a borrowed Mixed-shaped cell. Output: `x0` / `rax` = an owned
/// Mixed cell with the same value.
pub fn emit_mixed_min_max(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_min_max_x86_64(emitter);
        emit_mixed_min_max_own_x86_64(emitter);
        return;
    }
    emit_mixed_min_max_select_aarch64(emitter);
    emit_mixed_min_max_own_aarch64(emitter);
}

/// Emits the AArch64 implementation of `__rt_mixed_min_max_select`.
fn emit_mixed_min_max_select_aarch64(emitter: &mut Emitter) {

    emitter.blank();
    emitter.comment("--- runtime: mixed_min_max_select ---");
    emitter.label_global("__rt_mixed_min_max_select");

    // Frame (128 bytes): [0]=operand array [8]=count [16]=rule [24]=candidate index
    //                    [32]=mode (0 integer, 1 double, 2 generic) [40]=result cell
    //                    [48..64]=result tag/lo/hi [72]=candidate cell
    //                    [80..96]=candidate tag/lo/hi [112]=x29/x30
    emitter.instruction("sub sp, sp, #128");                                    // allocate the selection frame
    emitter.instruction("stp x29, x30, [sp, #112]");                            // save frame pointer and return address
    emitter.instruction("add x29, sp, #112");                                   // establish the selection frame pointer
    emitter.instruction("stp x0, x1, [sp, #0]");                                // save the operand pointer array and its length
    emitter.instruction("str x2, [sp, #16]");                                   // save the min/max rule
    emitter.instruction("ldr x0, [x0]");                                        // load the first operand cell
    emitter.instruction("str x0, [sp, #40]");                                   // the first operand is the initial result
    emitter.instruction("cbz x0, __rt_mmx_slow_first");                         // a null cell unboxes to PHP null through the shared helper
    emitter.instruction("ldr x9, [x0]");                                        // load the operand's runtime tag
    emitter.instruction("cbz x9, __rt_mmx_fast_first");                         // an integer cell needs no peeling
    emitter.instruction("cmp x9, #2");                                          // is the operand a float cell?
    emitter.instruction("b.ne __rt_mmx_slow_first");                            // every other tag goes through the unboxing helper
    emitter.label("__rt_mmx_fast_first");
    emitter.instruction("ldr x1, [x0, #8]");                                    // load the numeric payload word
    emitter.instruction("mov x2, #0");                                          // numbers carry no high payload word
    emitter.instruction("mov x0, x9");                                          // return the numeric tag
    emitter.instruction("b __rt_mmx_peeled_first");                             // skip the unboxing helper
    emitter.label("__rt_mmx_slow_first");
    emitter.instruction("bl __rt_mixed_unbox");                                 // peel the cell into a concrete tag/lo/hi triple
    emitter.label("__rt_mmx_peeled_first");
    emitter.instruction("stp x0, x1, [sp, #48]");                               // save the result's runtime tag and low payload word
    emitter.instruction("str x2, [sp, #64]");                                   // save the result's high payload word

    // -- an int starts in integer mode, a float in double mode, anything else in generic mode --
    emitter.instruction("mov x9, #2");                                          // assume the generic zend_compare mode
    emitter.instruction("cbz x0, __rt_mmx_first_int");                          // an integer first operand starts in integer mode
    emitter.instruction("cmp x0, #2");                                          // is the first operand a float?
    emitter.instruction("b.ne __rt_mmx_mode_ready");                            // a non-numeric first operand keeps the generic mode
    emitter.instruction("mov x9, #1");                                          // a float first operand starts in double mode
    emitter.instruction("b __rt_mmx_mode_ready");                               // the starting mode is known
    emitter.label("__rt_mmx_first_int");
    emitter.instruction("mov x9, #0");                                          // integer mode: every operand so far is an int
    emitter.label("__rt_mmx_mode_ready");
    emitter.instruction("str x9, [sp, #32]");                                   // save the comparison mode
    emitter.instruction("mov x9, #1");                                          // the second operand is the first candidate
    emitter.instruction("str x9, [sp, #24]");                                   // save the candidate index
    emitter.label("__rt_mmx_loop");
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the candidate index
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the operand count
    emitter.instruction("cmp x9, x10");                                         // has every operand been considered?
    emitter.instruction("b.ge __rt_mmx_done");                                  // return the selected operand
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the operand pointer array
    emitter.instruction("ldr x0, [x10, x9, lsl #3]");                           // load the candidate cell
    emitter.instruction("str x0, [sp, #72]");                                   // save the candidate cell
    emitter.instruction("cbz x0, __rt_mmx_slow_next");                          // a null cell unboxes to PHP null through the shared helper
    emitter.instruction("ldr x9, [x0]");                                        // load the operand's runtime tag
    emitter.instruction("cbz x9, __rt_mmx_fast_next");                          // an integer cell needs no peeling
    emitter.instruction("cmp x9, #2");                                          // is the operand a float cell?
    emitter.instruction("b.ne __rt_mmx_slow_next");                             // every other tag goes through the unboxing helper
    emitter.label("__rt_mmx_fast_next");
    emitter.instruction("ldr x1, [x0, #8]");                                    // load the numeric payload word
    emitter.instruction("mov x2, #0");                                          // numbers carry no high payload word
    emitter.instruction("mov x0, x9");                                          // return the numeric tag
    emitter.instruction("b __rt_mmx_peeled_next");                              // skip the unboxing helper
    emitter.label("__rt_mmx_slow_next");
    emitter.instruction("bl __rt_mixed_unbox");                                 // peel the cell into a concrete tag/lo/hi triple
    emitter.label("__rt_mmx_peeled_next");
    emitter.instruction("stp x0, x1, [sp, #80]");                               // save the candidate's runtime tag and low payload word
    emitter.instruction("str x2, [sp, #96]");                                   // save the candidate's high payload word
    emitter.instruction("ldr x9, [sp, #32]");                                   // reload the comparison mode
    emitter.instruction("ldr x10, [sp, #48]");                                  // reload the result's runtime tag
    emitter.instruction("ldr x11, [sp, #56]");                                  // reload the result's low payload word
    emitter.instruction("cmp x9, #2");                                          // has the call already switched to generic comparison?
    emitter.instruction("b.eq __rt_mmx_compare");                               // generic mode is permanent
    emitter.instruction("cbnz x9, __rt_mmx_double_mode");                       // mode 1 compares doubles

    // -- integer mode: an int candidate stays, a float needs the integer result as a double --
    emitter.instruction("cbz x0, __rt_mmx_compare");                            // two integers compare exactly
    emitter.instruction("cmp x0, #2");                                          // is the candidate a float?
    emitter.instruction("b.ne __rt_mmx_enter_generic");                         // any other candidate switches to generic comparison
    emitter.instruction("scvtf d2, x11");                                       // convert the integer result to a double
    emitter.instruction("fcvtzs x13, d2");                                      // convert it back to an integer
    emitter.instruction("cmp x13, x11");                                        // did the round trip preserve the integer?
    emitter.instruction("b.ne __rt_mmx_enter_generic");                         // an integer a double cannot represent compares generically
    emitter.instruction("mov x13, #0x7fffffffffffffff");                        // the largest integer rounds up to 2^63 and saturates back
    emitter.instruction("cmp x11, x13");                                        // is the result that unrepresentable integer?
    emitter.instruction("b.eq __rt_mmx_enter_generic");                         // it compares generically too
    emitter.instruction("mov x9, #1");                                          // continue in double mode
    emitter.instruction("str x9, [sp, #32]");                                   // save the comparison mode
    emitter.instruction("b __rt_mmx_compare");                                  // compare the pair as doubles
    emitter.label("__rt_mmx_double_mode");
    emitter.instruction("cmp x0, #2");                                          // is the candidate a float?
    emitter.instruction("b.eq __rt_mmx_compare");                               // two doubles compare directly
    emitter.instruction("cbnz x0, __rt_mmx_enter_generic");                     // a non-numeric candidate switches to generic comparison
    emitter.instruction("scvtf d2, x1");                                        // convert the integer candidate to a double
    emitter.instruction("fcvtzs x13, d2");                                      // convert it back to an integer
    emitter.instruction("cmp x13, x1");                                         // did the round trip preserve the integer?
    emitter.instruction("b.ne __rt_mmx_enter_generic");                         // an integer a double cannot represent compares generically
    emitter.instruction("mov x13, #0x7fffffffffffffff");                        // the largest integer rounds up to 2^63 and saturates back
    emitter.instruction("cmp x1, x13");                                         // is the candidate that unrepresentable integer?
    emitter.instruction("b.ne __rt_mmx_compare");                               // a representable integer compares as a double
    emitter.label("__rt_mmx_enter_generic");
    emitter.instruction("mov x9, #2");                                          // generic comparison for the rest of the call
    emitter.instruction("str x9, [sp, #32]");                                   // save the comparison mode

    // -- order result against candidate: x12 = -1/0/1, x13 = unordered --
    emitter.label("__rt_mmx_compare");
    emitter.instruction("cbz x10, __rt_mmx_result_numeric");                    // an integer result compares numerically
    emitter.instruction("cmp x10, #2");                                         // is the result a float?
    emitter.instruction("b.ne __rt_mmx_generic_compare");                       // a non-numeric result needs PHP's comparison table
    emitter.label("__rt_mmx_result_numeric");
    emitter.instruction("cbz x0, __rt_mmx_both_numeric");                       // an integer candidate compares numerically
    emitter.instruction("cmp x0, #2");                                          // is the candidate a float?
    emitter.instruction("b.ne __rt_mmx_generic_compare");                       // a non-numeric candidate needs PHP's comparison table
    emitter.label("__rt_mmx_both_numeric");
    emitter.instruction("orr x12, x10, x0");                                    // a zero combined tag means two integers
    emitter.instruction("cbnz x12, __rt_mmx_float_compare");                    // any float compares the pair as doubles
    emitter.instruction("cmp x11, x1");                                         // order the integer result against the integer candidate
    emitter.instruction("cset x12, gt");                                        // 1 when the result is greater
    emitter.instruction("csinv x12, x12, xzr, ge");                             // -1 when the result is smaller
    emitter.instruction("mov x13, #0");                                         // integers are always ordered
    emitter.instruction("b __rt_mmx_decide");                                   // apply the rule
    emitter.label("__rt_mmx_float_compare");
    emitter.instruction("cbnz x10, __rt_mmx_result_float");                     // a float result is used as stored
    emitter.instruction("scvtf d0, x11");                                       // promote the integer result to a double
    emitter.instruction("b __rt_mmx_result_ready");                             // the result double is ready
    emitter.label("__rt_mmx_result_float");
    emitter.instruction("fmov d0, x11");                                        // move the result's float bits into a float register
    emitter.label("__rt_mmx_result_ready");
    emitter.instruction("cbnz x0, __rt_mmx_candidate_float");                   // a float candidate is used as stored
    emitter.instruction("scvtf d1, x1");                                        // promote the integer candidate to a double
    emitter.instruction("b __rt_mmx_candidate_ready");                          // the candidate double is ready
    emitter.label("__rt_mmx_candidate_float");
    emitter.instruction("fmov d1, x1");                                         // move the candidate's float bits into a float register
    emitter.label("__rt_mmx_candidate_ready");
    emitter.instruction("fcmp d0, d1");                                         // order the result double against the candidate double
    emitter.instruction("cset x13, vs");                                        // a NAN on either side makes the pair unordered
    emitter.instruction("cset x12, gt");                                        // 1 when the result is greater
    emitter.instruction("csinv x12, x12, xzr, ge");                             // -1 when the result is smaller

    // -- apply the rule to the ordering --
    emitter.label("__rt_mmx_decide");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the min/max rule
    emitter.instruction("cbnz x13, __rt_mmx_unordered");                        // an unordered pair is decided by rule and mode
    emitter.instruction("cmp x9, #2");                                          // split the max rules from the min rules
    emitter.instruction("b.lo __rt_mmx_max_ordered");                           // rules 0 and 1 are max()
    emitter.instruction("b.eq __rt_mmx_min_pair_ordered");                      // rule 2 is the two-argument min()
    emitter.instruction("cmp x12, #0");                                         // is the result greater than the candidate?
    emitter.instruction("b.gt __rt_mmx_replace");                               // a longer min() replaces only on a strict win
    emitter.instruction("b __rt_mmx_next");                                     // ties keep the earlier argument
    emitter.label("__rt_mmx_min_pair_ordered");
    emitter.instruction("cmp x12, #0");                                         // is the left operand strictly smaller than the right one?
    emitter.instruction("b.ge __rt_mmx_replace");                               // two-argument min() returns its right operand on a tie
    emitter.instruction("b __rt_mmx_next");                                     // the left operand is the minimum
    emitter.label("__rt_mmx_max_ordered");
    emitter.instruction("cmp x12, #0");                                         // is the result smaller than the candidate?
    emitter.instruction("b.lt __rt_mmx_replace");                               // max() replaces only on a strict win
    emitter.instruction("b __rt_mmx_next");                                     // ties keep the earlier argument
    emitter.label("__rt_mmx_unordered");
    emitter.instruction("cmp x9, #2");                                          // split the max rules from the min rules
    emitter.instruction("b.eq __rt_mmx_replace");                               // two-argument min() fails `left < right` and returns the right operand
    emitter.instruction("b.hi __rt_mmx_next");                                  // a longer min() never replaces on an unordered comparison
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload the comparison mode
    emitter.instruction("cmp x10, #2");                                         // is the call in generic mode?
    emitter.instruction("cset x10, eq");                                        // 1 in generic mode, 0 in a numeric mode
    emitter.instruction("cmp x10, x9");                                         // pair max replaces only numerically, a longer max only generically
    emitter.instruction("b.eq __rt_mmx_replace");                               // zend_compare reports unordered as greater; `>=` and `<` as false
    emitter.instruction("b __rt_mmx_next");                                     // keep the running result

    // -- generic mode: a non-numeric operand uses PHP's comparison table --
    emitter.label("__rt_mmx_generic_compare");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the min/max rule
    emitter.instruction("tbnz x9, #0, __rt_mmx_generic_fold");                  // a longer call compares the candidate against the result
    emitter.instruction("ldp x0, x1, [sp, #48]");                               // pass the left tag and low word as the first operand
    emitter.instruction("ldr x2, [sp, #64]");                                   // pass the left high word
    emitter.instruction("ldp x3, x4, [sp, #80]");                               // pass the right tag and low word as the second operand
    emitter.instruction("ldr x5, [sp, #96]");                                   // pass the right high word
    emitter.instruction("bl __rt_php_compare");                                 // apply PHP 8's ordering table to left versus right
    emitter.instruction("mov x12, x0");                                         // keep the ordering of left against right
    emitter.instruction("mov x13, x1");                                         // keep the unordered flag
    emitter.instruction("b __rt_mmx_decide");                                   // apply the rule
    emitter.label("__rt_mmx_generic_fold");
    emitter.instruction("ldp x0, x1, [sp, #80]");                               // pass the candidate tag and low word as the first operand
    emitter.instruction("ldr x2, [sp, #96]");                                   // pass the candidate high word
    emitter.instruction("ldp x3, x4, [sp, #48]");                               // pass the result tag and low word as the second operand
    emitter.instruction("ldr x5, [sp, #64]");                                   // pass the result high word
    emitter.instruction("bl __rt_php_compare");                                 // apply PHP 8's ordering table to candidate versus result
    emitter.instruction("neg x12, x0");                                         // express the ordering as result against candidate
    emitter.instruction("mov x13, x1");                                         // keep the unordered flag
    emitter.instruction("b __rt_mmx_decide");                                   // apply the rule
    emitter.label("__rt_mmx_replace");
    emitter.instruction("ldr x9, [sp, #72]");                                   // reload the candidate cell
    emitter.instruction("str x9, [sp, #40]");                                   // the candidate becomes the result
    emitter.instruction("ldp x9, x10, [sp, #80]");                              // reload the candidate's tag and low payload word
    emitter.instruction("stp x9, x10, [sp, #48]");                              // they become the result's tag and low payload word
    emitter.instruction("ldr x9, [sp, #96]");                                   // reload the candidate's high payload word
    emitter.instruction("str x9, [sp, #64]");                                   // it becomes the result's high payload word
    emitter.label("__rt_mmx_next");
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the candidate index
    emitter.instruction("add x9, x9, #1");                                      // advance to the next operand
    emitter.instruction("str x9, [sp, #24]");                                   // save the candidate index
    emitter.instruction("b __rt_mmx_loop");                                     // consider the next operand
    emitter.label("__rt_mmx_done");
    emitter.instruction("ldr x0, [sp, #40]");                                   // return the borrowed cell of the selected operand
    emitter.instruction("ldp x29, x30, [sp, #112]");                            // restore frame pointer and return address
    emitter.instruction("add sp, sp, #128");                                    // release the selection frame
    emitter.instruction("ret");                                                 // return to the caller
}

/// Emits the Linux x86_64 implementation of `__rt_mixed_min_max_select`.
fn emit_mixed_min_max_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_min_max_select ---");
    emitter.label_global("__rt_mixed_min_max_select");

    // Frame (below rbp): [-8]=operand array [-16]=count [-24]=rule [-32]=candidate index
    //                    [-40]=mode (0 integer, 1 double, 2 generic) [-48]=result cell
    //                    [-56..-72]=result tag/lo/hi [-80]=candidate cell
    //                    [-88..-104]=candidate tag/lo/hi
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the selection frame pointer
    emitter.instruction("sub rsp, 112");                                        // allocate the aligned selection frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the operand pointer array
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the operand count
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the min/max rule
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // load the first operand cell
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // the first operand is the initial result
    emitter.instruction("test rax, rax");                                       // a null cell unboxes to PHP null through the shared helper
    emitter.instruction("jz __rt_mmx_slow_first_x86");                          // take the unboxing helper for a null cell
    emitter.instruction("mov rcx, QWORD PTR [rax]");                            // load the operand's runtime tag
    emitter.instruction("test rcx, rcx");                                       // is the operand an integer cell?
    emitter.instruction("jz __rt_mmx_fast_first_x86");                          // an integer cell needs no peeling
    emitter.instruction("cmp rcx, 2");                                          // is the operand a float cell?
    emitter.instruction("jne __rt_mmx_slow_first_x86");                         // every other tag goes through the unboxing helper
    emitter.label("__rt_mmx_fast_first_x86");
    emitter.instruction("mov rdi, QWORD PTR [rax + 8]");                        // load the numeric payload word
    emitter.instruction("xor edx, edx");                                        // numbers carry no high payload word
    emitter.instruction("mov rax, rcx");                                        // return the numeric tag
    emitter.instruction("jmp __rt_mmx_peeled_first_x86");                       // skip the unboxing helper
    emitter.label("__rt_mmx_slow_first_x86");
    emitter.instruction("call __rt_mixed_unbox");                               // peel the cell into a concrete tag/lo/hi triple
    emitter.label("__rt_mmx_peeled_first_x86");
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save the result's runtime tag
    emitter.instruction("mov QWORD PTR [rbp - 64], rdi");                       // save the result's low payload word
    emitter.instruction("mov QWORD PTR [rbp - 72], rdx");                       // save the result's high payload word

    // -- an int starts in integer mode, a float in double mode, anything else in generic mode --
    emitter.instruction("mov rcx, 2");                                          // assume the generic zend_compare mode
    emitter.instruction("test rax, rax");                                       // is the first operand an integer?
    emitter.instruction("jz __rt_mmx_first_int_x86");                           // an integer first operand starts in integer mode
    emitter.instruction("cmp rax, 2");                                          // is the first operand a float?
    emitter.instruction("jne __rt_mmx_mode_ready_x86");                         // a non-numeric first operand keeps the generic mode
    emitter.instruction("mov rcx, 1");                                          // a float first operand starts in double mode
    emitter.instruction("jmp __rt_mmx_mode_ready_x86");                         // the starting mode is known
    emitter.label("__rt_mmx_first_int_x86");
    emitter.instruction("xor ecx, ecx");                                        // integer mode: every operand so far is an int
    emitter.label("__rt_mmx_mode_ready_x86");
    emitter.instruction("mov QWORD PTR [rbp - 40], rcx");                       // save the comparison mode
    emitter.instruction("mov QWORD PTR [rbp - 32], 1");                         // the second operand is the first candidate
    emitter.label("__rt_mmx_loop_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // reload the candidate index
    emitter.instruction("cmp rcx, QWORD PTR [rbp - 16]");                       // has every operand been considered?
    emitter.instruction("jge __rt_mmx_done_x86");                               // return the selected operand
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the operand pointer array
    emitter.instruction("mov rax, QWORD PTR [rax + rcx * 8]");                  // load the candidate cell
    emitter.instruction("mov QWORD PTR [rbp - 80], rax");                       // save the candidate cell
    emitter.instruction("test rax, rax");                                       // a null cell unboxes to PHP null through the shared helper
    emitter.instruction("jz __rt_mmx_slow_next_x86");                           // take the unboxing helper for a null cell
    emitter.instruction("mov rcx, QWORD PTR [rax]");                            // load the operand's runtime tag
    emitter.instruction("test rcx, rcx");                                       // is the operand an integer cell?
    emitter.instruction("jz __rt_mmx_fast_next_x86");                           // an integer cell needs no peeling
    emitter.instruction("cmp rcx, 2");                                          // is the operand a float cell?
    emitter.instruction("jne __rt_mmx_slow_next_x86");                          // every other tag goes through the unboxing helper
    emitter.label("__rt_mmx_fast_next_x86");
    emitter.instruction("mov rdi, QWORD PTR [rax + 8]");                        // load the numeric payload word
    emitter.instruction("xor edx, edx");                                        // numbers carry no high payload word
    emitter.instruction("mov rax, rcx");                                        // return the numeric tag
    emitter.instruction("jmp __rt_mmx_peeled_next_x86");                        // skip the unboxing helper
    emitter.label("__rt_mmx_slow_next_x86");
    emitter.instruction("call __rt_mixed_unbox");                               // peel the cell into a concrete tag/lo/hi triple
    emitter.label("__rt_mmx_peeled_next_x86");
    emitter.instruction("mov QWORD PTR [rbp - 88], rax");                       // save the candidate's runtime tag
    emitter.instruction("mov QWORD PTR [rbp - 96], rdi");                       // save the candidate's low payload word
    emitter.instruction("mov QWORD PTR [rbp - 104], rdx");                      // save the candidate's high payload word
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // reload the comparison mode
    emitter.instruction("mov r8, QWORD PTR [rbp - 56]");                        // reload the result's runtime tag
    emitter.instruction("mov r9, QWORD PTR [rbp - 64]");                        // reload the result's low payload word
    emitter.instruction("cmp rcx, 2");                                          // has the call already switched to generic comparison?
    emitter.instruction("je __rt_mmx_compare_x86");                             // generic mode is permanent
    emitter.instruction("test rcx, rcx");                                       // is the call still in integer mode?
    emitter.instruction("jnz __rt_mmx_double_mode_x86");                        // mode 1 compares doubles

    // -- integer mode: an int candidate stays, a float needs the integer result as a double --
    emitter.instruction("test rax, rax");                                       // is the candidate an integer?
    emitter.instruction("jz __rt_mmx_compare_x86");                             // two integers compare exactly
    emitter.instruction("cmp rax, 2");                                          // is the candidate a float?
    emitter.instruction("jne __rt_mmx_enter_generic_x86");                      // any other candidate switches to generic comparison
    emitter.instruction("cvtsi2sd xmm2, r9");                                   // convert the integer result to a double
    emitter.instruction("cvttsd2si r10, xmm2");                                 // convert it back; 2^63 yields the integer indefinite value
    emitter.instruction("cmp r10, r9");                                         // did the round trip preserve the integer?
    emitter.instruction("jne __rt_mmx_enter_generic_x86");                      // an integer a double cannot represent compares generically
    emitter.instruction("mov QWORD PTR [rbp - 40], 1");                         // continue in double mode
    emitter.instruction("jmp __rt_mmx_compare_x86");                            // compare the pair as doubles
    emitter.label("__rt_mmx_double_mode_x86");
    emitter.instruction("cmp rax, 2");                                          // is the candidate a float?
    emitter.instruction("je __rt_mmx_compare_x86");                             // two doubles compare directly
    emitter.instruction("test rax, rax");                                       // is the candidate an integer?
    emitter.instruction("jnz __rt_mmx_enter_generic_x86");                      // a non-numeric candidate switches to generic comparison
    emitter.instruction("cvtsi2sd xmm2, rdi");                                  // convert the integer candidate to a double
    emitter.instruction("cvttsd2si r10, xmm2");                                 // convert it back; 2^63 yields the integer indefinite value
    emitter.instruction("cmp r10, rdi");                                        // did the round trip preserve the integer?
    emitter.instruction("je __rt_mmx_compare_x86");                             // a representable integer compares as a double
    emitter.label("__rt_mmx_enter_generic_x86");
    emitter.instruction("mov QWORD PTR [rbp - 40], 2");                         // generic comparison for the rest of the call

    // -- order result against candidate: r11 = -1/0/1, r10 = unordered --
    emitter.label("__rt_mmx_compare_x86");
    emitter.instruction("test r8, r8");                                         // is the result an integer?
    emitter.instruction("jz __rt_mmx_result_numeric_x86");                      // an integer result compares numerically
    emitter.instruction("cmp r8, 2");                                           // is the result a float?
    emitter.instruction("jne __rt_mmx_generic_compare_x86");                    // a non-numeric result needs PHP's comparison table
    emitter.label("__rt_mmx_result_numeric_x86");
    emitter.instruction("test rax, rax");                                       // is the candidate an integer?
    emitter.instruction("jz __rt_mmx_both_numeric_x86");                        // an integer candidate compares numerically
    emitter.instruction("cmp rax, 2");                                          // is the candidate a float?
    emitter.instruction("jne __rt_mmx_generic_compare_x86");                    // a non-numeric candidate needs PHP's comparison table
    emitter.label("__rt_mmx_both_numeric_x86");
    emitter.instruction("xor r10d, r10d");                                      // assume an ordered pair
    emitter.instruction("xor r11d, r11d");                                      // assume equal operands
    emitter.instruction("mov rcx, r8");                                         // copy the result tag
    emitter.instruction("or rcx, rax");                                         // a zero combined tag means two integers
    emitter.instruction("jnz __rt_mmx_float_compare_x86");                      // any float compares the pair as doubles
    emitter.instruction("cmp r9, rdi");                                         // order the integer result against the integer candidate
    emitter.instruction("mov rcx, 1");                                          // ordering for a greater result
    emitter.instruction("cmovg r11, rcx");                                      // 1 when the result is greater
    emitter.instruction("mov rcx, -1");                                         // ordering for a smaller result
    emitter.instruction("cmovl r11, rcx");                                      // -1 when the result is smaller
    emitter.instruction("jmp __rt_mmx_decide_x86");                             // apply the rule
    emitter.label("__rt_mmx_float_compare_x86");
    emitter.instruction("test r8, r8");                                         // is the result an integer?
    emitter.instruction("jnz __rt_mmx_result_float_x86");                       // a float result is used as stored
    emitter.instruction("cvtsi2sd xmm0, r9");                                   // promote the integer result to a double
    emitter.instruction("jmp __rt_mmx_result_ready_x86");                       // the result double is ready
    emitter.label("__rt_mmx_result_float_x86");
    emitter.instruction("movq xmm0, r9");                                       // move the result's float bits into a float register
    emitter.label("__rt_mmx_result_ready_x86");
    emitter.instruction("test rax, rax");                                       // is the candidate an integer?
    emitter.instruction("jnz __rt_mmx_candidate_float_x86");                    // a float candidate is used as stored
    emitter.instruction("cvtsi2sd xmm1, rdi");                                  // promote the integer candidate to a double
    emitter.instruction("jmp __rt_mmx_candidate_ready_x86");                    // the candidate double is ready
    emitter.label("__rt_mmx_candidate_float_x86");
    emitter.instruction("movq xmm1, rdi");                                      // move the candidate's float bits into a float register
    emitter.label("__rt_mmx_candidate_ready_x86");
    emitter.instruction("ucomisd xmm0, xmm1");                                  // order the result double against the candidate double
    emitter.instruction("jp __rt_mmx_nan_x86");                                 // a NAN on either side makes the pair unordered
    emitter.instruction("mov rcx, 1");                                          // ordering for a greater result
    emitter.instruction("cmova r11, rcx");                                      // 1 when the result is greater
    emitter.instruction("mov rcx, -1");                                         // ordering for a smaller result
    emitter.instruction("cmovb r11, rcx");                                      // -1 when the result is smaller
    emitter.instruction("jmp __rt_mmx_decide_x86");                             // apply the rule
    emitter.label("__rt_mmx_nan_x86");
    emitter.instruction("mov r10, 1");                                          // flag the unordered pair

    // -- apply the rule to the ordering --
    emitter.label("__rt_mmx_decide_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 24]");                       // reload the min/max rule
    emitter.instruction("test r10, r10");                                       // did the comparison meet an unordered NAN?
    emitter.instruction("jnz __rt_mmx_unordered_x86");                          // an unordered pair is decided by rule and mode
    emitter.instruction("cmp rcx, 2");                                          // split the max rules from the min rules
    emitter.instruction("jb __rt_mmx_max_ordered_x86");                         // rules 0 and 1 are max()
    emitter.instruction("je __rt_mmx_min_pair_ordered_x86");                    // rule 2 is the two-argument min()
    emitter.instruction("cmp r11, 0");                                          // is the result greater than the candidate?
    emitter.instruction("jg __rt_mmx_replace_x86");                             // a longer min() replaces only on a strict win
    emitter.instruction("jmp __rt_mmx_next_x86");                               // ties keep the earlier argument
    emitter.label("__rt_mmx_min_pair_ordered_x86");
    emitter.instruction("cmp r11, 0");                                          // is the left operand strictly smaller than the right one?
    emitter.instruction("jge __rt_mmx_replace_x86");                            // two-argument min() returns its right operand on a tie
    emitter.instruction("jmp __rt_mmx_next_x86");                               // the left operand is the minimum
    emitter.label("__rt_mmx_max_ordered_x86");
    emitter.instruction("cmp r11, 0");                                          // is the result smaller than the candidate?
    emitter.instruction("jl __rt_mmx_replace_x86");                             // max() replaces only on a strict win
    emitter.instruction("jmp __rt_mmx_next_x86");                               // ties keep the earlier argument
    emitter.label("__rt_mmx_unordered_x86");
    emitter.instruction("cmp rcx, 2");                                          // split the max rules from the min rules
    emitter.instruction("je __rt_mmx_replace_x86");                             // two-argument min() fails `left < right` and returns the right operand
    emitter.instruction("ja __rt_mmx_next_x86");                                // a longer min() never replaces on an unordered comparison
    emitter.instruction("xor eax, eax");                                        // assume a numeric comparison mode
    emitter.instruction("cmp QWORD PTR [rbp - 40], 2");                         // is the call in generic mode?
    emitter.instruction("sete al");                                             // 1 in generic mode, 0 in a numeric mode
    emitter.instruction("cmp rax, rcx");                                        // pair max replaces only numerically, a longer max only generically
    emitter.instruction("je __rt_mmx_replace_x86");                             // zend_compare reports unordered as greater; `>=` and `<` as false
    emitter.instruction("jmp __rt_mmx_next_x86");                               // keep the running result

    // -- generic mode: a non-numeric operand uses PHP's comparison table --
    emitter.label("__rt_mmx_generic_compare_x86");
    emitter.instruction("test QWORD PTR [rbp - 24], 1");                        // odd rules compare the candidate against the result
    emitter.instruction("jnz __rt_mmx_generic_fold_x86");                       // take the longer-call operand order
    emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                       // pass the left runtime tag as the first operand
    emitter.instruction("mov rsi, QWORD PTR [rbp - 64]");                       // pass the left low payload word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 72]");                       // pass the left high payload word
    emitter.instruction("mov rcx, QWORD PTR [rbp - 88]");                       // pass the right runtime tag as the second operand
    emitter.instruction("mov r8, QWORD PTR [rbp - 96]");                        // pass the right low payload word
    emitter.instruction("mov r9, QWORD PTR [rbp - 104]");                       // pass the right high payload word
    emitter.instruction("call __rt_php_compare");                               // apply PHP 8's ordering table to left versus right
    emitter.instruction("mov r11, rax");                                        // keep the ordering of left against right
    emitter.instruction("mov r10, rdx");                                        // keep the unordered flag
    emitter.instruction("jmp __rt_mmx_decide_x86");                             // apply the rule
    emitter.label("__rt_mmx_generic_fold_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 88]");                       // pass the candidate runtime tag as the first operand
    emitter.instruction("mov rsi, QWORD PTR [rbp - 96]");                       // pass the candidate low payload word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 104]");                      // pass the candidate high payload word
    emitter.instruction("mov rcx, QWORD PTR [rbp - 56]");                       // pass the result runtime tag as the second operand
    emitter.instruction("mov r8, QWORD PTR [rbp - 64]");                        // pass the result low payload word
    emitter.instruction("mov r9, QWORD PTR [rbp - 72]");                        // pass the result high payload word
    emitter.instruction("call __rt_php_compare");                               // apply PHP 8's ordering table to candidate versus result
    emitter.instruction("mov r11, rax");                                        // copy the ordering of candidate against result
    emitter.instruction("neg r11");                                             // express the ordering as result against candidate
    emitter.instruction("mov r10, rdx");                                        // keep the unordered flag
    emitter.instruction("jmp __rt_mmx_decide_x86");                             // apply the rule
    emitter.label("__rt_mmx_replace_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // reload the candidate cell
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // the candidate becomes the result
    emitter.instruction("mov rax, QWORD PTR [rbp - 88]");                       // reload the candidate's runtime tag
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // it becomes the result's runtime tag
    emitter.instruction("mov rax, QWORD PTR [rbp - 96]");                       // reload the candidate's low payload word
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // it becomes the result's low payload word
    emitter.instruction("mov rax, QWORD PTR [rbp - 104]");                      // reload the candidate's high payload word
    emitter.instruction("mov QWORD PTR [rbp - 72], rax");                       // it becomes the result's high payload word
    emitter.label("__rt_mmx_next_x86");
    emitter.instruction("add QWORD PTR [rbp - 32], 1");                         // advance to the next operand
    emitter.instruction("jmp __rt_mmx_loop_x86");                               // consider the next operand
    emitter.label("__rt_mmx_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");                       // return the borrowed cell of the selected operand
    emitter.instruction("mov rsp, rbp");                                        // release the selection frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the caller
}

/// Emits the AArch64 implementation of `__rt_mixed_min_max_own`.
fn emit_mixed_min_max_own_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_min_max_own ---");
    emitter.label_global("__rt_mixed_min_max_own");
    emitter.instruction("cbz x0, __rt_mmx_own_copy");                           // a null cell is copied as PHP null
    crate::codegen_support::abi::emit_symbol_address(emitter, "x9", "_heap_buf");
    emitter.instruction("cmp x0, x9");                                          // is the cell below the managed heap?
    emitter.instruction("b.lo __rt_mmx_own_copy");                              // static data has no heap header to retain
    crate::codegen_support::abi::emit_symbol_address(emitter, "x10", "_heap_off");
    emitter.instruction("ldr x10, [x10]");                                      // load the current heap extent
    emitter.instruction("add x10, x9, x10");                                    // compute the managed heap end
    emitter.instruction("cmp x0, x10");                                         // is the cell beyond the managed heap?
    emitter.instruction("b.hs __rt_mmx_own_copy");                              // a stack or frame-local cell must be copied to outlive its frame
    emitter.instruction("b __rt_incref");                                       // tail-call: retain the heap cell and return it as the owned result
    emitter.label("__rt_mmx_own_copy");
    emitter.instruction("b __rt_mixed_clone");                                  // tail-call: copy the cell into an owned heap cell
}

/// Emits the Linux x86_64 implementation of `__rt_mixed_min_max_own`.
fn emit_mixed_min_max_own_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_min_max_own ---");
    emitter.label_global("__rt_mixed_min_max_own");
    emitter.instruction("test rax, rax");                                       // is the cell a null pointer?
    emitter.instruction("jz __rt_mmx_own_copy_x86");                            // a null cell is copied as PHP null
    crate::codegen_support::abi::emit_symbol_address(emitter, "r10", "_heap_buf");
    emitter.instruction("lea r11, [r10 + 16]");                                 // first valid user payload begins after the initial heap header
    emitter.instruction("cmp rax, r11");                                        // is the cell below the managed heap?
    emitter.instruction("jb __rt_mmx_own_copy_x86");                            // static data has no heap header to retain
    crate::codegen_support::abi::emit_symbol_address(emitter, "r11", "_heap_off");
    emitter.instruction("mov r11, QWORD PTR [r11]");                            // load the current heap extent
    emitter.instruction("add r11, r10");                                        // compute the managed heap end
    emitter.instruction("cmp rax, r11");                                        // is the cell beyond the managed heap?
    emitter.instruction("jae __rt_mmx_own_copy_x86");                           // a stack or frame-local cell must be copied to outlive its frame
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // load the heap kind word from the uniform header
    emitter.instruction("shr r10, 32");                                         // isolate the x86_64 heap marker
    emitter.instruction(&format!("cmp r10d, 0x{:x}", crate::codegen_support::sentinels::X86_64_HEAP_MAGIC_HI32)); // does the cell carry an elephc heap header?
    emitter.instruction("jne __rt_mmx_own_copy_x86");                           // a foreign pointer is copied instead of retained
    emitter.instruction("jmp __rt_incref");                                     // tail-call: retain the heap cell and return it as the owned result
    emitter.label("__rt_mmx_own_copy_x86");
    emitter.instruction("jmp __rt_mixed_clone");                                // tail-call: copy the cell into an owned heap cell
}
