//! Purpose:
//! Exposes native Throwable previous links as owned nullable eval values.
//!
//! Called from:
//! - Compact builtin Throwable method dispatch in the generated eval bridge.
//!
//! Key details:
//! - The runtime reader understands raw and boxed previous slots.
//! - Mixed boxing retains a borrowed previous object and represents absence as boxed null.

use super::*;

/// Reads getPrevious through the shared storage helper and returns an owned box on either architecture.
pub(super) fn emit(module: &Module, emitter: &mut Emitter, done_label: &str, fail_label: &str) {
    emitter.label(BUILTIN_THROWABLE_GET_PREVIOUS_LABEL);
    let null = "__elephc_eval_builtin_throwable_previous_null";
    let boxed = "__elephc_eval_builtin_throwable_previous_box";
    if emitter.target.arch == Arch::AArch64 {
        emit_aarch64_validate_builtin_throwable_method_arg_count(module, emitter, fail_label);
        emitter.instruction("ldr x0, [sp, #16]");                               // borrow the current native Throwable receiver
        emitter.instruction("bl __rt_throwable_previous");                      // read raw or boxed previous storage without acquiring another owner
        emitter.instruction("mov x1, x0");                                      // pass the borrowed previous object as the low boxing payload
        emitter.instruction("mov x2, xzr");                                     // objects and null use no high payload word
        emitter.instruction(&format!("cbz x0, {null}"));                        // represent a missing previous owner as PHP null
        emitter.instruction("mov x0, #6");                                      // select the object tag so boxing retains the previous Throwable
        emitter.instruction(&format!("b {boxed}"));                             // share allocation after choosing the concrete payload kind
        emitter.label(null);
        emitter.instruction("mov x0, #8");                                      // select boxed null rather than a fatal null return pointer
        emitter.label(boxed);
        emitter.instruction("bl __rt_mixed_from_value");                        // publish one independently owned eval result
        emitter.instruction(&format!("b {done_label}"));                        // return through the method bridge's protected epilogue
    } else {
        emit_x86_64_validate_builtin_throwable_method_arg_count(module, emitter, fail_label);
        emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                   // borrow the current native Throwable receiver
        emitter.instruction("call __rt_throwable_previous");                    // read its prior error without changing existing ownership
        emitter.instruction("mov rdi, rax");                                    // supply the borrowed object as the low boxing payload
        emitter.instruction("xor esi, esi");                                    // objects and null have no high payload word
        emitter.instruction("test rax, rax");                                   // detect the end of a native exception chain
        emitter.instruction(&format!("jz {null}"));                             // preserve a missing previous value as PHP null
        emitter.instruction("mov eax, 6");                                      // retain the borrowed previous Throwable through object boxing
        emitter.instruction(&format!("jmp {boxed}"));                           // share owned result allocation after tag selection
        emitter.label(null);
        emitter.instruction("mov eax, 8");                                      // use the nullable result tag instead of a fatal null pointer
        emitter.label(boxed);
        emitter.instruction("call __rt_mixed_from_value");                      // publish one independent eval owner of the previous value
        emitter.instruction(&format!("jmp {done_label}"));                      // restore method bridge state before returning to Magician
    }
}
