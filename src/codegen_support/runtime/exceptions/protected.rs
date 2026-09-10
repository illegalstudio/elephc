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

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
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
    for (offset, symbol) in [(0, "_exc_handler_top"), (8, "_exc_call_frame_top"),
        (16, "_rt_diag_suppression"), (256, "_gc_release_suppressed")] {
        abi::emit_load_symbol_to_reg(emitter, "x10", symbol, 0);
        emitter.instruction(&format!("str x10, [sp, #{offset}]"));              // preserve the caller's exception, suppression, or GC state
    }
    emitter.instruction("mov x10, sp");                                         // publish this stack record as the PHP exception boundary
    abi::emit_store_reg_to_symbol(emitter, "x10", "_exc_handler_top", 0);
    emitter.instruction(&format!("add x0, sp, #{TRY_HANDLER_JMP_BUF_OFFSET}")); // address the platform jump buffer inside the handler record
    emitter.bl_c("setjmp");
    emitter.instruction(&format!("cbnz x0, {name}_throw"));                     // convert PHP longjmp into a returned callback status
    body(emitter);
    if !body_status {
        emitter.instruction("mov x0, #0");                                      // successful callback completion transfers no pending exception
    }
    emitter.instruction(&format!("b {name}_done"));                             // share runtime-state restoration with the exceptional return
    emitter.label(&format!("{name}_throw"));
    emitter.instruction("mov x0, #2");                                          // preserve the throwable already published by the PHP unwinder
    emitter.label(&format!("{name}_done"));
    for (offset, symbol) in [(0, "_exc_handler_top"), (8, "_exc_call_frame_top"),
        (16, "_rt_diag_suppression"), (256, "_gc_release_suppressed")] {
        emitter.instruction(&format!("ldr x10, [sp, #{offset}]"));              // recover the state captured before any nested callback
        abi::emit_store_reg_to_symbol(emitter, "x10", symbol, 0);
    }
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
    for (offset, symbol) in [(0, "_exc_handler_top"), (8, "_exc_call_frame_top"),
        (16, "_rt_diag_suppression"), (256, "_gc_release_suppressed")] {
        abi::emit_load_symbol_to_reg(emitter, "r10", symbol, 0);
        emitter.instruction(&format!("mov QWORD PTR [rsp + {offset}], r10"));   // preserve the caller's exception, suppression, or GC state
    }
    emitter.instruction("mov r10, rsp");                                        // publish this stack record as the protected PHP handler
    abi::emit_store_reg_to_symbol(emitter, "r10", "_exc_handler_top", 0);
    emitter.instruction(&format!("lea rdi, [rsp + {TRY_HANDLER_JMP_BUF_OFFSET}]")); // address the platform jump buffer through the first C argument
    emitter.bl_c("setjmp");
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
    for (offset, symbol) in [(0, "_exc_handler_top"), (8, "_exc_call_frame_top"),
        (16, "_rt_diag_suppression"), (256, "_gc_release_suppressed")] {
        emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {offset}]"));   // recover each state captured before nested PHP execution
        abi::emit_store_reg_to_symbol(emitter, "r10", symbol, 0);
    }
    emitter.instruction("leave");                                               // discard the full handler and restore the C caller frame
    emitter.instruction("ret");                                                 // return success or PendingThrowable without crossing Rust via longjmp
}
