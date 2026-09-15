//! Purpose:
//! Contains destructor throws until the collector has finished recounting and reclaiming its graph.
//!
//! Called from:
//! - The cycle collector's destructor loop and its completed-pass epilogue.
//!
//! Key details:
//! - Each destructor has its own native handler, so unwinding preserves the collector activation.
//! - The pending raw Throwable owns its chain and remains a root throughout subsequent GC passes.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use crate::codegen_support::try_handlers::{
    TRY_HANDLER_DIAG_DEPTH_OFFSET, TRY_HANDLER_JMP_BUF_OFFSET, TRY_HANDLER_SLOT_SIZE,
};

/// Emits protected destructor calls and post-cleanup exception propagation for every target.
pub(super) fn emit_gc_exception_helpers(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Emits an ARM64 destructor boundary preserving the caller's active exception and diagnostics.
fn emit_aarch64(emitter: &mut Emitter) {
    let frame = TRY_HANDLER_SLOT_SIZE + 48;
    let receiver = TRY_HANDLER_SLOT_SIZE;
    let previous = receiver + 8;
    let link = frame - 16;
    emitter.label_global("__rt_gc_protected_destructor");
    // -- bound each destructor's unwind without discarding the collector's activation --
    emitter.instruction(&format!("sub sp, sp, #{frame}"));                      // reserve the complete handler and parked caller state
    emitter.instruction(&format!("stp x29, x30, [sp, #{link}]"));               // preserve the collector's frame linkage
    emitter.instruction(&format!("add x29, sp, #{link}"));                      // establish the native exception boundary frame
    emitter.instruction(&format!("str x0, [sp, #{receiver}]"));                 // borrow the receiver already owned by the collector snapshot
    abi::emit_load_symbol_to_reg(emitter, "x10", "_exc_value", 0);
    emitter.instruction(&format!("str x10, [sp, #{previous}]"));                // park the surrounding exception owner without releasing it
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    abi::emit_load_symbol_to_reg(emitter, "x10", "_exc_handler_top", 0);
    emitter.instruction("str x10, [sp]");                                       // link this boundary to the enclosing catch
    abi::emit_load_symbol_to_reg(emitter, "x10", "_exc_call_frame_top", 0);
    emitter.instruction("str x10, [sp, #8]");                                   // keep every activation that existed before the destructor call
    abi::emit_load_symbol_to_reg(emitter, "x10", "_rt_diag_suppression", 0);
    emitter.instruction(&format!("str x10, [sp, #{TRY_HANDLER_DIAG_DEPTH_OFFSET}]")); // preserve the caller's diagnostic suppression depth
    emitter.instruction("mov x10, sp");                                         // address this invocation's handler record
    abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0);
    emitter.instruction(&format!("add x0, sp, #{TRY_HANDLER_JMP_BUF_OFFSET}")); // address the handler's opaque jump buffer
    emitter.bl_c("setjmp");                                                     // catch escaping PHP throws inside the native collector loop
    emitter.instruction("cbnz x0, __rt_gc_destructor_caught");                  // resume here rather than bypassing pins and collector flags
    emitter.instruction(&format!("ldr x0, [sp, #{receiver}]"));                 // recover the snapshot-owned receiver for normal destructor dispatch
    emitter.instruction("bl __rt_call_object_destructor");                      // execute user code with the local exception boundary installed
    emitter.instruction("b __rt_gc_destructor_finish");                         // normal completion shares state restoration with the catch path

    // -- root and chain throws while leaving later destructors runnable --
    emitter.label("__rt_gc_destructor_caught");
    abi::emit_load_symbol_to_reg(emitter, "x0", "_exc_value", 0);
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    abi::emit_load_symbol_to_reg(emitter, "x1", "_gc_pending_throw", 0);
    abi::emit_store_reg_to_symbol(emitter, "x0", "_gc_pending_throw", 0);
    emitter.instruction("bl __rt_exception_chain");                             // the pending root adopts the older exception without replacing its links
    emitter.label("__rt_gc_destructor_finish");
    emitter.instruction("ldr x10, [sp]");                                       // recover the collector caller's handler
    abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0);
    emitter.instruction(&format!("ldr x10, [sp, #{TRY_HANDLER_DIAG_DEPTH_OFFSET}]")); // recover diagnostic state after normal or exceptional completion
    abi::emit_store_reg_to_symbol(emitter, "x10", "_rt_diag_suppression", 0);
    emitter.instruction(&format!("ldr x10, [sp, #{previous}]"));                // restore the surrounding exception owner parked at entry
    abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_value", 0);
    emitter.instruction(&format!("ldp x29, x30, [sp, #{link}]"));               // restore the collector frame
    emitter.instruction(&format!("add sp, sp, #{frame}"));                      // release the local handler and borrowed receiver slot
    emitter.instruction("ret");                                                 // let the collector mark completion, recount, and continue sweeping

    emitter.label_global("__rt_gc_rethrow_pending");
    // -- publish a pending exception only after the collector has balanced all of its state --
    abi::emit_load_symbol_to_reg(emitter, "x10", "_gc_pending_throw", 0);
    emitter.instruction("cbz x10, __rt_gc_rethrow_return");                     // preserve the collection count on successful completion
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // preserve the caller while joining its enclosing exception
    emitter.instruction("mov x29, sp");                                         // establish an aligned frame for the chain helper
    emitter.instruction("mov x0, x10");                                         // transfer the collector's pending raw owner to native throw state
    abi::emit_store_zero_to_symbol(emitter, "_gc_pending_throw", 0);
    abi::emit_load_symbol_to_reg(emitter, "x1", "_exc_value", 0);
    abi::emit_store_reg_to_symbol(emitter, "x0", "_exc_value", 0);
    emitter.instruction("bl __rt_exception_chain");                             // preserve an exception that surrounded the explicit collection call
    emitter.instruction("ldp x29, x30, [sp], #16");                             // remove this helper's frame before propagation
    emitter.instruction("b __rt_throw_current");                                // the caller observes the throw after pins, timing, and GC flags are balanced
    emitter.label("__rt_gc_rethrow_return");
    emitter.instruction("ret");                                                 // return the untouched collected-node count when there was no throw
}

/// Emits the System V destructor boundary with the same raw-owner and cleanup guarantees.
fn emit_x86_64(emitter: &mut Emitter) {
    let frame = TRY_HANDLER_SLOT_SIZE + 16;
    emitter.label_global("__rt_gc_protected_destructor");
    // -- bound each destructor's unwind without discarding the collector's activation --
    emitter.instruction("push rbp");                                            // preserve the collector frame and align outgoing calls
    emitter.instruction("mov rbp, rsp");                                        // establish slots stable across setjmp and longjmp
    emitter.instruction(&format!("sub rsp, {frame}"));                          // reserve the complete handler and two parked caller values
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // borrow the receiver kept alive by the collector snapshot
    abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_value", 0);
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // park the surrounding raw exception owner
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_handler_top", 0);
    emitter.instruction(&format!("mov QWORD PTR [rbp - {frame}], r10"));        // chain this local boundary to the caller's handler
    abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_call_frame_top", 0);
    emitter.instruction(&format!("mov QWORD PTR [rbp - {}], r10", frame - 8));  // preserve every activation outside this destructor invocation
    abi::emit_load_symbol_to_reg(emitter, "r10", "_rt_diag_suppression", 0);
    emitter.instruction(&format!("mov QWORD PTR [rbp - {}], r10", frame - TRY_HANDLER_DIAG_DEPTH_OFFSET)); // save diagnostic suppression across throws
    emitter.instruction(&format!("lea r10, [rbp - {frame}]"));                  // address the complete handler record
    abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0);
    emitter.instruction(&format!("lea rdi, [rbp - {}]", frame - TRY_HANDLER_JMP_BUF_OFFSET)); // pass the handler's opaque jump buffer to libc
    emitter.bl_c("setjmp");                                                     // contain PHP throws below the surviving collector activation
    emitter.instruction("test eax, eax");                                       // distinguish a throw from the initial boundary setup
    emitter.instruction("jnz __rt_gc_destructor_caught");                       // catch without skipping collector cleanup
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // recover the snapshot-owned receiver
    emitter.instruction("call __rt_call_object_destructor");                    // execute user code under this local handler
    emitter.instruction("jmp __rt_gc_destructor_finish");                       // share state restoration between normal and exceptional returns

    // -- root and chain throws while leaving later destructors runnable --
    emitter.label("__rt_gc_destructor_caught");
    abi::emit_load_symbol_to_reg(emitter, "rax", "_exc_value", 0);
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    abi::emit_load_symbol_to_reg(emitter, "rdi", "_gc_pending_throw", 0);
    abi::emit_store_reg_to_symbol(emitter, "rax", "_gc_pending_throw", 0);
    emitter.instruction("call __rt_exception_chain");                           // preserve the older exception under the newly published pending root
    emitter.label("__rt_gc_destructor_finish");
    emitter.instruction(&format!("mov r10, QWORD PTR [rbp - {frame}]"));        // recover the caller's native handler record
    abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0);
    emitter.instruction(&format!("mov r10, QWORD PTR [rbp - {}]", frame - TRY_HANDLER_DIAG_DEPTH_OFFSET)); // recover suppression after both call outcomes
    abi::emit_store_reg_to_symbol(emitter, "r10", "_rt_diag_suppression", 0);
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // recover the enclosing raw exception owner
    abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_value", 0);
    emitter.instruction("leave");                                               // discard the bounded invocation's frame
    emitter.instruction("ret");                                                 // let the collector mark this destructor done and finish its graph work

    emitter.label_global("__rt_gc_rethrow_pending");
    // -- publish a pending exception only after the collector has balanced all of its state --
    abi::emit_load_symbol_to_reg(emitter, "r10", "_gc_pending_throw", 0);
    emitter.instruction("test r10, r10");                                       // inspect the pending root without clobbering the count in rax
    emitter.instruction("jz __rt_gc_rethrow_return");                           // successful collection returns its original count
    emitter.instruction("push rbp");                                            // preserve the caller and align the chain-helper invocation
    emitter.instruction("mov rbp, rsp");                                        // establish the propagation helper frame
    emitter.instruction("mov rax, r10");                                        // transfer the pending raw owner into native exception state
    abi::emit_store_zero_to_symbol(emitter, "_gc_pending_throw", 0);
    abi::emit_load_symbol_to_reg(emitter, "rdi", "_exc_value", 0);
    abi::emit_store_reg_to_symbol(emitter, "rax", "_exc_value", 0);
    emitter.instruction("call __rt_exception_chain");                           // preserve an exception surrounding the explicit collection call
    emitter.instruction("pop rbp");                                             // restore the caller before the exceptional transfer
    emitter.instruction("jmp __rt_throw_current");                              // throw only after snapshots and collector flags are fully balanced
    emitter.label("__rt_gc_rethrow_return");
    emitter.instruction("ret");                                                 // return the unchanged count when no destructor threw
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// All targets install the boundary before user code and publish the pending owner before chaining.
    #[test]
    fn collector_destructor_boundaries_preserve_handler_and_pending_owners_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_gc_exception_helpers(&mut emitter);
            let asm = emitter.output();
            assert!(asm.find(&target.extern_symbol("setjmp")).unwrap() < asm.find("__rt_call_object_destructor").unwrap(), "{name}");
            let catch = asm.split("__rt_gc_destructor_caught:\n").nth(1).unwrap();
            assert!(catch.find("_gc_pending_throw").unwrap() < catch.find("__rt_exception_chain").unwrap(), "{name}");
            assert!(asm.contains("_rt_diag_suppression") && asm.contains("_exc_call_frame_top"), "{name}");
            assert!(asm.contains("__rt_gc_rethrow_pending:") && asm.contains("__rt_throw_current"), "{name}");
        }
    }
}
