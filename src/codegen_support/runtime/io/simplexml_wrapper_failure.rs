//! Purpose:
//! Emits the `__rt_simplexml_wrapper_failure` helper that reproduces PHP's chained
//! TypeError when a user stream wrapper is registered with a SimpleXML class.
//!
//! Called from:
//! - `crate::codegen_support::runtime::io::emit_simplexml_wrapper_failure()`.
//!
//! Key details:
//! - php-src assigns the stream context RESOURCE to the wrapper object's `context`
//!   property. SimpleXML rejects that write with "SimpleXMLElement is not properly
//!   initialized", then the type check raises the resource TypeError with that Error as
//!   its previous exception.
//! - Both Throwables use the compact runtime layout; the fresh Error reference is
//!   transferred into the TypeError previous slot so destruction releases it exactly once.

use elephc_memoir::values::throwable_storage;
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Message of the Error SimpleXML raises when the wrapper context write reaches an
/// uninitialized object.
pub(crate) const SIMPLEXML_WRAPPER_INIT_MESSAGE: &str = "SimpleXMLElement is not properly initialized";

/// Message php-src raises when the wrapper context resource cannot be stored.
pub(crate) const SIMPLEXML_WRAPPER_CONTEXT_MESSAGE: &str =
    "It's not possible to assign a complex type to properties, resource given";

/// Emits `__rt_simplexml_wrapper_failure` for the active target.
pub(crate) fn emit_simplexml_wrapper_failure(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64(emitter),
        Arch::X86_64 => emit_x86_64(emitter),
    }
}

/// Emits the AArch64 chained-exception helper.
fn emit_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: simplexml_wrapper_failure ---");
    emitter.label_global("__rt_simplexml_wrapper_failure");

    // -- frame: [sp,#0] unused, [sp,#16] saved x19, [sp,#32] x29/x30 --
    emitter.instruction("sub sp, sp, #48");                                     // reserve the helper frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve the caller frame and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the helper frame pointer
    emitter.instruction("str x19, [sp, #16]");                                  // preserve the callee-saved previous-exception owner

    // -- allocate the initialization Error --
    emit_error_aarch64(emitter, "_spl_error_class_id",
        "_simplexml_wrapper_init_message", SIMPLEXML_WRAPPER_INIT_MESSAGE.len());
    emitter.instruction("mov x19, x0");                                         // retain the Error pointer as the previous exception

    // -- allocate the TypeError with the Error as previous --
    emit_error_aarch64(emitter, "_spl_type_error_class_id",
        "_simplexml_wrapper_context_message", SIMPLEXML_WRAPPER_CONTEXT_MESSAGE.len());
    emitter.instruction("str x19, [x0, #40]");                                  // transfer the fresh Error reference into previous
    abi::emit_store_reg_to_symbol(emitter, "x0", "_exc_value", 0);              // publish the TypeError for the unwinder

    emitter.instruction("ldr x19, [sp, #16]");                                  // restore the callee-saved register
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore the caller frame and return address
    emitter.instruction("add sp, sp, #48");                                     // release the helper frame
    abi::emit_call_label(emitter, "__rt_throw_current");                        // unwind to the nearest PHP catch handler
    emitter.instruction("ret");                                                 // unreachable: throw_current does not return here
}

/// Allocates and initializes one compact AArch64 Throwable in `x0`.
fn emit_error_aarch64(emitter: &mut Emitter, class_id: &str, message: &str, length: usize) {
    throwable_storage::emit_allocate(emitter);
    throwable_storage::emit_class_id(emitter, class_id);
    throwable_storage::emit_static_message(emitter, message, length as i64, throwable_storage::LengthStore::Register);
    throwable_storage::emit_default_fields(emitter, "x0");
}

/// Emits the x86_64 chained-exception helper.
fn emit_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: simplexml_wrapper_failure ---");
    emitter.label_global("__rt_simplexml_wrapper_failure");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame base
    emitter.instruction("sub rsp, 16");                                         // reserve the saved callee register slot
    emitter.instruction("mov QWORD PTR [rbp - 8], r12");                        // preserve the callee-saved previous-exception owner

    // -- allocate the initialization Error --
    emit_error_x86_64(emitter, "_spl_error_class_id",
        "_simplexml_wrapper_init_message", SIMPLEXML_WRAPPER_INIT_MESSAGE.len());
    emitter.instruction("mov r12, rax");                                        // retain the Error pointer as the previous exception

    // -- allocate the TypeError with the Error as previous --
    emit_error_x86_64(emitter, "_spl_type_error_class_id",
        "_simplexml_wrapper_context_message", SIMPLEXML_WRAPPER_CONTEXT_MESSAGE.len());
    emitter.instruction("mov QWORD PTR [rax + 40], r12");                       // transfer the fresh Error reference into previous
    abi::emit_store_reg_to_symbol(emitter, "rax", "_exc_value", 0);            // publish the TypeError for the unwinder

    emitter.instruction("mov r12, QWORD PTR [rbp - 8]");                        // restore the callee-saved register
    emitter.instruction("mov rsp, rbp");                                        // release the helper frame; rsp stays 16-byte aligned
    abi::emit_call_label(emitter, "__rt_throw_current");                        // unwind to the nearest PHP catch handler
    emitter.instruction("ret");                                                 // unreachable: throw_current does not return here
}

/// Allocates and initializes one compact x86_64 Throwable in `rax`.
fn emit_error_x86_64(emitter: &mut Emitter, class_id: &str, message: &str, length: usize) {
    throwable_storage::emit_allocate(emitter);
    throwable_storage::emit_class_id(emitter, class_id);
    throwable_storage::emit_static_message(emitter, message, length as i64, throwable_storage::LengthStore::Register);
    throwable_storage::emit_default_fields(emitter, "rax");
}
