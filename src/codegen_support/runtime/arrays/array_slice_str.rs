//! Purpose:
//! Emits `__rt_array_slice_str`, the `array_slice()` runtime helper for STRING arrays, whose
//! 16-byte `(ptr, len)` slots the 8-byte scalar and refcounted helpers cannot copy.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - The PHP `$offset`/`$length` window arithmetic is the shared [`emit_slice_bounds`]
//!   sequence, so this helper cannot drift from the scalar and refcounted slice helpers.
//! - Selected elements are re-persisted through `__rt_array_push_str`, so the result owns its
//!   bytes — the ownership pattern every string listing already uses.
//! - Every live value is reloaded from the frame around each call: the append helper is free
//!   to clobber caller-saved registers, and an index kept in one across a call is the exact
//!   x86-only defect this repo has already shipped twice.

use crate::codegen_support::runtime::arrays::slice_bounds::emit_slice_bounds;
use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits the `__rt_array_slice_str` runtime helper for the active target.
///
/// # ABI
/// - **ARM64** — in: `x0` = source string-array pointer, `x1` = raw `$offset`, `x2` = raw
///   `$length`, `x3` = 1 when a `$length` was passed. Out: `x0` = freshly allocated result.
/// - **x86_64** — in: `rdi`, `rsi`, `rdx`, `rcx` with the same meaning. Out: `rax`.
pub fn emit_array_slice_str(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_slice_str_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_slice_str ---");
    emitter.label_global("__rt_array_slice_str");
    emit_slice_bounds(emitter, "__rt_array_slice_str");                         // x1 = window start, x2 = window length; x0 untouched

    // Frame: [0]=source [8]=dest [16]=i [24]=end, linkage at [32].
    emitter.instruction("sub sp, sp, #48");
    emitter.instruction("stp x29, x30, [sp, #32]");
    emitter.instruction("add x29, sp, #32");
    emitter.instruction("str x0, [sp, #0]");                                    // the source array
    emitter.instruction("str x1, [sp, #16]");                                   // i = the normalized window start
    emitter.instruction("add x9, x1, x2");
    emitter.instruction("str x9, [sp, #24]");                                   // end = start + window length
    emitter.instruction("mov x0, x2");                                          // capacity = window length
    emitter.instruction("mov x1, #16");                                         // 16-byte (ptr, len) slots
    emitter.instruction("bl __rt_array_new");
    emitter.instruction("str x0, [sp, #8]");                                    // the destination array

    emitter.label("__rt_asls_copy");
    emitter.instruction("ldr x9, [sp, #16]");                                   // i
    emitter.instruction("ldr x10, [sp, #24]");                                  // end
    emitter.instruction("cmp x9, x10");
    emitter.instruction("b.hs __rt_asls_done");                                 // the window is materialized
    emitter.instruction("ldr x10, [sp, #0]");
    emitter.instruction("add x10, x10, #24");
    emitter.instruction("add x10, x10, x9, lsl #4");                            // the source slot's address
    emitter.instruction("ldr x1, [x10]");                                       // the string pointer
    emitter.instruction("ldr x2, [x10, #8]");                                   // and its length
    emitter.instruction("ldr x0, [sp, #8]");
    emitter.instruction("bl __rt_array_push_str");                              // persist into the destination
    emitter.instruction("str x0, [sp, #8]");                                    // the append may have grown it
    emitter.instruction("ldr x9, [sp, #16]");
    emitter.instruction("add x9, x9, #1");                                      // i += 1
    emitter.instruction("str x9, [sp, #16]");
    emitter.instruction("b __rt_asls_copy");

    emitter.label("__rt_asls_done");
    emitter.instruction("ldr x0, [sp, #8]");                                    // the sliced array
    emitter.instruction("ldp x29, x30, [sp, #32]");
    emitter.instruction("add sp, sp, #48");
    emitter.instruction("ret");
}

/// Emits the x86_64 form of [`emit_array_slice_str`].
fn emit_array_slice_str_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_slice_str ---");
    emitter.label_global("__rt_array_slice_str");
    emit_slice_bounds(emitter, "__rt_array_slice_str");                         // rsi = window start, rdx = window length; rdi untouched

    // Frame: [rbp-8]=source [rbp-16]=dest [rbp-24]=i [rbp-32]=end.
    emitter.instruction("push rbp");
    emitter.instruction("mov rbp, rsp");
    emitter.instruction("sub rsp, 32");
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // the source array
    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // i = the normalized window start
    emitter.instruction("lea r9, [rsi + rdx]");
    emitter.instruction("mov QWORD PTR [rbp - 32], r9");                        // end = start + window length
    emitter.instruction("mov rdi, rdx");                                        // capacity = window length
    emitter.instruction("mov rsi, 16");                                         // 16-byte (ptr, len) slots
    emitter.instruction("call __rt_array_new");
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // the destination array

    emitter.label("__rt_asls_copy_x");
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");                        // i
    emitter.instruction("cmp r9, QWORD PTR [rbp - 32]");                        // against end
    emitter.instruction("jae __rt_asls_done_x");                                // the window is materialized
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");
    emitter.instruction("shl r9, 4");
    emitter.instruction("lea r10, [r10 + r9 + 24]");                            // the source slot's address
    emitter.instruction("mov rsi, QWORD PTR [r10]");                            // the string pointer
    emitter.instruction("mov rdx, QWORD PTR [r10 + 8]");                        // and its length
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");
    emitter.instruction("call __rt_array_push_str");                            // persist into the destination
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // the append may have grown it
    emitter.instruction("mov r9, QWORD PTR [rbp - 24]");
    emitter.instruction("add r9, 1");                                           // i += 1
    emitter.instruction("mov QWORD PTR [rbp - 24], r9");
    emitter.instruction("jmp __rt_asls_copy_x");

    emitter.label("__rt_asls_done_x");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // the sliced array
    emitter.instruction("add rsp, 32");
    emitter.instruction("pop rbp");
    emitter.instruction("ret");
}
