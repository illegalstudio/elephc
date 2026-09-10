//! Purpose:
//! Bridges native output-handler invocation to the versioned eval callback request.
//!
//! Called from:
//! - `super::ob_handler::emit_ob_eval_trampoline()` during runtime emission.
//!
//! Key details:
//! - The 48-byte record matches the shared builtin contract on all supported targets.
//! - Returned PHP exceptions are published only after the Rust hook has returned.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Marshals the handler stub arguments into a C request with distinct result and exception owners.
pub(super) fn emit(emitter: &mut Emitter) {
    emitter.label_global("__rt_ob_eval_trampoline");
    if emitter.target.arch == Arch::AArch64 {
        emitter.instruction("sub sp, sp, #64");                                 // reserve the callback record and native linkage
        emitter.instruction("stp x29, x30, [sp, #48]");                         // preserve the native caller across Rust execution
        emitter.instruction("add x29, sp, #48");                                // establish the callback frame
        emitter.instruction("stp x0, x1, [sp]");                                // store registry identity and borrowed byte pointer
        emitter.instruction("stp x2, x3, [sp, #16]");                           // store byte length and handler phase
        emitter.instruction("stp xzr, xzr, [sp, #32]");                         // initialize independently transferred output owners
        abi::emit_load_symbol_to_reg(emitter, "x10", "_elephc_eval_ob_handler_fn", 0);
        emitter.instruction("cbz x10, __rt_ob_eval_result");                    // missing hooks leave a null pass-through result
        emitter.instruction("mov x0, sp");                                      // pass exclusive request storage using the C ABI
        emitter.instruction("blr x10");                                         // return from Rust before interpreting callback status
        emitter.instruction("cbz x0, __rt_ob_eval_result");                     // consume the successful result independently of its value
        emitter.instruction("cmp x0, #2");                                      // distinguish PHP exceptions from fatal callback errors
        emitter.instruction("b.ne __rt_ob_eval_fatal");                         // preserve a fatal callback failure
        emitter.instruction("ldr x0, [sp, #40]");                               // transfer the pending boxed Throwable owner
        emitter.instruction("cbz x0, __rt_ob_eval_fatal");                      // reject exception status without ownership
        emitter.instruction("ldp x29, x30, [sp, #48]");                         // restore native linkage after Rust returned
        emitter.instruction("add sp, sp, #64");                                 // leave the request frame before native unwinding
        emitter.instruction("b __rt_destructor_throw_mixed");                   // publish and propagate the owned PHP exception
        emitter.label("__rt_ob_eval_result");
        emitter.instruction("ldr x0, [sp, #32]");                               // transfer the returned Mixed cell or null pass-through
        emitter.instruction("ldp x29, x30, [sp, #48]");                         // restore the caller for result conversion
        emitter.instruction("add sp, sp, #64");                                 // release request staging
        emitter.instruction("b __rt_ob_result_to_bytes");                       // consume the result through the shared byte conversion
        emitter.label("__rt_ob_eval_fatal");
        emitter.instruction("mov x0, #1");                                      // terminate on an unrecoverable callback status
    } else {
        emitter.instruction("push rbp");                                        // preserve caller linkage and align Rust calls
        emitter.instruction("mov rbp, rsp");                                    // establish callback request staging
        emitter.instruction("sub rsp, 48");                                     // reserve the six-word version-one record
        emitter.instruction("mov QWORD PTR [rsp], rdi");                        // retain the registry identity
        emitter.instruction("mov QWORD PTR [rsp + 8], rsi");                    // retain borrowed buffer bytes
        emitter.instruction("mov QWORD PTR [rsp + 16], rdx");                   // retain the readable byte count
        emitter.instruction("mov QWORD PTR [rsp + 24], rcx");                   // retain the computed handler phase
        emitter.instruction("mov QWORD PTR [rsp + 32], 0");                     // initialize the owned result output
        emitter.instruction("mov QWORD PTR [rsp + 40], 0");                     // initialize the owned Throwable output
        abi::emit_load_symbol_to_reg(emitter, "r10", "_elephc_eval_ob_handler_fn", 0);
        emitter.instruction("test r10, r10");                                   // inspect whether a runtime eval hook is installed
        emitter.instruction("jz __rt_ob_eval_result");                          // missing hooks retain a null pass-through result
        emitter.instruction("mov rdi, rsp");                                    // pass exclusive request storage with the C ABI
        emitter.instruction("call r10");                                        // finish Rust execution before any PHP propagation
        emitter.instruction("test rax, rax");                                   // inspect callback success independently of its returned value
        emitter.instruction("jz __rt_ob_eval_result");                          // convert the successful owned result
        emitter.instruction("cmp rax, 2");                                      // recognize a pending PHP Throwable
        emitter.instruction("jne __rt_ob_eval_fatal");                          // preserve fatal callback failures
        emitter.instruction("mov rax, QWORD PTR [rsp + 40]");                   // transfer the boxed exception owner
        emitter.instruction("test rax, rax");                                   // require ownership before publishing an exception
        emitter.instruction("jz __rt_ob_eval_fatal");                           // reject an incomplete callback result
        emitter.instruction("leave");                                           // retire request staging before native unwinding
        emitter.instruction("jmp __rt_destructor_throw_mixed");                 // consume and propagate the PHP exception after Rust exits
        emitter.label("__rt_ob_eval_result");
        emitter.instruction("mov rax, QWORD PTR [rsp + 32]");                   // transfer the returned Mixed cell or null
        emitter.instruction("leave");                                           // restore native caller linkage
        emitter.instruction("jmp __rt_ob_result_to_bytes");                     // consume the result using the shared mapping
        emitter.label("__rt_ob_eval_fatal");
        emitter.instruction("mov edi, 1");                                      // terminate on an unrecoverable callback protocol failure
    }
    emitter.bl_c("exit");
}
