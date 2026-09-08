//! Purpose:
//! Contains native cycle-collector exceptions before control returns to Rust eval.
//!
//! Called from:
//! - The eval bridge runtime emitter.
//!
//! Key details:
//! - The C caller supplies a non-null output slot for an owned boxed Throwable.
//! - Native longjmp targets stay below Rust frames; enclosing exception state is restored.

use super::*;
use crate::codegen_support::try_handlers::{
    TRY_HANDLER_DIAG_DEPTH_OFFSET, TRY_HANDLER_JMP_BUF_OFFSET, TRY_HANDLER_SLOT_SIZE,
};

/// Emits `gc_collect_cycles(Throwable **out) -> count` with a native exception boundary.
pub(super) fn emit_gc_collection_boundary(emitter: &mut Emitter) {
    // No legacy alias: callers without the output argument must fail at link time.
    label_c_global(emitter, "__elephc_eval_gc_collect_cycles_v2");
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Emits the ARM64 boundary, transferring any escaping object owner into one Mixed box.
fn emit_aarch64(emitter: &mut Emitter) {
    let frame = TRY_HANDLER_SLOT_SIZE + 64;
    let output = TRY_HANDLER_SLOT_SIZE;
    let previous = output + 8;
    let count = output + 16;
    let throwable = output + 24;
    let link = frame - 16;
    // -- preserve the C output slot and establish a native exception boundary --
    emitter.instruction(&format!("sub sp, sp, #{frame}"));                      // reserve handler, output pointer, owners, and frame linkage
    emitter.instruction(&format!("stp x29, x30, [sp, #{link}]"));               // preserve the C caller's frame and return address
    emitter.instruction(&format!("add x29, sp, #{link}"));                      // establish the protected native frame
    emitter.instruction(&format!("str x0, [sp, #{output}]"));                   // preserve the Rust-owned Throwable output slot across setjmp
    emitter.instruction("str xzr, [x0]");                                       // successful collection returns no Throwable
    abi::emit_load_symbol_to_reg(emitter, "x10", "_exc_value", 0);
    emitter.instruction(&format!("str x10, [sp, #{previous}]"));                // park an enclosing native exception owner without releasing it
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    abi::emit_load_symbol_to_reg(emitter, "x10", "_exc_handler_top", 0);
    emitter.instruction("str x10, [sp]");                                       // link the internal boundary to the caller's handler
    abi::emit_load_symbol_to_reg(emitter, "x10", "_exc_call_frame_top", 0);
    emitter.instruction("str x10, [sp, #8]");                                   // preserve every activation outside this native call
    abi::emit_load_symbol_to_reg(emitter, "x10", "_rt_diag_suppression", 0);
    emitter.instruction(&format!("str x10, [sp, #{TRY_HANDLER_DIAG_DEPTH_OFFSET}]")); // save suppression depth for the exceptional return
    emitter.instruction("mov x10, sp");                                         // address the complete native handler record
    abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0);
    emitter.instruction(&format!("add x0, sp, #{TRY_HANDLER_JMP_BUF_OFFSET}")); // pass this boundary's opaque jump buffer to libc
    // -- contain both collection outcomes below the Rust caller's frames --
    emitter.bl_c("setjmp");                                                     // contain native throws before they can cross Rust frames
    emitter.instruction("cbnz x0, __elephc_eval_gc_caught");                    // distinguish longjmp from the initial setjmp return
    emitter.instruction("bl __rt_gc_collect_cycles_explicit");                  // collect under the native exception boundary
    emitter.instruction(&format!("str x0, [sp, #{count}]"));                    // preserve the collector's count while restoring handler state
    emitter.instruction(&format!("str xzr, [sp, #{throwable}]"));               // the normal return transfers no exception
    emitter.instruction("b __elephc_eval_gc_finish");                           // share state restoration with the catch path
    emitter.label("__elephc_eval_gc_caught");
    abi::emit_load_symbol_to_reg(emitter, "x10", "_exc_value", 0);
    emitter.instruction(&format!("str x10, [sp, #{throwable}]"));               // take ownership of the escaping raw Throwable
    emitter.instruction(&format!("str xzr, [sp, #{count}]"));                   // the caller ignores the count when an exception escaped
    emitter.label("__elephc_eval_gc_finish");
    // -- restore surrounding exception and diagnostic state before publishing the result --
    emitter.instruction("ldr x10, [sp]");                                       // recover the enclosing native exception handler
    abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0);
    emitter.instruction(&format!("ldr x10, [sp, #{TRY_HANDLER_DIAG_DEPTH_OFFSET}]")); // restore diagnostic state even after longjmp
    abi::emit_store_reg_to_symbol(emitter, "x10", "_rt_diag_suppression", 0);
    emitter.instruction(&format!("ldr x10, [sp, #{previous}]"));                // return the parked exception owner to its enclosing scope
    abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_value", 0);
    emitter.instruction(&format!("ldr x1, [sp, #{throwable}]"));                // inspect the raw Throwable transferred by the catch path
    emitter.instruction("cbz x1, __elephc_eval_gc_return");                     // normal collection needs no boxing or ownership adjustment
    // -- transfer the raw exception owner into the caller's owned Mixed output --
    emitter.instruction("mov x0, #6");                                          // runtime value tag six boxes an object payload
    emitter.instruction("mov x2, xzr");                                         // objects have no second payload word
    emitter.instruction("bl __rt_mixed_from_value");                            // create the output box and acquire its independent object owner
    emitter.instruction(&format!("ldr x10, [sp, #{output}]"));                  // recover the Rust-owned output address after allocation
    emitter.instruction("str x0, [x10]");                                       // transfer the owned box to the C caller
    emitter.instruction(&format!("ldr x0, [sp, #{throwable}]"));                // recover the raw owner taken from native exception state
    emitter.instruction("bl __rt_decref_any");                                  // balance that raw owner while the output box keeps the object alive
    emitter.label("__elephc_eval_gc_return");
    emitter.instruction(&format!("ldr x0, [sp, #{count}]"));                    // return the count independently of the Throwable output
    emitter.instruction(&format!("ldp x29, x30, [sp, #{link}]"));               // restore the Rust caller's native frame linkage
    emitter.instruction(&format!("add sp, sp, #{frame}"));                      // release the bounded invocation's stack storage
    emitter.instruction("ret");                                                 // Rust observes an ordinary C return on success and throw
}

/// Emits the System V boundary with the same output ownership and exception-state rules.
fn emit_x86_64(emitter: &mut Emitter) {
    let frame = TRY_HANDLER_SLOT_SIZE + 32;
    // -- preserve the C output slot and establish a native exception boundary --
    emitter.instruction("push rbp");                                            // align the stack and preserve the C caller's frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish stable addressing across setjmp and longjmp
    emitter.instruction(&format!("sub rsp, {frame}"));                          // reserve a complete handler and four owner/result slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // preserve the Rust-owned Throwable output slot
    emitter.instruction("mov QWORD PTR [rdi], 0");                              // successful collection returns no Throwable
    abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_value", 0);
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // park an enclosing native exception owner
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_handler_top", 0);
    emitter.instruction(&format!("mov QWORD PTR [rbp - {frame}], r10"));        // link the boundary to the previous native handler
    abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_call_frame_top", 0);
    emitter.instruction(&format!("mov QWORD PTR [rbp - {}], r10", frame - 8));  // preserve activations outside this native invocation
    abi::emit_load_symbol_to_reg(emitter, "r10", "_rt_diag_suppression", 0);
    emitter.instruction(&format!("mov QWORD PTR [rbp - {}], r10", frame - TRY_HANDLER_DIAG_DEPTH_OFFSET)); // snapshot diagnostic suppression
    emitter.instruction(&format!("lea r10, [rbp - {frame}]"));                  // address this invocation's handler record
    abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0);
    emitter.instruction(&format!("lea rdi, [rbp - {}]", frame - TRY_HANDLER_JMP_BUF_OFFSET)); // pass the boundary's opaque jump buffer
    // -- contain both collection outcomes below the Rust caller's frames --
    emitter.bl_c("setjmp");                                                     // keep native exception jumps below Rust stack frames
    emitter.instruction("test eax, eax");                                       // identify an exceptional resume through longjmp
    emitter.instruction("jnz __elephc_eval_gc_caught");                         // take the Throwable from native exception state on throw
    emitter.instruction("call __rt_gc_collect_cycles_explicit");                // execute cycle collection within the bounded native frame
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the successful count across state restoration
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // no exception owner leaves the normal path
    emitter.instruction("jmp __elephc_eval_gc_finish");                         // share boundary restoration between both outcomes
    emitter.label("__elephc_eval_gc_caught");
    abi::emit_load_symbol_to_reg(emitter, "r10", "_exc_value", 0);
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // take the escaping raw Throwable owner
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // the count is unused when an exception escapes
    emitter.label("__elephc_eval_gc_finish");
    // -- restore surrounding exception and diagnostic state before publishing the result --
    emitter.instruction(&format!("mov r10, QWORD PTR [rbp - {frame}]"));        // recover the caller's native exception handler
    abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0);
    emitter.instruction(&format!("mov r10, QWORD PTR [rbp - {}]", frame - TRY_HANDLER_DIAG_DEPTH_OFFSET)); // restore suppression after either control-flow outcome
    abi::emit_store_reg_to_symbol(emitter, "r10", "_rt_diag_suppression", 0);
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // recover the enclosing exception owner parked at entry
    abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_value", 0);
    emitter.instruction("mov rdi, QWORD PTR [rbp - 32]");                       // inspect the raw Throwable returned through the catch boundary
    emitter.instruction("test rdi, rdi");                                       // does the caller need an owned Throwable box?
    emitter.instruction("jz __elephc_eval_gc_return");                          // successful collection needs no box or additional owner
    // -- transfer the raw exception owner into the caller's owned Mixed output --
    emitter.instruction("mov eax, 6");                                          // runtime value tag six describes an object payload
    emitter.instruction("xor esi, esi");                                        // the object payload has no high word
    emitter.instruction("call __rt_mixed_from_value");                          // box the object and retain it independently of the raw exception
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // recover the Rust-owned output address after allocation
    emitter.instruction("mov QWORD PTR [r10], rax");                            // transfer the owned box to the C caller
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // recover the raw owner removed from native exception state
    emitter.instruction("call __rt_decref_any");                                // release that owner while the output box roots its object
    emitter.label("__elephc_eval_gc_return");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the independent collection count
    emitter.instruction("leave");                                               // release boundary storage and restore the caller frame
    emitter.instruction("ret");                                                 // return through the C ABI even when PHP threw
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every supported ABI catches collection throws and balances raw-to-boxed ownership.
    #[test]
    fn gc_boundary_contains_native_throws_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_gc_collection_boundary(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains(&format!("{}:", target.extern_symbol("__elephc_eval_gc_collect_cycles_v2"))), "{name}");
            assert!(!asm.contains(&format!("{}:", target.extern_symbol("__elephc_eval_gc_collect_cycles"))), "{name}");
            let setjmp = asm.find(&target.extern_symbol("setjmp")).unwrap();
            let collect = asm.find("__rt_gc_collect_cycles_explicit").unwrap();
            let catch = asm.find("__elephc_eval_gc_caught:").unwrap();
            let boxed = asm.find("__rt_mixed_from_value").unwrap();
            let released = asm.find("__rt_decref_any").unwrap();
            assert!(setjmp < collect && collect < catch && catch < boxed && boxed < released, "{name}: {asm}");
            assert!(asm.contains("_exc_call_frame_top") && asm.contains("_rt_diag_suppression"), "{name}");
            assert!(asm.matches("_exc_handler_top").count() >= 3, "{name}");
            assert!(asm.matches("_exc_value").count() >= 4, "{name}");
        }
    }
}
