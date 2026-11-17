//! Purpose:
//! Suspends an exception owner while a container releases one child.
//!
//! Called from:
//! - The compiler's protected container cleanup dispatcher.
//!
//! Key details:
//! - The protected callback and Throwable chaining use the host exception ABI.
//! - The enclosing cleanup scope retains its pending flag across later releases.

use crate::{Arch, Emitter};

/// Emits the native release boundary after the host's protected callback entry.
pub fn emit(emitter: &mut impl Emitter) {
    match emitter.arch() {
        Arch::AArch64 => aarch64(emitter),
        Arch::X86_64 => x86_64(emitter),
    }
}

/// Suspends an outer AArch64 exception while one child cleanup runs under its own handler.
fn aarch64(emitter: &mut impl Emitter) {
    emitter.label_global("__rt_cleanup_call");
    emitter.instruction("sub sp, sp, #48");                                     // reserve state, saved registers, and native linkage
    emitter.instruction("stp x29, x30, [sp, #32]");                             // retain the caller across protected PHP callbacks
    emitter.instruction("stp x19, x20, [sp, #16]");                             // preserve the cleanup state and suspended exception registers
    emitter.instruction("add x29, sp, #32");                                    // establish a stable cleanup callback frame
    emitter.instruction("mov x19, x2");                                         // retain the enclosing container cleanup state
    emitter.emit_load_symbol_to_reg("x20", "_exc_value", 0);
    emitter.emit_store_reg_to_symbol("xzr", "_exc_value", 0);
    emitter.instruction("bl __rt_cleanup_call_protected");                      // run the release without exposing its caller's pending exception
    emitter.instruction("cbz x0, __rt_cleanup_call_restore");                   // restore the suspended exception when this release succeeds
    emitter.instruction("mov x9, #1");                                          // record a newly escaping exception in the enclosing cleanup
    emitter.instruction("str x9, [x19]");                                       // keep later successful releases from discarding this failure
    emitter.instruction("mov x0, x20");                                         // transfer the suspended exception into the new exception chain
    emitter.instruction("mov x1, x19");                                         // allow duplicate-owner release to update the same cleanup state
    emitter.instruction("bl __rt_cleanup_chain_previous");                      // preserve prior exceptions without introducing a chain cycle
    emitter.instruction("b __rt_cleanup_call_done");                            // keep the newest exception published until cleanup finishes
    emitter.label("__rt_cleanup_call_restore");
    emitter.emit_store_reg_to_symbol("x20", "_exc_value", 0);
    emitter.label("__rt_cleanup_call_done");
    emitter.instruction("ldp x19, x20, [sp, #16]");                             // restore the enclosing cleanup registers
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // recover the native caller linkage
    emitter.instruction("add sp, sp, #48");                                     // release the protected-call staging frame
    emitter.instruction("ret");                                                 // continue the enclosing child walk with exception state retained
}
/// Provides the same suspended-exception callback contract for SysV cleanup walkers.
fn x86_64(emitter: &mut impl Emitter) {
    emitter.label_global("__rt_cleanup_call");
    emitter.instruction("push rbp");                                            // align the stack and retain the caller frame
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame
    emitter.instruction("push r12");                                            // preserve the cleanup-state register
    emitter.instruction("push r13");                                            // preserve the suspended-exception register
    emitter.instruction("mov r12, rdx");                                        // retain the enclosing container's pending flag
    emitter.emit_load_symbol_to_reg("r13", "_exc_value", 0);
    emitter.emit_store_zero_to_symbol("_exc_value", 0);
    emitter.instruction("call __rt_cleanup_call_protected");                    // contain the child exception without crossing native cleanup frames
    emitter.instruction("test eax, eax");                                       // inspect whether this child returned or escaped through its PHP handler
    emitter.instruction("jz __rt_cleanup_call_restore");                        // restore the suspended exception after successful cleanup
    emitter.instruction("mov QWORD PTR [r12], 1");                              // retain the child failure through every remaining cleanup step
    emitter.instruction("mov rdi, r13");                                        // transfer the previous exception owner to the new chain
    emitter.instruction("mov rsi, r12");                                        // share pending state with a possible duplicate-owner release
    emitter.instruction("call __rt_cleanup_chain_previous");                    // append prior exceptions without forming a cycle
    emitter.instruction("jmp __rt_cleanup_call_done");                          // preserve the newly published exception
    emitter.label("__rt_cleanup_call_restore");
    emitter.emit_store_reg_to_symbol("r13", "_exc_value", 0);
    emitter.label("__rt_cleanup_call_done");
    emitter.instruction("pop r13");                                             // restore the caller's suspended-exception register
    emitter.instruction("pop r12");                                             // restore the parent cleanup-state register
    emitter.instruction("pop rbp");                                             // restore native frame linkage
    emitter.instruction("ret");                                                 // resume the enclosing cleanup walk
}
