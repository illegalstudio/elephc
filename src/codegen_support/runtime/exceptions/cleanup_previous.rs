//! Purpose:
//! Transfers suspended Throwable owners into the exception raised by native cleanup.
//!
//! Called from:
//! - `__rt_cleanup_call` after a protected child release returns an escaping exception.
//!
//! Key details:
//! - The active exception remains owned by `_exc_value`; the incoming previous owner is consumed.
//! - Shared previous-slot helpers distinguish compact raw pointers and ordinary nullable boxes.
//! - Existing chains are inspected before insertion to avoid duplicate links and cycles.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits the target-specific previous-chain owner transfer helper.
pub(super) fn emit(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::AArch64 { aarch64(emitter); }
    else { x86_64(emitter); }
}

/// Scans both chains with preserved AArch64 ownership before appending or consuming a duplicate.
fn aarch64(emitter: &mut Emitter) {
    emitter.label_global("__rt_cleanup_chain_previous");
    emitter.instruction("cbz x0, __rt_cleanup_chain_previous_empty");           // no suspended owner requires no chain traversal
    emitter.instruction("sub sp, sp, #48");                                     // reserve preserved chain cursors and caller linkage
    emitter.instruction("stp x29, x30, [sp, #32]");                             // retain the native return across shared slot access
    emitter.instruction("stp x19, x20, [sp]");                                  // preserve cleanup state and prior exception registers
    emitter.instruction("stp x21, x22, [sp, #16]");                             // preserve both exception-chain cursors
    emitter.instruction("add x29, sp, #32");                                    // establish stable chain traversal linkage
    emitter.instruction("mov x19, x1");                                         // retain the enclosing pending-exception state
    emitter.instruction("mov x20, x0");                                         // retain the consumed prior exception owner
    abi::emit_load_symbol_to_reg(emitter, "x21", "_exc_value", 0);
    emitter.label("__rt_cleanup_chain_previous_outer");
    emitter.instruction("mov x22, x20");                                        // restart the old ancestor scan for each new-chain node
    emitter.label("__rt_cleanup_chain_previous_inner");
    emitter.instruction("cmp x22, x21");                                        // recognize an existing link or a link that would create a cycle
    emitter.instruction("b.eq __rt_cleanup_chain_previous_duplicate");          // consume redundant ownership without adding a repeated ancestor
    emitter.instruction("mov x0, x22");                                         // provide the old ancestor through the shared native slot accessor
    emitter.instruction("bl __rt_throwable_previous");                          // borrow its previous pointer using the actual storage representation
    emitter.instruction("mov x22, x0");                                         // retain the next old ancestor across loop iterations
    emitter.instruction("cbnz x22, __rt_cleanup_chain_previous_inner");         // inspect every old ancestor before extending the new chain
    emitter.instruction("mov x0, x21");                                         // provide the current new-chain node for previous lookup
    emitter.instruction("bl __rt_throwable_previous");                          // read compact and ordinary nullable previous slots identically
    emitter.instruction("cbz x0, __rt_cleanup_chain_previous_append");          // append only after reaching the checked vacant tail
    emitter.instruction("mov x21, x0");                                         // advance through the existing new exception chain
    emitter.instruction("b __rt_cleanup_chain_previous_outer");                 // test the next new-chain node against all old ancestors
    emitter.label("__rt_cleanup_chain_previous_append");
    emitter.instruction("mov x0, x21");                                         // provide the checked destination Throwable
    emitter.instruction("mov x1, x20");                                         // transfer the suspended previous owner into that destination
    emitter.instruction("bl __rt_throwable_append_previous");                   // fill the correct raw or boxed slot without an extra child retain
    emitter.instruction("b __rt_cleanup_chain_previous_done");                  // restore the caller after successful ownership transfer
    emitter.label("__rt_cleanup_chain_previous_duplicate");
    emitter.instruction("mov x2, x19");                                         // preserve the enclosing cleanup state for duplicate-owner destruction
    emitter.instruction("mov x1, x20");                                         // consume the duplicate prior exception through protected release
    abi::emit_symbol_address(emitter, "x0", "__rt_decref_any");
    emitter.instruction("bl __rt_cleanup_call");                                // protect custom exception destructors while dropping redundant ownership
    emitter.label("__rt_cleanup_chain_previous_done");
    emitter.instruction("ldp x19, x20, [sp]");                                  // restore the caller cleanup and prior-owner registers
    emitter.instruction("ldp x21, x22, [sp, #16]");                             // restore the caller chain cursors
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // recover native linkage
    emitter.instruction("add sp, sp, #48");                                     // release chain traversal storage
    emitter.label("__rt_cleanup_chain_previous_empty");
    emitter.instruction("ret");                                                 // retain the active exception in the global ownership slot
}

/// Applies the same chain-intersection algorithm with preserved SysV cursors.
fn x86_64(emitter: &mut Emitter) {
    emitter.label_global("__rt_cleanup_chain_previous");
    emitter.instruction("test rdi, rdi");                                       // check whether a prior exception owner was suspended
    emitter.instruction("jz __rt_cleanup_chain_previous_empty");                // skip traversal when only the new exception exists
    emitter.instruction("push rbp");                                            // align the stack and preserve caller linkage
    emitter.instruction("mov rbp, rsp");                                        // establish a stable chain traversal frame
    emitter.instruction("push r12");                                            // preserve the enclosing cleanup-state register
    emitter.instruction("push r13");                                            // preserve the suspended exception register
    emitter.instruction("push r14");                                            // preserve the new-chain cursor register
    emitter.instruction("push r15");                                            // preserve the old-chain cursor and maintain SysV call alignment
    emitter.instruction("mov r12, rsi");                                        // retain the enclosing pending-exception state
    emitter.instruction("mov r13, rdi");                                        // retain the consumed suspended exception owner
    abi::emit_load_symbol_to_reg(emitter, "r14", "_exc_value", 0);
    emitter.label("__rt_cleanup_chain_previous_outer");
    emitter.instruction("mov r15, r13");                                        // restart the old ancestor scan for this new-chain node
    emitter.label("__rt_cleanup_chain_previous_inner");
    emitter.instruction("cmp r15, r14");                                        // detect existing ancestry or a prospective chain cycle
    emitter.instruction("je __rt_cleanup_chain_previous_duplicate");            // consume duplicate ownership without extending either chain
    emitter.instruction("mov rax, r15");                                        // pass the old ancestor using the native slot accessor convention
    emitter.instruction("call __rt_throwable_previous");                        // borrow the previous object from its actual raw or boxed storage
    emitter.instruction("mov r15, rax");                                        // retain the next old ancestor
    emitter.instruction("test r15, r15");                                       // check whether the old chain still has ancestors
    emitter.instruction("jnz __rt_cleanup_chain_previous_inner");               // finish the old-chain intersection scan
    emitter.instruction("mov rax, r14");                                        // pass the current new-chain node through the same accessor
    emitter.instruction("call __rt_throwable_previous");                        // borrow the next previous object or zero
    emitter.instruction("test rax, rax");                                       // recognize a checked vacant previous slot
    emitter.instruction("jz __rt_cleanup_chain_previous_append");               // transfer ownership only at the end of the new chain
    emitter.instruction("mov r14, rax");                                        // advance the new-chain cursor
    emitter.instruction("jmp __rt_cleanup_chain_previous_outer");               // check the next new-chain node against all prior ancestors
    emitter.label("__rt_cleanup_chain_previous_append");
    emitter.instruction("mov rdi, r14");                                        // provide the destination Throwable through the C argument convention
    emitter.instruction("mov rsi, r13");                                        // consume the suspended owner into the destination previous property
    emitter.instruction("call __rt_throwable_append_previous");                 // fill raw or boxed storage with balanced ownership
    emitter.instruction("jmp __rt_cleanup_chain_previous_done");                // restore caller state after appending
    emitter.label("__rt_cleanup_chain_previous_duplicate");
    emitter.instruction("mov rdx, r12");                                        // share pending state with a possible throwing exception destructor
    emitter.instruction("mov rsi, r13");                                        // provide the redundant old owner for protected destruction
    abi::emit_symbol_address(emitter, "rdi", "__rt_decref_any");
    emitter.instruction("call __rt_cleanup_call");                              // consume redundant ownership under the same protected cleanup boundary
    emitter.label("__rt_cleanup_chain_previous_done");
    emitter.instruction("pop r15");                                             // restore the caller old-chain cursor
    emitter.instruction("pop r14");                                             // restore the caller new-chain cursor
    emitter.instruction("pop r13");                                             // restore the caller suspended-owner register
    emitter.instruction("pop r12");                                             // restore the enclosing cleanup state
    emitter.instruction("pop rbp");                                             // recover native caller linkage
    emitter.label("__rt_cleanup_chain_previous_empty");
    emitter.instruction("ret");                                                 // keep the active exception owned by its global pending slot
}
