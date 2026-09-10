//! Purpose:
//! Invokes native or eval Stringable conversion behind a non-unwinding C boundary.
//!
//! Called from:
//! - Shared mbstring argument adapters after a Stringable preparation action.
//!
//! Key details:
//! - C inputs are optional eval context, borrowed boxed object, and MbHostStringV1 output.
//! - Success transfers native string ownership; a throw returns PendingThrowable with zero output.
//! - The boundary restores handler, activation-frame, and diagnostic-suppression state.

use super::*;
use crate::codegen_support::try_handlers::{TRY_HANDLER_DIAG_DEPTH_OFFSET, TRY_HANDLER_JMP_BUF_OFFSET, TRY_HANDLER_SLOT_SIZE};

/// Emits the protected host callback without retaining any Rust frame or request-state borrow.
pub(super) fn emit(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Emits AArch64 context/object/output staging and a complete PHP exception-handler record.
fn emit_aarch64(emitter: &mut Emitter) {
    let frame = TRY_HANDLER_SLOT_SIZE + 48;
    let locals = TRY_HANDLER_SLOT_SIZE;
    emitter.label_global("__rt_mbstring_stringable");
    emitter.instruction("cbz x2, __rt_mbstring_stringable_invalid");            // reject a missing output before accessing caller storage
    emitter.instruction("stp xzr, xzr, [x2]");                                  // clear the result byte pointer and length before any callback
    emitter.instruction("str xzr, [x2, #16]");                                  // failure transfers no native string owner
    emitter.instruction("cbz x1, __rt_mbstring_stringable_invalid");            // reject a missing borrowed boxed receiver
    emitter.instruction(&format!("sub sp, sp, #{frame}"));                      // reserve the handler record and aligned callback spills
    emitter.instruction(&format!("stp x29, x30, [sp, #{}]", frame - 16));       // preserve caller linkage across setjmp and PHP execution
    emitter.instruction(&format!("add x29, sp, #{}", frame - 16));              // establish the protected callback frame
    emitter.instruction(&format!("stp x0, x1, [sp, #{locals}]"));               // retain optional eval context and the borrowed object box
    emitter.instruction(&format!("str x2, [sp, #{}]", locals + 16));            // retain the caller-owned result slot
    for (offset, symbol) in [(0, "_exc_handler_top"), (8, "_exc_call_frame_top"), (TRY_HANDLER_DIAG_DEPTH_OFFSET, "_rt_diag_suppression")] {
        abi::emit_load_symbol_to_reg(emitter, "x10", symbol, 0);
        emitter.instruction(&format!("str x10, [sp, #{offset}]"));              // save the handler chain, surviving activation, or suppression depth
    }
    emitter.instruction("mov x10, sp");                                         // use the local record as this callback's exception boundary
    abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0);
    emitter.instruction(&format!("add x0, sp, #{TRY_HANDLER_JMP_BUF_OFFSET}")); // pass the opaque handler jump buffer to the platform C library
    emitter.bl_c("setjmp");
    emitter.instruction("cbnz x0, __rt_mbstring_stringable_throw");             // convert an escaping PHP throw into a returned runtime status
    emitter.instruction("mov x0, #7");                                          // select the formatter's borrowed boxed-value entry
    emitter.instruction(&format!("ldr x1, [sp, #{}]", locals + 8));             // reload the original boxed Stringable receiver
    emitter.instruction(&format!("ldr x2, [sp, #{locals}]"));                   // propagate the active eval context to dynamic methods
    emitter.instruction("bl __rt_sprintf_mixed_to_string");                     // reuse native and eval method dispatch with explicit string ownership
    emitter.instruction(&format!("ldr x10, [sp, #{}]", locals + 16));           // recover the caller's native string result slot
    emitter.instruction("stp x1, x2, [x10]");                                   // transfer the converted binary string pointer and byte length
    emitter.instruction("str x0, [x10, #16]");                                  // transfer the distinct native owner for eventual host cleanup
    emitter.instruction("mov w0, #0");                                          // report a successful conversion
    emitter.instruction("b __rt_mbstring_stringable_done");                     // pop the same boundary on success and failure
    emitter.label("__rt_mbstring_stringable_throw");
    emitter.instruction(&format!("mov w0, #{}", RuntimeBuiltinStatus::PendingThrowable as i32)); // retain the throwable already published by the native unwinder
    emitter.label("__rt_mbstring_stringable_done");
    for (offset, symbol) in [(0, "_exc_handler_top"), (8, "_exc_call_frame_top"), (TRY_HANDLER_DIAG_DEPTH_OFFSET, "_rt_diag_suppression")] {
        emitter.instruction(&format!("ldr x10, [sp, #{offset}]"));              // restore state skipped by an escaping callback or nested suppression
        abi::emit_store_reg_to_symbol(emitter, "x10", symbol, 0);
    }
    emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", frame - 16));       // restore the original caller linkage after boundary cleanup
    emitter.instruction(&format!("add sp, sp, #{frame}"));                      // release every callback spill and handler word
    emitter.instruction("ret");                                                 // return a C status without unwinding through the caller
    emitter.label("__rt_mbstring_stringable_invalid");
    emitter.instruction(&format!("mov w0, #{}", RuntimeBuiltinStatus::RuntimeFatal as i32)); // fail closed when the C input/output contract is invalid
    emitter.instruction("ret");                                                 // no handler or temporary ownership was established
}

/// Emits SysV context/object/output staging and restores the full host state after longjmp.
fn emit_x86_64(emitter: &mut Emitter) {
    let frame = TRY_HANDLER_SLOT_SIZE + 32;
    emitter.label_global("__rt_mbstring_stringable");
    emitter.instruction("test rdx, rdx");                                       // require writable native string result storage
    emitter.instruction("jz __rt_mbstring_stringable_invalid");                 // reject null output before accessing it
    for offset in [0, 8, 16] {
        emitter.instruction(&format!("mov QWORD PTR [rdx + {offset}], 0"));     // failure transfers no borrowed bytes or native owner
    }
    emitter.instruction("test rsi, rsi");                                       // require a borrowed boxed receiver
    emitter.instruction("jz __rt_mbstring_stringable_invalid");                 // reject null input without establishing a handler
    emitter.instruction("push rbp");                                            // preserve caller linkage and align subsequent C calls
    emitter.instruction("mov rbp, rsp");                                        // keep a stable base across setjmp and callback frames
    emitter.instruction(&format!("sub rsp, {frame}"));                          // reserve a full handler record and callback spills
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // retain the optional eval context
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // retain the borrowed boxed object
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // retain the caller-owned string result slot
    for (offset, symbol) in [(0, "_exc_handler_top"), (8, "_exc_call_frame_top"), (TRY_HANDLER_DIAG_DEPTH_OFFSET, "_rt_diag_suppression")] {
        abi::emit_load_symbol_to_reg(emitter, "r10", symbol, 0);
        emitter.instruction(&format!("mov QWORD PTR [rsp + {offset}], r10"));   // save the previous handler, activation frame, or suppression depth
    }
    emitter.instruction("mov r10, rsp");                                        // publish this callback's local exception record
    abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0);
    emitter.instruction(&format!("lea rdi, [rsp + {TRY_HANDLER_JMP_BUF_OFFSET}]")); // pass the opaque jump buffer through the SysV C ABI
    emitter.bl_c("setjmp");
    emitter.instruction("test eax, eax");                                       // distinguish initial execution from an escaping PHP throw
    emitter.instruction("jnz __rt_mbstring_stringable_throw");                  // return a pending throwable after restoring native state
    emitter.instruction("mov edi, 7");                                          // select borrowed boxed-value conversion
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the original Stringable receiver
    emitter.instruction("mov rdx, QWORD PTR [rbp - 8]");                        // propagate the caller's active eval context
    emitter.instruction("call __rt_sprintf_mixed_to_string");                   // reuse native and eval dispatch with explicit owner output
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // recover writable native string result storage
    emitter.instruction("mov QWORD PTR [r10], rax");                            // transfer the exact binary string pointer
    emitter.instruction("mov QWORD PTR [r10 + 8], rdx");                        // transfer its complete byte length
    emitter.instruction("mov QWORD PTR [r10 + 16], rcx");                       // transfer the owner released by the host's GC helper
    emitter.instruction("xor eax, eax");                                        // report successful conversion
    emitter.instruction("jmp __rt_mbstring_stringable_done");                   // share handler teardown with callback failure
    emitter.label("__rt_mbstring_stringable_throw");
    emitter.instruction(&format!("mov eax, {}", RuntimeBuiltinStatus::PendingThrowable as i32)); // preserve the native unwinder's pending throwable
    emitter.label("__rt_mbstring_stringable_done");
    for (offset, symbol) in [(0, "_exc_handler_top"), (8, "_exc_call_frame_top"), (TRY_HANDLER_DIAG_DEPTH_OFFSET, "_rt_diag_suppression")] {
        emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {offset}]"));   // restore state after normal return or longjmp
        abi::emit_store_reg_to_symbol(emitter, "r10", symbol, 0);
    }
    emitter.instruction("leave");                                               // discard the complete handler and restore the caller frame
    emitter.instruction("ret");                                                 // return a C status without unwinding through Rust
    emitter.label("__rt_mbstring_stringable_invalid");
    emitter.instruction(&format!("mov eax, {}", RuntimeBuiltinStatus::RuntimeFatal as i32)); // fail closed before acquiring callback ownership
    emitter.instruction("ret");                                                 // invalid metadata leaves native state untouched
}

#[cfg(test)]
mod tests;
