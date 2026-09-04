//! Purpose:
//! Emits `__rt_sort_mixed_asc` and `__rt_sort_mixed_desc`, the built-in comparators that let
//! `sort()` and `rsort()` order an indexed array whose elements are boxed `Mixed` cells.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::arrays`.
//! - `crate::codegen::lower_inst::builtins::arrays::sort_dispatch`, which hands one of them to
//!   `__rt_usort` in place of a user comparator.
//!
//! Key details:
//! - `sort()` used to refuse a `Mixed` element outright — `unsupported EIR backend feature: sort
//!   indexed-array element PHP type Mixed` — which is a COMPILE error on valid php. It is easy to
//!   reach without writing anything exotic: `while (($e = readdir($h)) !== false) { $seen[] = $e; }
//!   sort($seen);` types `$seen` as `array<Mixed>`, because `readdir()` answers `string|false`.
//! - Nothing new sorts here. `__rt_usort` already permutes 8-byte slots against a comparator, and
//!   `__rt_php_compare` already implements php 8's ordering table over an unboxed triple. These
//!   two helpers are the adapter between them, so `sort()` on a `Mixed` array orders exactly the
//!   way `<=>` does — which is what php does too.
//! - Descending order swaps the OPERANDS rather than negating the result: `__rt_php_compare`'s
//!   contract is the sign of the answer, and reversing the arguments cannot depend on its
//!   magnitude the way a negation would.

use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits both built-in `Mixed` comparators.
pub fn emit_sort_mixed(emitter: &mut Emitter) {
    emit_comparator(emitter, "__rt_sort_mixed_asc", false);
    emit_comparator(emitter, "__rt_sort_mixed_desc", true);
}

/// Emits one comparator: `(left_cell, right_cell)` in, php's ordering of them out.
fn emit_comparator(emitter: &mut Emitter, name: &str, descending: bool) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter, name, descending),
        Arch::X86_64 => emit_x86_64(emitter, name, descending),
    }
}

/// Emits the AArch64 comparator.
///
/// Input:  x0 = left boxed cell, x1 = right boxed cell
/// Output: x0 = negative, zero or positive, as php orders the two values
fn emit_aarch64(emitter: &mut Emitter, name: &str, descending: bool) {
    emitter.blank();
    emitter.comment(&format!("--- runtime: {name} ---"));
    emitter.label_global(name);

    // Frame: [0]=the second cell across the first unbox, [8..24]=the first unboxed triple.
    emitter.instruction("sub sp, sp, #64");                                     // reserve the comparator frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the comparator frame pointer
    emitter.instruction("str x1, [sp, #0]");                                    // the unbox call takes x0 and clobbers x1

    emitter.instruction("bl __rt_mixed_unbox");                                 // peel the first cell into tag/lo/hi
    emitter.instruction("stp x0, x1, [sp, #8]");                                // hold its tag and low payload word
    emitter.instruction("str x2, [sp, #24]");                                   // and its high payload word

    emitter.instruction("ldr x0, [sp, #0]");                                    // the second cell
    emitter.instruction("bl __rt_mixed_unbox");                                 // peel it into tag/lo/hi as well

    if descending {
        // The second value becomes the LEFT operand, which is what reverses the order.
        emitter.instruction("ldr x3, [sp, #8]");                                // the first value's tag, as the right operand
        emitter.instruction("ldr x4, [sp, #16]");
        emitter.instruction("ldr x5, [sp, #24]");
    } else {
        emitter.instruction("mov x3, x0");                                      // the second value's tag, as the right operand
        emitter.instruction("mov x4, x1");
        emitter.instruction("mov x5, x2");
        emitter.instruction("ldp x0, x1, [sp, #8]");                            // the first value, as the left operand
        emitter.instruction("ldr x2, [sp, #24]");
    }
    emitter.instruction("bl __rt_php_compare");                                 // php 8's ordering table decides

    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the comparator frame
    emitter.instruction("ret");                                                 // the ordering is already in x0
}

/// Emits the x86_64 comparator.
///
/// Input:  rdi = left boxed cell, rsi = right boxed cell
/// Output: rax = negative, zero or positive, as php orders the two values
fn emit_x86_64(emitter: &mut Emitter, name: &str, descending: bool) {
    emitter.blank();
    emitter.comment(&format!("--- runtime: {name} ---"));
    emitter.label_global(name);

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the comparator frame
    emitter.instruction("sub rsp, 48");                                         // reserve the saved cell and the first triple
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // the unbox call clobbers rdi and rdx

    emitter.instruction("mov rax, rdi");                                        // __rt_mixed_unbox reads the cell from rax
    emitter.instruction("call __rt_mixed_unbox");                               // peel the first cell into rax/rdi/rdx
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // hold its tag
    emitter.instruction("mov QWORD PTR [rbp - 24], rdi");                       // its low payload word
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // and its high payload word

    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // the second cell
    emitter.instruction("call __rt_mixed_unbox");                               // peel it into rax/rdi/rdx as well

    if descending {
        // The second value becomes the LEFT operand, which is what reverses the order.
        emitter.instruction("mov rsi, rdi");                                    // its low payload word, before rdi is reloaded
        emitter.instruction("mov rdi, rax");                                    // its tag
        emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                   // the first value's tag, as the right operand
        emitter.instruction("mov r8, QWORD PTR [rbp - 24]");
        emitter.instruction("mov r9, QWORD PTR [rbp - 32]");
    } else {
        emitter.instruction("mov rcx, rax");                                    // the second value's tag, as the right operand
        emitter.instruction("mov r8, rdi");                                     // its low payload word
        emitter.instruction("mov r9, rdx");                                     // its high payload word
        emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                   // the first value, as the left operand
        emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");
        emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");
    }
    emitter.instruction("call __rt_php_compare");                               // php 8's ordering table decides

    emitter.instruction("add rsp, 48");                                         // release the comparator frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // the ordering is already in rax
}
