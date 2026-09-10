//! Purpose:
//! Initializes the managed Oniguruma provider for both direct and interpreted mbregex calls.
//!
//! Called from:
//! - The shared mbstring invocation entry and eval-context setup when mbregex is enabled.
//!
//! Key details:
//! - All five incoming invocation arguments survive provider initialization on both architectures.
//! - The shared bridge initializes Oniguruma once and rejects inconsistent provider tables.

use super::*;

/// Emits an idempotent provider registration boundary that preserves the incoming PHP call arguments.
pub(super) fn emit(emitter: &mut Emitter) {
    emitter.label_global("__rt_mbregex_init");
    let arm = emitter.target.arch == Arch::AArch64;
    if arm {
        emitter.instruction("stp x29, x30, [sp, #-64]!");                       // retain native linkage and reserve five argument slots
        emitter.instruction("mov x29, sp");                                     // establish an aligned registration frame
        emitter.instruction("stp x0, x1, [sp, #16]");                           // retain operation identity and borrowed argument pointers
        emitter.instruction("stp x2, x3, [sp, #32]");                           // retain argument count and caller strictness
        emitter.instruction("str x4, [sp, #48]");                               // retain the optional eval context
    } else {
        emitter.instruction("push rbp");                                        // preserve linkage and align the native provider calls
        emitter.instruction("mov rbp, rsp");                                    // establish the registration frame
        emitter.instruction("sub rsp, 48");                                     // reserve five incoming C argument registers and padding
        emitter.instruction("mov QWORD PTR [rsp], rdi");                        // preserve the selected operation identity
        emitter.instruction("mov QWORD PTR [rsp + 8], rsi");                    // preserve borrowed argument pointers
        emitter.instruction("mov QWORD PTR [rsp + 16], rdx");                   // preserve the exact supplied argument count
        emitter.instruction("mov QWORD PTR [rsp + 24], rcx");                   // preserve caller strictness
        emitter.instruction("mov QWORD PTR [rsp + 32], r8");                    // preserve the optional eval context
    }
    emitter.bl_c("elephc_oniguruma_v1_provider");
    if !arm { emitter.instruction("mov rdi, rax"); }                            // pass the native table through the SysV first argument register
    emitter.bl_c("elephc_mbstring_regex_provider_v1");
    if arm {
        emitter.instruction("cbz w0, __rt_mbregex_init_ready");                 // continue only after complete provider validation and initialization
        emitter.instruction("mov x0, #1");                                      // preserve fatal process status for an invalid native integration
    } else {
        emitter.instruction("test eax, eax");                                   // inspect the shared provider installation status
        emitter.instruction("jz __rt_mbregex_init_ready");                      // continue only with the reviewed initialized provider
        emitter.instruction("mov edi, 1");                                      // report fatal native integration failure
    }
    emitter.bl_c("exit");
    emitter.label("__rt_mbregex_init_ready");
    if arm {
        emitter.instruction("ldr x4, [sp, #48]");                               // restore the caller's optional eval context
        emitter.instruction("ldp x2, x3, [sp, #32]");                           // restore count and strictness after native initialization
        emitter.instruction("ldp x0, x1, [sp, #16]");                           // restore the operation and borrowed argument pointers
        emitter.instruction("ldp x29, x30, [sp], #64");                         // release registration storage and restore linkage
    } else {
        emitter.instruction("mov r8, QWORD PTR [rsp + 32]");                    // restore the optional eval context
        emitter.instruction("mov rcx, QWORD PTR [rsp + 24]");                   // restore the caller's strictness
        emitter.instruction("mov rdx, QWORD PTR [rsp + 16]");                   // restore the supplied count
        emitter.instruction("mov rsi, QWORD PTR [rsp + 8]");                    // restore borrowed argument pointers
        emitter.instruction("mov rdi, QWORD PTR [rsp]");                        // restore the selected operation identity
        emitter.instruction("leave");                                           // release registration storage and restore linkage
    }
    emitter.instruction("ret");                                                 // resume argument preparation with the original invocation registers
}
