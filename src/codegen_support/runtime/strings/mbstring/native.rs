//! Purpose:
//! Propagates native mbstring failures after the shared invocation coordinator returns.
//!
//! Called from:
//! - AOT mbstring calls with concrete argument descriptors and caller strictness.
//!
//! Key details:
//! - Shared EIR ownership guards protect captured arguments before and during this call.
//! - Rust and protected host callbacks finish before the PHP unwinder runs.
//! - Successful calls retain ordinary EIR argument cleanup.

use super::*;

/// Emits native exception propagation through the shared runtime on every supported target.
pub(super) fn emit(emitter: &mut Emitter, mbregex: bool) {
    emit_entry(emitter, "__rt_mbstring_native", "__rt_mbstring_invoke");
    emit_entry(emitter, "__rt_mbstring_query_native", "__rt_mbstring_query_invoke");
    if mbregex { emit_entry(emitter, "__rt_mbstring_capture_native", "__rt_mbstring_capture_invoke"); }
}

/// Shares native unwind behavior between ordinary value calls and capture-reference calls.
fn emit_entry(emitter: &mut Emitter, name: &str, invoke: &str) {
    emitter.label_global(name);
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction("stp x29, x30, [sp, #-16]!");                       // retain caller linkage until the shared coordinator returns
        emitter.instruction("mov x29, sp");                                     // establish an aligned native wrapper frame
        emitter.instruction(&format!("bl {invoke}"));                           // finish PHP argument preparation and every Rust callback frame
        emitter.instruction(&format!("cbz x1, {name}_done"));                   // return successful results to normal EIR argument cleanup
        emitter.instruction("cmp x1, #2");                                      // identify a pending PHP throwable
        emitter.instruction(&format!("b.eq {name}_throw"));                     // unwind only after Rust returned and released its own argument copies
        emitter.instruction("mov x0, #1");                                      // preserve nonzero process failure for an invalid host protocol
        emitter.bl_c("exit");
        emitter.label(&format!("{name}_throw"));
        emitter.instruction("ldp x29, x30, [sp], #16");                         // restore the native caller before walking EIR capture guards
        emitter.instruction("b __rt_throw_current");                            // release abandoned captures through the common exception chain
        emitter.label(&format!("{name}_done"));
        emitter.instruction("ldp x29, x30, [sp], #16");                         // preserve the successful value/status/length/kind tuple
        emitter.instruction("ret");                                             // let ordinary EIR cleanup consume or transfer arguments
    } else {
        emitter.instruction("push rbp");                                        // preserve native linkage and align the coordinator call
        emitter.instruction("mov rbp, rsp");                                    // establish stable native wrapper linkage
        emitter.instruction(&format!("call {invoke}"));                         // complete every Rust and protected host callback before native unwinding
        emitter.instruction("test edx, edx");                                   // distinguish a successful result from pending or fatal failure
        emitter.instruction(&format!("jz {name}_done"));                        // leave successful captures to normal EIR cleanup
        emitter.instruction("cmp edx, 2");                                      // recognize a catchable pending throwable
        emitter.instruction(&format!("je {name}_throw"));                       // transfer failure to the common native unwind path
        emitter.instruction("mov edi, 1");                                      // retain nonzero fatal process status for a broken host protocol
        emitter.bl_c("exit");
        emitter.label(&format!("{name}_throw"));
        emitter.instruction("pop rbp");                                         // restore caller linkage before its capture records are unwound
        emitter.instruction("jmp __rt_throw_current");                          // release abandoned argument guards and propagate the published throwable
        emitter.label(&format!("{name}_done"));
        emitter.instruction("pop rbp");                                         // keep the successful value/status/length/kind registers intact
        emitter.instruction("ret");                                             // return for ordinary EIR argument cleanup
    }
}
