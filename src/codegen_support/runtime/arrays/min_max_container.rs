//! Purpose:
//! Emits `__rt_min_max_mixed`, `__rt_min_max_str` and `__rt_min_max_hash`, the runtime
//! reductions behind PHP's single-array `min()` / `max()` form for the container shapes
//! whose elements cannot be compared as raw 8-byte scalar words: indexed arrays of boxed
//! `Mixed` cells, indexed arrays of strings, and hash-backed associative arrays.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::arrays`.
//! - Generated code, through
//!   `crate::codegen::lower_inst::builtins::math::min_max_array`.
//!
//! Key details:
//! - All three helpers implement PHP's own reduction: the first element seeds the result
//!   and a later element only replaces it on a strict win, so ties keep the *earlier*
//!   element and the winner keeps its original runtime tag.
//! - Comparison is delegated to `__rt_php_compare`, so every container shape agrees on
//!   PHP 8's ordering table.
//! - The result is the unboxed `(tag, lo, hi)` triple of the winning element: AArch64
//!   `x0`/`x1`/`x2`, x86_64 `rax`/`rdi`/`rsi` (the exact input registers of
//!   `__rt_mixed_from_value`, so the caller can box it with one call). Tag `-1` reports
//!   an empty or null container, which the caller turns into PHP's `ValueError`.
//! - String payloads stay **borrowed** from the container: nothing is persisted, retained
//!   or released here.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Byte offset of the first payload slot inside an indexed-array allocation.
const ARRAY_DATA_OFFSET: i64 = 24;

/// Emits `__rt_min_max_mixed`: reduces an indexed array of boxed `Mixed` cells.
///
/// Input: AArch64 `x0` = array pointer, `x1` = 1 for `max()` and 0 for `min()`;
/// x86_64 `rdi` = array pointer, `rsi` = the same flag. Output is the winning
/// element's unboxed triple, or tag `-1` when the array holds no element.
/// Element cells stay borrowed.
pub fn emit_min_max_mixed(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_min_max_mixed_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: min_max_mixed ---");
    emitter.label_global("__rt_min_max_mixed");

    // Frame (96 bytes): [0]=array [8]=length [16]=cursor [24]=want_max
    //                   [32..48]=best tag/lo/hi [56..72]=candidate tag/lo/hi [80]=x29/x30
    emitter.instruction("sub sp, sp, #96");                                     // allocate the reduction frame
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish the reduction frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the source indexed-array pointer
    emitter.instruction("str x1, [sp, #24]");                                   // save the min/max direction flag
    emitter.instruction("mov x9, #-1");                                         // tag -1 reports "the container yielded no element"
    emitter.instruction("str x9, [sp, #32]");                                   // seed the running result with the empty sentinel
    emitter.instruction("str xzr, [sp, #40]");                                  // clear the running low payload word
    emitter.instruction("str xzr, [sp, #48]");                                  // clear the running high payload word
    emitter.instruction("cbz x0, __rt_mmm_done");                               // a null container behaves like an empty array
    emitter.instruction("ldr x9, [x0]");                                        // load the array's logical element count from its header
    emitter.instruction("str x9, [sp, #8]");                                    // preserve the element count across the helper calls
    emitter.instruction("cbz x9, __rt_mmm_done");                               // an empty array yields the sentinel tag

    // -- seed the reduction with the first element --
    emitter.instruction(&format!("ldr x0, [x0, #{}]", ARRAY_DATA_OFFSET));      // load the borrowed Mixed cell of element zero
    emitter.instruction("bl __rt_mixed_unbox");                                 // peel the cell into a concrete tag/lo/hi triple
    emitter.instruction("str x0, [sp, #32]");                                   // seed the running runtime tag
    emitter.instruction("str x1, [sp, #40]");                                   // seed the running low payload word
    emitter.instruction("str x2, [sp, #48]");                                   // seed the running high payload word
    emitter.instruction("mov x9, #1");                                          // the reduction resumes at the second element
    emitter.instruction("str x9, [sp, #16]");                                   // save the element cursor

    // -- fold every remaining element into the running result --
    emitter.label("__rt_mmm_loop");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the element cursor
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the element count
    emitter.instruction("cmp x9, x10");                                         // has every element been folded in?
    emitter.instruction("b.ge __rt_mmm_done");                                  // finish once the payload slots are exhausted
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the source indexed-array pointer
    emitter.instruction(&format!("add x10, x10, #{}", ARRAY_DATA_OFFSET));      // advance from the header to the payload slots
    emitter.instruction("ldr x0, [x10, x9, lsl #3]");                           // load the borrowed Mixed cell the cursor points at
    emitter.instruction("bl __rt_mixed_unbox");                                 // peel the candidate into a concrete tag/lo/hi triple
    emitter.instruction("str x0, [sp, #56]");                                   // save the candidate runtime tag
    emitter.instruction("str x1, [sp, #64]");                                   // save the candidate low payload word
    emitter.instruction("str x2, [sp, #72]");                                   // save the candidate high payload word
    emitter.instruction("ldr x3, [sp, #32]");                                   // pass the running runtime tag as the right operand
    emitter.instruction("ldr x4, [sp, #40]");                                   // pass the running low payload word
    emitter.instruction("ldr x5, [sp, #48]");                                   // pass the running high payload word
    emitter.instruction("bl __rt_php_compare");                                 // apply PHP 8's ordering table to candidate versus result
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the min/max direction flag
    emitter.instruction("cbz x9, __rt_mmm_want_min");                           // min() keeps the smaller element
    emitter.instruction("cmp x0, #0");                                          // did the candidate compare greater than the result?
    emitter.instruction("b.gt __rt_mmm_take");                                  // max() only replaces on a strict win, so ties keep the earlier element
    emitter.instruction("b __rt_mmm_next");                                     // otherwise keep the running result
    emitter.label("__rt_mmm_want_min");
    emitter.instruction("cmp x0, #0");                                          // did the candidate compare smaller than the result?
    emitter.instruction("b.ge __rt_mmm_next");                                  // min() only replaces on a strict win
    emitter.label("__rt_mmm_take");
    emitter.instruction("ldr x9, [sp, #56]");                                   // reload the winning candidate tag
    emitter.instruction("str x9, [sp, #32]");                                   // publish it as the new running tag
    emitter.instruction("ldr x9, [sp, #64]");                                   // reload the winning candidate low payload word
    emitter.instruction("str x9, [sp, #40]");                                   // publish it as the new running low word
    emitter.instruction("ldr x9, [sp, #72]");                                   // reload the winning candidate high payload word
    emitter.instruction("str x9, [sp, #48]");                                   // publish it as the new running high word
    emitter.label("__rt_mmm_next");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the element cursor
    emitter.instruction("add x9, x9, #1");                                      // advance to the next payload slot
    emitter.instruction("str x9, [sp, #16]");                                   // save the advanced cursor
    emitter.instruction("b __rt_mmm_loop");                                     // continue the reduction

    emitter.label("__rt_mmm_done");
    emitter.instruction("ldr x0, [sp, #32]");                                   // return the winning runtime tag
    emitter.instruction("ldr x1, [sp, #40]");                                   // return the winning low payload word
    emitter.instruction("ldr x2, [sp, #48]");                                   // return the winning high payload word
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the reduction frame
    emitter.instruction("ret");                                                 // return the reduced element triple
}

/// Emits the Linux x86_64 System V implementation of the boxed-Mixed reduction.
fn emit_min_max_mixed_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: min_max_mixed ---");
    emitter.label_global("__rt_min_max_mixed");

    // Frame (80 bytes below rbp): [-8]=array [-16]=length [-24]=cursor [-32]=want_max
    //                            [-40..-56]=best tag/lo/hi [-64..-80]=candidate tag/lo/hi
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the reduction frame pointer
    emitter.instruction("sub rsp, 80");                                         // allocate the aligned reduction frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the source indexed-array pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rsi");                       // save the min/max direction flag
    emitter.instruction("mov QWORD PTR [rbp - 40], -1");                        // tag -1 reports "the container yielded no element"
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // clear the running low payload word
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // clear the running high payload word
    emitter.instruction("test rdi, rdi");                                       // is the container pointer null?
    emitter.instruction("jz __rt_mmm_done_x86");                                // a null container behaves like an empty array
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // load the array's logical element count from its header
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the element count across the helper calls
    emitter.instruction("test rax, rax");                                       // does the array hold any element?
    emitter.instruction("jz __rt_mmm_done_x86");                                // an empty array yields the sentinel tag

    // -- seed the reduction with the first element --
    emitter.instruction(&format!("mov rax, QWORD PTR [rdi + {}]", ARRAY_DATA_OFFSET)); // load the borrowed Mixed cell of element zero
    emitter.instruction("call __rt_mixed_unbox");                               // peel the cell into a concrete tag/lo/hi triple
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // seed the running runtime tag
    emitter.instruction("mov QWORD PTR [rbp - 48], rdi");                       // seed the running low payload word
    emitter.instruction("mov QWORD PTR [rbp - 56], rdx");                       // seed the running high payload word
    emitter.instruction("mov QWORD PTR [rbp - 24], 1");                         // the reduction resumes at the second element

    // -- fold every remaining element into the running result --
    emitter.label("__rt_mmm_loop_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the element cursor
    emitter.instruction("cmp r10, QWORD PTR [rbp - 16]");                       // has every element been folded in?
    emitter.instruction("jge __rt_mmm_done_x86");                               // finish once the payload slots are exhausted
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the source indexed-array pointer
    emitter.instruction(&format!("mov rax, QWORD PTR [rax + r10 * 8 + {}]", ARRAY_DATA_OFFSET)); // load the borrowed Mixed cell the cursor points at
    emitter.instruction("call __rt_mixed_unbox");                               // peel the candidate into a concrete tag/lo/hi triple
    emitter.instruction("mov QWORD PTR [rbp - 64], rax");                       // save the candidate runtime tag
    emitter.instruction("mov QWORD PTR [rbp - 72], rdi");                       // save the candidate low payload word
    emitter.instruction("mov QWORD PTR [rbp - 80], rdx");                       // save the candidate high payload word
    emitter.instruction("mov rdi, rax");                                        // pass the candidate runtime tag as the left operand
    emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                       // pass the candidate low payload word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 80]");                       // pass the candidate high payload word
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // pass the running runtime tag as the right operand
    emitter.instruction("mov r8, QWORD PTR [rbp - 48]");                        // pass the running low payload word
    emitter.instruction("mov r9, QWORD PTR [rbp - 56]");                        // pass the running high payload word
    emitter.instruction("call __rt_php_compare");                               // apply PHP 8's ordering table to candidate versus result
    emitter.instruction("cmp QWORD PTR [rbp - 32], 0");                         // reload the min/max direction flag
    emitter.instruction("je __rt_mmm_want_min_x86");                            // min() keeps the smaller element
    emitter.instruction("cmp rax, 0");                                          // did the candidate compare greater than the result?
    emitter.instruction("jg __rt_mmm_take_x86");                                // max() only replaces on a strict win, so ties keep the earlier element
    emitter.instruction("jmp __rt_mmm_next_x86");                               // otherwise keep the running result
    emitter.label("__rt_mmm_want_min_x86");
    emitter.instruction("cmp rax, 0");                                          // did the candidate compare smaller than the result?
    emitter.instruction("jge __rt_mmm_next_x86");                               // min() only replaces on a strict win
    emitter.label("__rt_mmm_take_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // reload the winning candidate tag
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // publish it as the new running tag
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // reload the winning candidate low payload word
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // publish it as the new running low word
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // reload the winning candidate high payload word
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // publish it as the new running high word
    emitter.label("__rt_mmm_next_x86");
    emitter.instruction("add QWORD PTR [rbp - 24], 1");                         // advance the cursor to the next payload slot
    emitter.instruction("jmp __rt_mmm_loop_x86");                               // continue the reduction

    emitter.label("__rt_mmm_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // return the winning runtime tag
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // return the winning low payload word
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // return the winning high payload word
    emitter.instruction("mov rsp, rbp");                                        // release the reduction frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the reduced element triple
}

/// Emits `__rt_min_max_str`: reduces an indexed array of PHP byte strings.
///
/// Indexed string arrays use 16-byte payload slots (`[ptr:8][len:8]`), so the
/// element is already an unboxed string triple with runtime tag 1. Input and
/// output registers match `__rt_min_max_mixed`; the returned pointer is borrowed
/// from the array's own payload slot.
pub fn emit_min_max_str(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_min_max_str_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: min_max_str ---");
    emitter.label_global("__rt_min_max_str");

    // Frame (96 bytes): [0]=array [8]=length [16]=cursor [24]=want_max
    //                   [32..48]=best tag/ptr/len [56..72]=candidate tag/ptr/len [80]=x29/x30
    emitter.instruction("sub sp, sp, #96");                                     // allocate the reduction frame
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish the reduction frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the source indexed-array pointer
    emitter.instruction("str x1, [sp, #24]");                                   // save the min/max direction flag
    emitter.instruction("mov x9, #-1");                                         // tag -1 reports "the container yielded no element"
    emitter.instruction("str x9, [sp, #32]");                                   // seed the running result with the empty sentinel
    emitter.instruction("str xzr, [sp, #40]");                                  // clear the running string pointer
    emitter.instruction("str xzr, [sp, #48]");                                  // clear the running string length
    emitter.instruction("cbz x0, __rt_mms_done");                               // a null container behaves like an empty array
    emitter.instruction("ldr x9, [x0]");                                        // load the array's logical element count from its header
    emitter.instruction("str x9, [sp, #8]");                                    // preserve the element count across the helper calls
    emitter.instruction("cbz x9, __rt_mms_done");                               // an empty array yields the sentinel tag

    // -- seed the reduction with the first string slot --
    emitter.instruction(&format!("add x10, x0, #{}", ARRAY_DATA_OFFSET));       // advance from the header to the 16-byte string slots
    emitter.instruction("ldr x11, [x10]");                                      // load the first element's borrowed string pointer
    emitter.instruction("ldr x12, [x10, #8]");                                  // load the first element's string length
    emitter.instruction("mov x9, #1");                                          // runtime tag 1 = string
    emitter.instruction("str x9, [sp, #32]");                                   // seed the running runtime tag
    emitter.instruction("str x11, [sp, #40]");                                  // seed the running string pointer
    emitter.instruction("str x12, [sp, #48]");                                  // seed the running string length
    emitter.instruction("str x9, [sp, #16]");                                   // the reduction resumes at the second element

    // -- fold every remaining string into the running result --
    emitter.label("__rt_mms_loop");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the element cursor
    emitter.instruction("ldr x10, [sp, #8]");                                   // reload the element count
    emitter.instruction("cmp x9, x10");                                         // has every element been folded in?
    emitter.instruction("b.ge __rt_mms_done");                                  // finish once the payload slots are exhausted
    emitter.instruction("ldr x10, [sp, #0]");                                   // reload the source indexed-array pointer
    emitter.instruction(&format!("add x10, x10, #{}", ARRAY_DATA_OFFSET));      // advance from the header to the string slots
    emitter.instruction("add x10, x10, x9, lsl #4");                            // address the 16-byte slot the cursor points at
    emitter.instruction("mov x0, #1");                                          // the candidate is a string
    emitter.instruction("ldr x1, [x10]");                                       // load the candidate's borrowed string pointer
    emitter.instruction("ldr x2, [x10, #8]");                                   // load the candidate's string length
    emitter.instruction("str x0, [sp, #56]");                                   // save the candidate runtime tag
    emitter.instruction("str x1, [sp, #64]");                                   // save the candidate string pointer
    emitter.instruction("str x2, [sp, #72]");                                   // save the candidate string length
    emitter.instruction("ldr x3, [sp, #32]");                                   // pass the running runtime tag as the right operand
    emitter.instruction("ldr x4, [sp, #40]");                                   // pass the running string pointer
    emitter.instruction("ldr x5, [sp, #48]");                                   // pass the running string length
    emitter.instruction("bl __rt_php_compare");                                 // apply PHP 8's ordering table to candidate versus result
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the min/max direction flag
    emitter.instruction("cbz x9, __rt_mms_want_min");                           // min() keeps the smaller element
    emitter.instruction("cmp x0, #0");                                          // did the candidate compare greater than the result?
    emitter.instruction("b.gt __rt_mms_take");                                  // max() only replaces on a strict win, so ties keep the earlier element
    emitter.instruction("b __rt_mms_next");                                     // otherwise keep the running result
    emitter.label("__rt_mms_want_min");
    emitter.instruction("cmp x0, #0");                                          // did the candidate compare smaller than the result?
    emitter.instruction("b.ge __rt_mms_next");                                  // min() only replaces on a strict win
    emitter.label("__rt_mms_take");
    emitter.instruction("ldr x9, [sp, #64]");                                   // reload the winning candidate string pointer
    emitter.instruction("str x9, [sp, #40]");                                   // publish it as the new running string pointer
    emitter.instruction("ldr x9, [sp, #72]");                                   // reload the winning candidate string length
    emitter.instruction("str x9, [sp, #48]");                                   // publish it as the new running string length
    emitter.label("__rt_mms_next");
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the element cursor
    emitter.instruction("add x9, x9, #1");                                      // advance to the next string slot
    emitter.instruction("str x9, [sp, #16]");                                   // save the advanced cursor
    emitter.instruction("b __rt_mms_loop");                                     // continue the reduction

    emitter.label("__rt_mms_done");
    emitter.instruction("ldr x0, [sp, #32]");                                   // return the winning runtime tag
    emitter.instruction("ldr x1, [sp, #40]");                                   // return the winning string pointer
    emitter.instruction("ldr x2, [sp, #48]");                                   // return the winning string length
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the reduction frame
    emitter.instruction("ret");                                                 // return the reduced element triple
}

/// Emits the Linux x86_64 System V implementation of the indexed-string reduction.
fn emit_min_max_str_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: min_max_str ---");
    emitter.label_global("__rt_min_max_str");

    // Frame (80 bytes below rbp): [-8]=array [-16]=length [-24]=cursor [-32]=want_max
    //                            [-40..-56]=best tag/ptr/len [-64..-80]=candidate tag/ptr/len
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the reduction frame pointer
    emitter.instruction("sub rsp, 80");                                         // allocate the aligned reduction frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the source indexed-array pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rsi");                       // save the min/max direction flag
    emitter.instruction("mov QWORD PTR [rbp - 40], -1");                        // tag -1 reports "the container yielded no element"
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // clear the running string pointer
    emitter.instruction("mov QWORD PTR [rbp - 56], 0");                         // clear the running string length
    emitter.instruction("test rdi, rdi");                                       // is the container pointer null?
    emitter.instruction("jz __rt_mms_done_x86");                                // a null container behaves like an empty array
    emitter.instruction("mov rax, QWORD PTR [rdi]");                            // load the array's logical element count from its header
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the element count across the helper calls
    emitter.instruction("test rax, rax");                                       // does the array hold any element?
    emitter.instruction("jz __rt_mms_done_x86");                                // an empty array yields the sentinel tag

    // -- seed the reduction with the first string slot --
    emitter.instruction(&format!("lea r10, [rdi + {}]", ARRAY_DATA_OFFSET));    // advance from the header to the 16-byte string slots
    emitter.instruction("mov QWORD PTR [rbp - 40], 1");                         // runtime tag 1 = string
    emitter.instruction("mov rax, QWORD PTR [r10]");                            // load the first element's borrowed string pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // seed the running string pointer
    emitter.instruction("mov rax, QWORD PTR [r10 + 8]");                        // load the first element's string length
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // seed the running string length
    emitter.instruction("mov QWORD PTR [rbp - 24], 1");                         // the reduction resumes at the second element

    // -- fold every remaining string into the running result --
    emitter.label("__rt_mms_loop_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the element cursor
    emitter.instruction("cmp r10, QWORD PTR [rbp - 16]");                       // has every element been folded in?
    emitter.instruction("jge __rt_mms_done_x86");                               // finish once the payload slots are exhausted
    emitter.instruction("shl r10, 4");                                          // convert the cursor into a 16-byte slot offset
    emitter.instruction("add r10, QWORD PTR [rbp - 8]");                        // address the payload slot inside the source array
    emitter.instruction(&format!("add r10, {}", ARRAY_DATA_OFFSET));            // skip the indexed-array header
    emitter.instruction("mov rdi, 1");                                          // the candidate is a string
    emitter.instruction("mov rsi, QWORD PTR [r10]");                            // load the candidate's borrowed string pointer
    emitter.instruction("mov rdx, QWORD PTR [r10 + 8]");                        // load the candidate's string length
    emitter.instruction("mov QWORD PTR [rbp - 72], rsi");                       // save the candidate string pointer
    emitter.instruction("mov QWORD PTR [rbp - 80], rdx");                       // save the candidate string length
    emitter.instruction("mov rcx, QWORD PTR [rbp - 40]");                       // pass the running runtime tag as the right operand
    emitter.instruction("mov r8, QWORD PTR [rbp - 48]");                        // pass the running string pointer
    emitter.instruction("mov r9, QWORD PTR [rbp - 56]");                        // pass the running string length
    emitter.instruction("call __rt_php_compare");                               // apply PHP 8's ordering table to candidate versus result
    emitter.instruction("cmp QWORD PTR [rbp - 32], 0");                         // reload the min/max direction flag
    emitter.instruction("je __rt_mms_want_min_x86");                            // min() keeps the smaller element
    emitter.instruction("cmp rax, 0");                                          // did the candidate compare greater than the result?
    emitter.instruction("jg __rt_mms_take_x86");                                // max() only replaces on a strict win, so ties keep the earlier element
    emitter.instruction("jmp __rt_mms_next_x86");                               // otherwise keep the running result
    emitter.label("__rt_mms_want_min_x86");
    emitter.instruction("cmp rax, 0");                                          // did the candidate compare smaller than the result?
    emitter.instruction("jge __rt_mms_next_x86");                               // min() only replaces on a strict win
    emitter.label("__rt_mms_take_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // reload the winning candidate string pointer
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // publish it as the new running string pointer
    emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                       // reload the winning candidate string length
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // publish it as the new running string length
    emitter.label("__rt_mms_next_x86");
    emitter.instruction("add QWORD PTR [rbp - 24], 1");                         // advance the cursor to the next string slot
    emitter.instruction("jmp __rt_mms_loop_x86");                               // continue the reduction

    emitter.label("__rt_mms_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // return the winning runtime tag
    emitter.instruction("mov rdi, QWORD PTR [rbp - 48]");                       // return the winning string pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 56]");                       // return the winning string length
    emitter.instruction("mov rsp, rbp");                                        // release the reduction frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the reduced element triple
}

/// Emits `__rt_min_max_hash`: reduces a hash-backed associative array's values.
///
/// Walks the table in insertion order through `__rt_hash_iter_next_value`, normalizing
/// boxed entries (runtime tag 7) with `__rt_mixed_unbox` so values of any type
/// reach `__rt_php_compare` as a concrete triple. Input and output registers
/// match `__rt_min_max_mixed`; string payloads stay borrowed from the table.
pub fn emit_min_max_hash(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_min_max_hash_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: min_max_hash ---");
    emitter.label_global("__rt_min_max_hash");

    // Frame (96 bytes): [0]=hash [8]=cursor [16]=want_max
    //                   [24..40]=best tag/lo/hi [48..64]=candidate tag/lo/hi [80]=x29/x30
    emitter.instruction("sub sp, sp, #96");                                     // allocate the reduction frame
    emitter.instruction("stp x29, x30, [sp, #80]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #80");                                    // establish the reduction frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the source associative-array pointer
    emitter.instruction("str x1, [sp, #16]");                                   // save the min/max direction flag
    emitter.instruction("str xzr, [sp, #8]");                                   // start the insertion-order walk at cursor zero
    emitter.instruction("mov x9, #-1");                                         // tag -1 reports "the container yielded no element"
    emitter.instruction("str x9, [sp, #24]");                                   // seed the running result with the empty sentinel
    emitter.instruction("str xzr, [sp, #32]");                                  // clear the running low payload word
    emitter.instruction("str xzr, [sp, #40]");                                  // clear the running high payload word

    // -- visit every value in insertion order --
    emitter.label("__rt_mmh_loop");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the source hash pointer for the iterator
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the insertion-order cursor
    emitter.instruction("bl __rt_hash_iter_next_value");                        // fetch the next entry with cursor x0, payload x3/x4, and tag x5
    emitter.instruction("cmn x0, #1");                                          // did the iterator return its terminal negative-one cursor?
    emitter.instruction("b.eq __rt_mmh_done");                                  // finish once every entry has been folded in
    emitter.instruction("str x0, [sp, #8]");                                    // preserve the next insertion-order cursor
    emitter.instruction("cmp x5, #7");                                          // does the entry hold a boxed Mixed cell?
    emitter.instruction("b.ne __rt_mmh_direct");                                // unboxed entries already carry a concrete triple
    emitter.instruction("mov x0, x3");                                          // pass the borrowed Mixed cell to the unboxing helper
    emitter.instruction("bl __rt_mixed_unbox");                                 // peel the cell into a concrete tag/lo/hi triple
    emitter.instruction("b __rt_mmh_candidate");                                // continue with the normalized candidate
    emitter.label("__rt_mmh_direct");
    emitter.instruction("mov x0, x5");                                          // the entry's runtime tag is already concrete
    emitter.instruction("mov x1, x3");                                          // the entry's low payload word
    emitter.instruction("mov x2, x4");                                          // the entry's high payload word
    emitter.label("__rt_mmh_candidate");
    emitter.instruction("str x0, [sp, #48]");                                   // save the candidate runtime tag
    emitter.instruction("str x1, [sp, #56]");                                   // save the candidate low payload word
    emitter.instruction("str x2, [sp, #64]");                                   // save the candidate high payload word
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload the running runtime tag
    emitter.instruction("cmn x9, #1");                                          // is this the first value the walk has seen?
    emitter.instruction("b.eq __rt_mmh_take");                                  // the first value seeds the reduction unconditionally
    emitter.instruction("ldr x3, [sp, #24]");                                   // pass the running runtime tag as the right operand
    emitter.instruction("ldr x4, [sp, #32]");                                   // pass the running low payload word
    emitter.instruction("ldr x5, [sp, #40]");                                   // pass the running high payload word
    emitter.instruction("bl __rt_php_compare");                                 // apply PHP 8's ordering table to candidate versus result
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the min/max direction flag
    emitter.instruction("cbz x9, __rt_mmh_want_min");                           // min() keeps the smaller value
    emitter.instruction("cmp x0, #0");                                          // did the candidate compare greater than the result?
    emitter.instruction("b.gt __rt_mmh_take");                                  // max() only replaces on a strict win, so ties keep the earlier value
    emitter.instruction("b __rt_mmh_loop");                                     // otherwise keep the running result
    emitter.label("__rt_mmh_want_min");
    emitter.instruction("cmp x0, #0");                                          // did the candidate compare smaller than the result?
    emitter.instruction("b.ge __rt_mmh_loop");                                  // min() only replaces on a strict win
    emitter.label("__rt_mmh_take");
    emitter.instruction("ldr x9, [sp, #48]");                                   // reload the winning candidate tag
    emitter.instruction("str x9, [sp, #24]");                                   // publish it as the new running tag
    emitter.instruction("ldr x9, [sp, #56]");                                   // reload the winning candidate low payload word
    emitter.instruction("str x9, [sp, #32]");                                   // publish it as the new running low word
    emitter.instruction("ldr x9, [sp, #64]");                                   // reload the winning candidate high payload word
    emitter.instruction("str x9, [sp, #40]");                                   // publish it as the new running high word
    emitter.instruction("b __rt_mmh_loop");                                     // continue with the next insertion-order entry

    emitter.label("__rt_mmh_done");
    emitter.instruction("ldr x0, [sp, #24]");                                   // return the winning runtime tag
    emitter.instruction("ldr x1, [sp, #32]");                                   // return the winning low payload word
    emitter.instruction("ldr x2, [sp, #40]");                                   // return the winning high payload word
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the reduction frame
    emitter.instruction("ret");                                                 // return the reduced value triple
}

/// Emits the Linux x86_64 System V implementation of the associative-array reduction.
fn emit_min_max_hash_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: min_max_hash ---");
    emitter.label_global("__rt_min_max_hash");

    // Frame (80 bytes below rbp): [-8]=hash [-16]=cursor [-24]=want_max
    //                            [-32..-48]=best tag/lo/hi [-56..-72]=candidate tag/lo/hi
    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the reduction frame pointer
    emitter.instruction("sub rsp, 80");                                         // allocate the aligned reduction frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the source associative-array pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // save the min/max direction flag
    emitter.instruction("mov QWORD PTR [rbp - 16], 0");                         // start the insertion-order walk at cursor zero
    emitter.instruction("mov QWORD PTR [rbp - 32], -1");                        // tag -1 reports "the container yielded no element"
    emitter.instruction("mov QWORD PTR [rbp - 40], 0");                         // clear the running low payload word
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // clear the running high payload word

    // -- visit every value in insertion order --
    emitter.label("__rt_mmh_loop_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the source hash pointer for the iterator
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the insertion-order cursor
    emitter.instruction("call __rt_hash_iter_next_value");                      // fetch the next entry with cursor rax, payload rcx/r8, and tag r9
    emitter.instruction("cmp rax, -1");                                         // did the iterator return its terminal cursor?
    emitter.instruction("je __rt_mmh_done_x86");                                // finish once every entry has been folded in
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the next insertion-order cursor
    emitter.instruction("cmp r9, 7");                                           // does the entry hold a boxed Mixed cell?
    emitter.instruction("jne __rt_mmh_direct_x86");                             // unboxed entries already carry a concrete triple
    emitter.instruction("mov rax, rcx");                                        // pass the borrowed Mixed cell to the unboxing helper
    emitter.instruction("call __rt_mixed_unbox");                               // peel the cell into a concrete tag/lo/hi triple
    emitter.instruction("jmp __rt_mmh_candidate_x86");                          // continue with the normalized candidate
    emitter.label("__rt_mmh_direct_x86");
    emitter.instruction("mov rax, r9");                                         // the entry's runtime tag is already concrete
    emitter.instruction("mov rdi, rcx");                                        // the entry's low payload word
    emitter.instruction("mov rdx, r8");                                         // the entry's high payload word
    emitter.label("__rt_mmh_candidate_x86");
    emitter.instruction("mov QWORD PTR [rbp - 56], rax");                       // save the candidate runtime tag
    emitter.instruction("mov QWORD PTR [rbp - 64], rdi");                       // save the candidate low payload word
    emitter.instruction("mov QWORD PTR [rbp - 72], rdx");                       // save the candidate high payload word
    emitter.instruction("cmp QWORD PTR [rbp - 32], -1");                        // is this the first value the walk has seen?
    emitter.instruction("je __rt_mmh_take_x86");                                // the first value seeds the reduction unconditionally
    emitter.instruction("mov rdi, rax");                                        // pass the candidate runtime tag as the left operand
    emitter.instruction("mov rsi, QWORD PTR [rbp - 64]");                       // pass the candidate low payload word
    emitter.instruction("mov rdx, QWORD PTR [rbp - 72]");                       // pass the candidate high payload word
    emitter.instruction("mov rcx, QWORD PTR [rbp - 32]");                       // pass the running runtime tag as the right operand
    emitter.instruction("mov r8, QWORD PTR [rbp - 40]");                        // pass the running low payload word
    emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                        // pass the running high payload word
    emitter.instruction("call __rt_php_compare");                               // apply PHP 8's ordering table to candidate versus result
    emitter.instruction("cmp QWORD PTR [rbp - 24], 0");                         // reload the min/max direction flag
    emitter.instruction("je __rt_mmh_want_min_x86");                            // min() keeps the smaller value
    emitter.instruction("cmp rax, 0");                                          // did the candidate compare greater than the result?
    emitter.instruction("jg __rt_mmh_take_x86");                                // max() only replaces on a strict win, so ties keep the earlier value
    emitter.instruction("jmp __rt_mmh_loop_x86");                               // otherwise keep the running result
    emitter.label("__rt_mmh_want_min_x86");
    emitter.instruction("cmp rax, 0");                                          // did the candidate compare smaller than the result?
    emitter.instruction("jge __rt_mmh_loop_x86");                               // min() only replaces on a strict win
    emitter.label("__rt_mmh_take_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 56]");                       // reload the winning candidate tag
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // publish it as the new running tag
    emitter.instruction("mov rax, QWORD PTR [rbp - 64]");                       // reload the winning candidate low payload word
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // publish it as the new running low word
    emitter.instruction("mov rax, QWORD PTR [rbp - 72]");                       // reload the winning candidate high payload word
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // publish it as the new running high word
    emitter.instruction("jmp __rt_mmh_loop_x86");                               // continue with the next insertion-order entry

    emitter.label("__rt_mmh_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // return the winning runtime tag
    emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                       // return the winning low payload word
    emitter.instruction("mov rsi, QWORD PTR [rbp - 48]");                       // return the winning high payload word
    emitter.instruction("mov rsp, rbp");                                        // release the reduction frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the reduced value triple
}
