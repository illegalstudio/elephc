//! Purpose:
//! Registers eval string literals in the shared native INI origin registry.
//!
//! Called from:
//! - The eval runtime emitter and Magician's literal-value constructor.
//!
//! Key details:
//! - Only the owned native payload is registered; temporary Rust source addresses are never retained.
//! - Interning affects logical identity while ordinary Mixed ownership still releases native storage.

use super::*;

/// Boxes literal bytes through the ordinary constructor, then marks the owned payload before publication.
pub(crate) fn emit(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_value_string_literal");
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction("stp x29, x30, [sp, #-16]!");                       // preserve C linkage across native allocation and origin registration
        emitter.bl_c("__elephc_eval_value_string");
        emitter.instruction("ldr x1, [x0, #8]");                                // select the owned string payload without retaining Rust input storage
        emitter.instruction("ldr x2, [x0, #16]");                               // recover the complete native string length from its Mixed cell
        abi::emit_call_label(emitter, "__rt_mbstring_ini_literal");
        emitter.instruction("ldp x29, x30, [sp], #16");                         // restore C linkage with the original boxed result still in x0
    } else {
        emitter.instruction("push rbp");                                        // preserve C linkage and align calls before constructing the literal
        emitter.instruction("mov rbp, rsp");                                    // retain a stable frame for the owned cell result
        emitter.instruction("sub rsp, 16");                                     // reserve one boxed-result slot and call-alignment padding
        emitter.bl_c("__elephc_eval_value_string");
        emitter.instruction("mov QWORD PTR [rsp], rax");                        // preserve the owning Mixed cell while passing its payload to the origin hook
        emitter.instruction("mov rdx, QWORD PTR [rax + 16]");                   // retain the native string's exact logical byte count
        emitter.instruction("mov rax, QWORD PTR [rax + 8]");                    // register only the owned native payload address
        abi::emit_call_label(emitter, "__rt_mbstring_ini_literal");
        emitter.instruction("mov rax, QWORD PTR [rsp]");                        // return the owned cell after publishing its interned origin
        emitter.instruction("leave");                                           // restore C linkage and release the boxed-result slot
    }
    emitter.instruction("ret");                                                 // publish a complete literal whose future copies preserve logical identity
}
