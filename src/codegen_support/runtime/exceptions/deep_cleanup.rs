//! Purpose:
//! Keeps native container destruction resumable when a child or PHP destructor throws.
//!
//! Called from:
//! - Indexed-array, hash, Mixed-cell, and object deep-free emitters.
//!
//! Key details:
//! - Each cleanup frame owns a pending flag and its incoming GC suppression state.
//! - Protected child calls suspend the prior exception so nested PHP catches stay independent.
//! - A completed container propagates the latest exception only after releasing its storage.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Offsets of a sixteen-byte cleanup record in the enclosing native frame.
#[derive(Clone, Copy)]
pub(crate) struct Scope { pub arm: usize, pub x86: usize }

impl Scope {
    /// Initializes pending state and suppresses collection until this container is fully released.
    pub(crate) fn begin(self, emitter: &mut Emitter) {
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("str xzr, [sp, #{}]", self.arm));  // initialize this frame's pending-exception flag
                abi::emit_load_symbol_to_reg(emitter, "x10", "_gc_release_suppressed", 0);
                emitter.instruction(&format!("str x10, [sp, #{}]", self.arm + 8)); // retain the enclosing collector suppression state
                emitter.instruction("mov x10, #1");                             // prevent collection of partially destroyed containers
                abi::emit_store_reg_to_symbol(emitter, "x10", "_gc_release_suppressed", 0);
            },
            Arch::X86_64 => {
                emitter.instruction(&format!("mov QWORD PTR [rbp - {}], 0", self.x86)); // start this frame with no newly escaping exception
                abi::emit_load_symbol_to_reg(emitter, "r10", "_gc_release_suppressed", 0);
                emitter.instruction(&format!("mov QWORD PTR [rbp - {}], r10", self.x86 - 8)); // retain nested cleanup's incoming suppression state
                emitter.instruction("mov r10, 1");                              // suspend collector runs while child ownership is incomplete
                abi::emit_store_reg_to_symbol(emitter, "r10", "_gc_release_suppressed", 0);
            },
        }
    }

    /// Runs one potentially throwing release with a native or C-ABI unary value argument.
    pub(crate) fn call(self, emitter: &mut Emitter, target: &str, c_input: bool) {
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("mov x1, x0");                              // retain the native child value before materializing its release callback
                abi::emit_symbol_address(emitter, "x0", target);
                emitter.instruction(&format!("add x2, sp, #{}", self.arm));     // pass this container's exception state by address
            },
            Arch::X86_64 => {
                emitter.instruction(if c_input { "mov rsi, rdi" } else { "mov rsi, rax" }); // adapt the child's C or native unary value convention
                abi::emit_symbol_address(emitter, "rdi", target);
                emitter.instruction(&format!("lea rdx, [rbp - {}]", self.x86)); // retain pending state in the enclosing deep-free frame
            },
        }
        abi::emit_call_label(emitter, "__rt_cleanup_call");
    }

    /// Restores collector state and leaves the pending flag in the native result register.
    pub(crate) fn finish(self, emitter: &mut Emitter) {
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("ldr x10, [sp, #{}]", self.arm + 8)); // recover the suppression state of the enclosing release
                abi::emit_store_reg_to_symbol(emitter, "x10", "_gc_release_suppressed", 0);
                emitter.instruction(&format!("ldr x0, [sp, #{}]", self.arm));   // retain whether a child exception must propagate after frame teardown
            },
            Arch::X86_64 => {
                emitter.instruction(&format!("mov r10, QWORD PTR [rbp - {}]", self.x86 - 8)); // restore the exact outer collector state
                abi::emit_store_reg_to_symbol(emitter, "r10", "_gc_release_suppressed", 0);
                emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {}]", self.x86)); // preserve the pending flag across the native epilogue
            },
        }
    }
}

/// Emits the shared callback boundary and exception-state restoration used by all cleanup scopes.
pub(super) fn emit(emitter: &mut Emitter) {
    super::emit_protected(emitter, "__rt_cleanup_call_protected", invoke_body);
    if emitter.target.arch == Arch::AArch64 { aarch64(emitter); }
    else { x86_64(emitter); }
    super::cleanup_previous::emit(emitter);
    super::previous_storage::emit(emitter);
}

/// Calls the retained unary function with both supported native and C value conventions.
fn invoke_body(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x9, [sp, #224]");                          // recover the cleanup routine pointer after setjmp
            emitter.instruction("ldr x0, [sp, #232]");                          // provide the retained child value to its native release helper
            emitter.instruction("blr x9");                                      // contain every PHP exception raised by this child cleanup
        },
        Arch::X86_64 => {
            emitter.instruction("mov r11, QWORD PTR [rsp + 224]");              // recover the protected routine pointer
            emitter.instruction("mov rax, QWORD PTR [rsp + 232]");              // install the native unary value convention
            emitter.instruction("mov rdi, rax");                                // also provide the C convention used by object destructor calls
            emitter.instruction("call r11");                                    // catch an escaping destructor before its parent skips later children
        },
    }
}
/// Suspends an outer AArch64 exception while one child cleanup runs under its own handler.
fn aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_cleanup_call");
    emitter.instruction("sub sp, sp, #48");                                     // reserve state, saved registers, and native linkage
    emitter.instruction("stp x29, x30, [sp, #32]");                             // retain the caller across protected PHP callbacks
    emitter.instruction("stp x19, x20, [sp, #16]");                             // preserve the cleanup state and suspended exception registers
    emitter.instruction("add x29, sp, #32");                                    // establish a stable cleanup callback frame
    emitter.instruction("mov x19, x2");                                         // retain the enclosing container cleanup state
    abi::emit_load_symbol_to_reg(emitter, "x20", "_exc_value", 0);
    abi::emit_store_reg_to_symbol(emitter, "xzr", "_exc_value", 0);
    emitter.instruction("bl __rt_cleanup_call_protected");                      // run the release without exposing its caller's pending exception
    emitter.instruction("cbz x0, __rt_cleanup_call_restore");                   // restore the suspended exception when this release succeeds
    emitter.instruction("mov x9, #1");                                          // record a newly escaping exception in the enclosing cleanup
    emitter.instruction("str x9, [x19]");                                       // keep later successful releases from discarding this failure
    emitter.instruction("mov x0, x20");                                         // transfer the suspended exception into the new exception chain
    emitter.instruction("mov x1, x19");                                         // allow duplicate-owner release to update the same cleanup state
    emitter.instruction("bl __rt_cleanup_chain_previous");                      // preserve prior exceptions without introducing a chain cycle
    emitter.instruction("b __rt_cleanup_call_done");                            // keep the newest exception published until cleanup finishes
    emitter.label("__rt_cleanup_call_restore");
    abi::emit_store_reg_to_symbol(emitter, "x20", "_exc_value", 0);
    emitter.label("__rt_cleanup_call_done");
    emitter.instruction("ldp x19, x20, [sp, #16]");                             // restore the enclosing cleanup registers
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // recover the native caller linkage
    emitter.instruction("add sp, sp, #48");                                     // release the protected-call staging frame
    emitter.instruction("ret");                                                 // continue the enclosing child walk with exception state retained
}
/// Provides the same suspended-exception callback contract for SysV cleanup walkers.
fn x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_cleanup_call");
    emitter.instruction("push rbp");                                            // align the stack and retain the caller frame
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame
    emitter.instruction("push r12");                                            // preserve the cleanup-state register
    emitter.instruction("push r13");                                            // preserve the suspended-exception register
    emitter.instruction("mov r12, rdx");                                        // retain the enclosing container's pending flag
    abi::emit_load_symbol_to_reg(emitter, "r13", "_exc_value", 0);
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    emitter.instruction("call __rt_cleanup_call_protected");                    // contain the child exception without crossing native cleanup frames
    emitter.instruction("test eax, eax");                                       // inspect whether this child returned or escaped through its PHP handler
    emitter.instruction("jz __rt_cleanup_call_restore");                        // restore the suspended exception after successful cleanup
    emitter.instruction("mov QWORD PTR [r12], 1");                              // retain the child failure through every remaining cleanup step
    emitter.instruction("mov rdi, r13");                                        // transfer the previous exception owner to the new chain
    emitter.instruction("mov rsi, r12");                                        // share pending state with a possible duplicate-owner release
    emitter.instruction("call __rt_cleanup_chain_previous");                    // append prior exceptions without forming a cycle
    emitter.instruction("jmp __rt_cleanup_call_done");                          // preserve the newly published exception
    emitter.label("__rt_cleanup_call_restore");
    abi::emit_store_reg_to_symbol(emitter, "r13", "_exc_value", 0);
    emitter.label("__rt_cleanup_call_done");
    emitter.instruction("pop r13");                                             // restore the caller's suspended-exception register
    emitter.instruction("pop r12");                                             // restore the parent cleanup-state register
    emitter.instruction("pop rbp");                                             // restore native frame linkage
    emitter.instruction("ret");                                                 // resume the enclosing cleanup walk
}
