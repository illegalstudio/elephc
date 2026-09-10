//! Purpose:
//! Adapts native reference initialization to the capture coordinator's V4 output contract.
//!
//! Called from:
//! - The stack-local native capture host table after shared string coercion and pattern validation.
//!
//! Key details:
//! - The wrapped context retains an original eval context and a borrowed MbNativeCaptureV1 pointer.
//! - The larger internal result never aliases the V4 sixteen-byte output.
//! - Deferred ownership is published before status returns and survives coordinator cleanup.

use super::*;

const NAME: &str = "__rt_mbstring_capture_initialize";

/// Initializes one retained caller reference and transfers writer/deferred owners into separate outputs.
pub(super) fn emit(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    emitter.label_global(NAME);
    if arm {
        emitter.instruction(&format!("cbz x2, {NAME}_invalid"));                // require the coordinator's writable sixteen-byte output
        emitter.instruction("stp xzr, xzr, [x2]");                              // expose no writer or readiness until initialization completes
        emitter.instruction(&format!("cbz x0, {NAME}_invalid"));                // require this invocation's wrapped native context
        emitter.instruction("ldr x9, [x0, #8]");                                // recover borrowed capture mode and deferred ownership storage
        emitter.instruction(&format!("cbz x9, {NAME}_invalid"));                // a supplied output reference requires capture state
        emitter.instruction("sub sp, sp, #64");                                 // reserve a distinct internal initialization result and caller pointers
        emitter.instruction("stp x29, x30, [sp, #48]");                         // preserve linkage across destructive initialization
        emitter.instruction("str x2, [sp, #24]");                               // retain the coordinator's smaller output buffer
        emitter.instruction("str x9, [sp, #32]");                               // retain deferred ownership storage across PHP callbacks
        emitter.instruction("ldr x2, [x9]");                                    // select the already reviewed typed or untyped publication mode
        emitter.instruction("mov x0, #0");                                      // the low-level initializer needs no eval context
        emitter.instruction("mov x3, sp");                                      // pass separate twenty-four-byte result storage
    } else {
        emitter.instruction("test rdx, rdx");                                   // require writable coordinator output
        emitter.instruction(&format!("jz {NAME}_invalid"));                     // reject missing output before reading host metadata
        emitter.instruction("mov QWORD PTR [rdx], 0");                          // initialize readiness before validating the native context
        emitter.instruction("mov QWORD PTR [rdx + 8], 0");                      // initialize optional writer ownership
        emitter.instruction("test rdi, rdi");                                   // require a wrapped native host context
        emitter.instruction(&format!("jz {NAME}_invalid"));                     // reject missing context without touching the reference
        emitter.instruction("mov r10, QWORD PTR [rdi + 8]");                    // read borrowed capture state
        emitter.instruction("test r10, r10");                                   // output-reference calls require explicit publication policy
        emitter.instruction(&format!("jz {NAME}_invalid"));                     // leave the caller unchanged if capture state is absent
        emitter.instruction("push rbp");                                        // preserve linkage and align protected cleanup calls
        emitter.instruction("mov rbp, rsp");                                    // establish the initializer adapter frame
        emitter.instruction("sub rsp, 48");                                     // reserve the larger internal result and caller output pointers
        emitter.instruction("mov QWORD PTR [rsp + 24], rdx");                   // retain the V4 sixteen-byte output independently
        emitter.instruction("mov QWORD PTR [rsp + 32], r10");                   // retain deferred-owner state across callbacks
        emitter.instruction("mov rdx, QWORD PTR [r10]");                        // load typed or untyped initialization mode
        emitter.instruction("xor edi, edi");                                    // no eval context is needed by the low-level initializer
        emitter.instruction("mov rcx, rsp");                                    // provide separate twenty-four-byte result storage
    }
    abi::emit_call_label(emitter, "__rt_mbstring_capture_reference_begin");
    if arm {
        emitter.instruction("ldr x9, [sp, #24]");                               // recover the coordinator's output buffer
        emitter.instruction("ldp x10, x11, [sp]");                              // transfer readiness and writer ownership together
        emitter.instruction("stp x10, x11, [x9]");                              // publish the writer even when old-value destruction left a throwable
        emitter.instruction("ldr x9, [sp, #32]");                               // recover caller-owned deferred storage
        emitter.instruction("ldr x10, [sp, #16]");                              // transfer any overwritten reentrant PHP value
        emitter.instruction("str x10, [x9, #8]");                               // separate its lifetime from writer and argument retirement
        emitter.instruction("ldp x29, x30, [sp, #48]");                         // restore linkage without clobbering pending status
        emitter.instruction("add sp, sp, #64");                                 // retire internal result storage after transferring its owners
    } else {
        emitter.instruction("mov r10, QWORD PTR [rsp + 24]");                   // recover the coordinator's output buffer
        for offset in [0, 8] {
            emitter.instruction(&format!("mov r11, QWORD PTR [rsp + {offset}]"));// read readiness or its accompanying writer owner
            emitter.instruction(&format!("mov QWORD PTR [r10 + {offset}], r11"));// publish independently of the returned pending status
        }
        emitter.instruction("mov r10, QWORD PTR [rsp + 32]");                   // recover caller-owned deferred storage
        emitter.instruction("mov r11, QWORD PTR [rsp + 16]");                   // transfer the overwritten reentrant owner
        emitter.instruction("mov QWORD PTR [r10 + 8], r11");                    // preserve request lifetime beyond coordinator cleanup
        emitter.instruction("leave");                                           // retire internal result storage while preserving status in rax
    }
    emitter.instruction("ret");                                                 // return ready/success, ready/pending, or initialization failure
    emitter.label(&format!("{NAME}_invalid"));
    emitter.instruction(if arm { "mov x0, #1" } else { "mov eax, 1" });         // report missing native adapter state without mutating the caller
    emitter.instruction("ret");                                                 // return before acquiring any writer ownership
}
