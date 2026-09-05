//! Purpose:
//! Emits `__rt_php_compare_slots` / `__rt_php_compare_slots_desc`: comparator
//! callbacks that order two boxed `Mixed` array slots by PHP's own rules.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Written to `__rt_usort`'s callback ABI — `(a, b)` in the first two argument
//!   registers, ordering in the result register — so `sort()` over `Mixed`
//!   elements is the existing slot permuter driven by the existing ordering
//!   table, with nothing new deciding what "less than" means.
//! - The ordering itself is `__rt_php_compare`, which is what `<` and `<=>`
//!   already use for runtime-tagged operands. Reimplementing PHP's cross-type
//!   comparison here would be a second answer to a question the runtime already
//!   answers, and the two would drift.
//! - The descending variant negates the result rather than swapping the operands.
//!   Swapping would reverse the order of EQUAL elements too, and PHP's `rsort` is
//!   not specified to do that.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits both comparator callbacks for the current target.
pub fn emit_php_compare_slots(emitter: &mut Emitter) {
    emit_one(emitter, "__rt_php_compare_slots", false);
    emit_one(emitter, "__rt_php_compare_slots_desc", true);
}

fn emit_one(emitter: &mut Emitter, label: &str, descending: bool) {
    emitter.blank();
    emitter.comment(&format!("--- runtime: {label} ---"));
    emitter.label_global(label);

    match emitter.target.arch {
        Arch::AArch64 => {
            // Frame: [sp,#0] b's cell, [sp,#8] a.tag, [sp,#16] a.lo, [sp,#24] a.hi.
            emitter.instruction("sub sp, sp, #48");
            emitter.instruction("stp x29, x30, [sp, #32]");                     // save the frame record across two nested calls
            emitter.instruction("add x29, sp, #32");
            emitter.instruction("str x1, [sp, #0]");                            // keep b's cell: unboxing a clobbers the argument registers
            abi::emit_call_label(emitter, "__rt_mixed_unbox");                  // a → x0=tag, x1=lo, x2=hi
            emitter.instruction("stp x0, x1, [sp, #8]");                        // stash a's tag and low word
            emitter.instruction("str x2, [sp, #24]");                           // stash a's high word
            emitter.instruction("ldr x0, [sp, #0]");
            abi::emit_call_label(emitter, "__rt_mixed_unbox");                  // b → x0=tag, x1=lo, x2=hi
            emitter.instruction("mov x3, x0");                                  // right tag
            emitter.instruction("mov x4, x1");                                  // right low word
            emitter.instruction("mov x5, x2");                                  // right high word
            emitter.instruction("ldp x0, x1, [sp, #8]");                        // left tag and low word
            emitter.instruction("ldr x2, [sp, #24]");                           // left high word
            abi::emit_call_label(emitter, "__rt_php_compare");                  // → x0 = -1 / 0 / 1
            if descending {
                emitter.instruction("neg x0, x0");                              // reverse the order without reversing equal elements
            }
            emitter.instruction("ldp x29, x30, [sp, #32]");
            emitter.instruction("add sp, sp, #48");
            emitter.instruction("ret");
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");
            emitter.instruction("mov rbp, rsp");
            emitter.instruction("sub rsp, 48");                                 // locals, and rsp stays 16-byte aligned for the nested calls
            emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                // keep b's cell before unboxing a clobbers it
            emitter.instruction("mov rax, rdi");                                // __rt_mixed_unbox reads its cell from rax
            abi::emit_call_label(emitter, "__rt_mixed_unbox");                  // a → rax=tag, rdi=lo, rdx=hi
            emitter.instruction("mov QWORD PTR [rbp - 16], rax");
            emitter.instruction("mov QWORD PTR [rbp - 24], rdi");
            emitter.instruction("mov QWORD PTR [rbp - 32], rdx");
            emitter.instruction("mov rax, QWORD PTR [rbp - 8]");
            abi::emit_call_label(emitter, "__rt_mixed_unbox");                  // b → rax=tag, rdi=lo, rdx=hi
            emitter.instruction("mov rcx, rax");                                // right tag
            emitter.instruction("mov r8, rdi");                                 // right low word
            emitter.instruction("mov r9, rdx");                                 // right high word
            emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");               // left tag
            emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");               // left low word
            emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");               // left high word
            abi::emit_call_label(emitter, "__rt_php_compare");                  // → rax = -1 / 0 / 1
            if descending {
                emitter.instruction("neg rax");                                 // reverse the order without reversing equal elements
            }
            emitter.instruction("add rsp, 48");
            emitter.instruction("pop rbp");
            emitter.instruction("ret");
        }
    }
}
