//! Purpose:
//! Contains native callbacks behind complete PHP exception-handler records.
//!
//! Called from:
//! - Owned-value exception guards and the mbstring diagnostic/release callback emitters.
//!
//! Key details:
//! - Four C inputs are retained at offsets 224, 232, 240, and 248 on both architectures.
//! - Handler, activation, diagnostic suppression, and GC suppression are restored on both returns.
//! - A pending throwable returns status two; recursive deep-free completion needs separate GC handling.

use elephc_memoir::activation_records;

use crate::codegen_support::{emit::Emitter, platform::Arch};
use crate::codegen_support::try_handlers::{TRY_HANDLER_JMP_BUF_OFFSET, TRY_HANDLER_SLOT_SIZE};

/// Emits an exception-contained callback around a body using the retained C input slots.
pub(crate) fn emit(emitter: &mut Emitter, name: &str, body: fn(&mut Emitter)) {
    emit_inner(emitter, name, body, false);
}

/// Contains PHP exceptions while preserving an ordinary C status returned by the callback body.
pub(crate) fn emit_status(emitter: &mut Emitter, name: &str, body: fn(&mut Emitter)) {
    emit_inner(emitter, name, body, true);
}

/// Selects the target boundary and whether its body already supplies the success/failure status.
fn emit_inner(emitter: &mut Emitter, name: &str, body: fn(&mut Emitter), body_status: bool) {
    assert_eq!(TRY_HANDLER_SLOT_SIZE, 224, "protected callback spills follow the complete native handler");
    emitter.label_global(name);
    if emitter.target.arch == Arch::AArch64 { aarch64(emitter, name, body, body_status); }
    else { x86_64(emitter, name, body, body_status); }
}

/// Installs an AArch64 exception boundary and restores every captured runtime state after a throw.
fn aarch64(emitter: &mut Emitter, name: &str, body: fn(&mut Emitter), body_status: bool) {
    emitter.instruction("sub sp, sp, #288");                                    // reserve the full PHP handler, callback inputs, GC state, and linkage
    emitter.instruction("stp x29, x30, [sp, #272]");                            // preserve the C caller across setjmp and nested PHP execution
    emitter.instruction("add x29, sp, #272");                                   // establish a stable protected callback frame
    emitter.instruction("stp x0, x1, [sp, #224]");                              // retain the first two callback-specific C arguments
    emitter.instruction("stp x2, x3, [sp, #240]");                              // retain byte/output pointers and optional message length
    activation_records::emit_save_runtime_try_prefix(emitter, "x10");
    activation_records::emit_save_gc_release_suppression(emitter, "x10", 256);
    emitter.instruction("mov x10, sp");                                         // publish this stack record as the PHP exception boundary
    activation_records::emit_publish_try_handler(emitter, "x10");
    emitter.instruction(&format!("add x0, sp, #{TRY_HANDLER_JMP_BUF_OFFSET}")); // address the platform jump buffer inside the handler record
    emitter.save_exception_handler();
    emitter.instruction(&format!("cbnz x0, {name}_throw"));                     // convert PHP longjmp into a returned callback status
    body(emitter);
    if !body_status {
        emitter.instruction("mov x0, #0");                                      // successful callback completion transfers no pending exception
    }
    emitter.instruction(&format!("b {name}_done"));                             // share runtime-state restoration with the exceptional return
    emitter.label(&format!("{name}_throw"));
    emitter.instruction("mov x0, #2");                                          // preserve the throwable already published by the PHP unwinder
    emitter.label(&format!("{name}_done"));
    activation_records::emit_restore_runtime_try_prefix(emitter, "x10");
    activation_records::emit_restore_gc_release_suppression(emitter, "x10", 256);
    emitter.instruction("ldp x29, x30, [sp, #272]");                            // restore the original C caller after callback cleanup
    emitter.instruction("add sp, sp, #288");                                    // release the complete exception boundary and callback spills
    emitter.instruction("ret");                                                 // return success or PendingThrowable without unwinding through Rust
}

/// Installs the equivalent SysV boundary with aligned C calls and explicit GC-state restoration.
fn x86_64(emitter: &mut Emitter, name: &str, body: fn(&mut Emitter), body_status: bool) {
    emitter.instruction("push rbp");                                            // preserve linkage and align the protected C callback frame
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame across setjmp and PHP calls
    emitter.instruction("sub rsp, 272");                                        // reserve the complete handler, four inputs, and saved GC state
    for (offset, register) in [(224, "rdi"), (232, "rsi"), (240, "rdx"), (248, "rcx")] {
        emitter.instruction(&format!("mov QWORD PTR [rsp + {offset}], {register}")); // retain each incoming C argument before setjmp clobbers registers
    }
    activation_records::emit_save_runtime_try_prefix(emitter, "r10");
    activation_records::emit_save_gc_release_suppression(emitter, "r10", 256);
    emitter.instruction("mov r10, rsp");                                        // publish this stack record as the protected PHP handler
    activation_records::emit_publish_try_handler(emitter, "r10");
    emitter.instruction(&format!("lea rdi, [rsp + {TRY_HANDLER_JMP_BUF_OFFSET}]")); // address the platform jump buffer through the first C argument
    emitter.save_exception_handler();
    emitter.instruction("test eax, eax");                                       // distinguish initial callback execution from an escaping PHP throw
    emitter.instruction(&format!("jnz {name}_throw"));                          // return a pending status after restoring the caller's runtime state
    body(emitter);
    if !body_status {
        emitter.instruction("xor eax, eax");                                    // report successful callback completion
    }
    emitter.instruction(&format!("jmp {name}_done"));                           // share state restoration with the exceptional callback return
    emitter.label(&format!("{name}_throw"));
    emitter.instruction("mov eax, 2");                                          // retain the native unwinder's published pending throwable
    emitter.label(&format!("{name}_done"));
    activation_records::emit_restore_runtime_try_prefix(emitter, "r10");
    activation_records::emit_restore_gc_release_suppression(emitter, "r10", 256);
    emitter.instruction("leave");                                               // discard the full handler and restore the C caller frame
    emitter.instruction("ret");                                                 // return success or PendingThrowable without crossing Rust via longjmp
}
