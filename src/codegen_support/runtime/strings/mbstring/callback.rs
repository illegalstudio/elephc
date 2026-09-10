//! Purpose:
//! Supplies protected callable resolution and invocation to the mbregex replacement coordinator.
//!
//! Called from:
//! - `super::emit_mbstring()` when the mbregex capability is enabled.
//!
//! Key details:
//! - C callbacks return statuses after all PHP exception propagation is contained.
//! - Target helpers address aligned private frames; callback-specific owners have explicit guards.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch, runtime::exceptions::{emit_protected_status, guards}};

mod resolve;
mod invoke;
mod values;

/// Emits both protected host operations and their private native frames.
pub(super) fn emit(emitter: &mut Emitter, eval: bool) {
    emit_protected_status(emitter, "__rt_mbstring_callback_resolve", resolve_boundary);
    emit_protected_status(emitter, "__rt_mbstring_callback_call", call_boundary);
    resolve::emit(emitter, eval);
    invoke::emit(emitter, eval);
    values::emit(emitter);
}

/// Restores the three C inputs inside the installed exception boundary before resolving a callable.
fn resolve_boundary(emitter: &mut Emitter) { boundary_body(emitter, "__rt_mbstring_callback_resolve_body"); }

/// Restores the retained C inputs before allocating and invoking a capture-array argument.
fn call_boundary(emitter: &mut Emitter) { boundary_body(emitter, "__rt_mbstring_callback_call_body"); }

/// Calls a private body while the shared handler owns every escaping PHP exception.
fn boundary_body(emitter: &mut Emitter, label: &str) {
    for (index, offset) in [(0, 224), (1, 232), (2, 240)] { load_arg(emitter, index, offset); }
    abi::emit_call_label(emitter, label);
}

/// Reserves a target-aligned payload frame and retains the original three C arguments.
fn enter(emitter: &mut Emitter, bytes: usize) {
    if arm(emitter) {
        emitter.instruction(&format!("sub sp, sp, #{}", bytes + 16));           // reserve payload storage and native linkage
        emitter.instruction(&format!("stp x29, x30, [sp, #{bytes}]"));          // preserve the caller across runtime and PHP calls
        emitter.instruction(&format!("add x29, sp, #{bytes}"));                 // establish a stable private frame
    } else {
        emitter.instruction("push rbp");                                        // preserve linkage and align nested C calls
        emitter.instruction("mov rbp, rsp");                                    // establish the private frame pointer
        emitter.instruction(&format!("sub rsp, {bytes}"));                      // reserve aligned callback payload storage
    }
    for index in 0..3 { save(emitter, abi::int_arg_reg_name(emitter.target, index), index * 8); }
}

/// Restores caller linkage while leaving the native result register unchanged.
fn leave(emitter: &mut Emitter, bytes: usize) {
    if arm(emitter) {
        emitter.instruction(&format!("ldp x29, x30, [sp, #{bytes}]"));          // restore linkage after normal cleanup
        emitter.instruction(&format!("add sp, sp, #{}", bytes + 16));           // release callback-local storage
    } else { emitter.instruction("leave"); }                                    // restore linkage and release the payload frame
    emitter.instruction("ret");                                                 // return a value or contained callback status
}

/// Returns whether the emitter uses the supported AArch64 calling convention.
fn arm(emitter: &Emitter) -> bool { emitter.target.arch == Arch::AArch64 }

/// Loads one payload word through a target-specific stack address.
fn load(emitter: &mut Emitter, register: &str, offset: usize) {
    if arm(emitter) { emitter.instruction(&format!("ldr {register}, [sp, #{offset}]")); } // recover a retained callback value
    else { emitter.instruction(&format!("mov {register}, QWORD PTR [rsp + {offset}]")); } // recover the same retained payload word
}

/// Retains one register in callback-local payload storage.
fn save(emitter: &mut Emitter, register: &str, offset: usize) {
    if arm(emitter) { emitter.instruction(&format!("str {register}, [sp, #{offset}]")); } // retain a callback value across nested calls
    else { emitter.instruction(&format!("mov QWORD PTR [rsp + {offset}], {register}")); } // retain the same callback payload word
}

/// Restores one C argument from retained callback storage.
fn load_arg(emitter: &mut Emitter, index: usize, offset: usize) {
    load(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
}

/// Loads a retained owner using the native result-register convention.
fn load_result(emitter: &mut Emitter, offset: usize) { load(emitter, abi::int_result_reg(emitter), offset); }

/// Retains the current native result as an explicit private-frame owner.
fn save_result(emitter: &mut Emitter, offset: usize) { save(emitter, abi::int_result_reg(emitter), offset); }

/// Branches when the current result pointer or boolean is zero.
fn branch_zero(emitter: &mut Emitter, label: &str) {
    if arm(emitter) { emitter.instruction(&format!("cbz x0, {label}")); }       // reject an absent owner or failed validation
    else {
        emitter.instruction("test rax, rax");                                   // inspect the native result without changing it
        emitter.instruction(&format!("jz {label}"));                            // select the absent-owner or validation failure path
    }
}
