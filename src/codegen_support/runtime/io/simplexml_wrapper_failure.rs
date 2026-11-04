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

use crate::codegen_support::sentinels::{
    emit_throwable_creation_line_unknown, x86_64_heap_kind_word,
};
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Message of the Error SimpleXML raises when the wrapper context write reaches an
/// uninitialized object.
pub(crate) const SIMPLEXML_WRAPPER_INIT_MESSAGE: &str = "SimpleXMLElement is not properly initialized";

/// Message php-src raises when the wrapper context resource cannot be stored.
pub(crate) const SIMPLEXML_WRAPPER_CONTEXT_MESSAGE: &str =
    "It's not possible to assign a complex type to properties, resource given";

/// Compact Throwable payload size shared by every runtime-created exception.
const THROWABLE_PAYLOAD_SIZE: u64 = 56;

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
    abi::emit_load_int_immediate(emitter, "x0", THROWABLE_PAYLOAD_SIZE as i64); // request one compact Throwable payload
    abi::emit_call_label(emitter, "__rt_heap_alloc");                          // x0 = fresh payload storage
    emitter.instruction("mov x9, #6");                                          // heap kind 6 marks runtime object payloads
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the heap header before the payload
    abi::emit_call_label(emitter, "__rt_object_handle_acquire");               // bind the new object to its PHP handle
    abi::emit_load_symbol_to_reg(emitter, "x9", class_id, 0);                  // load the runtime Throwable class id
    emitter.instruction("str x9, [x0]");                                        // store the class id at payload offset zero
    abi::emit_symbol_address(emitter, "x9", message);                          // load the static message pointer
    emitter.instruction("str x9, [x0, #8]");                                    // store the message pointer
    abi::emit_load_int_immediate(emitter, "x9", length as i64);                // load the message byte length
    emitter.instruction("str x9, [x0, #16]");                                   // store the message length
    emitter.instruction("str xzr, [x0, #24]");                                  // exception code defaults to zero
    emit_throwable_creation_line_unknown(emitter, "x0");                       // no user `new` location for this internal throwable
    emitter.instruction("str xzr, [x0, #40]");                                  // previous defaults to null
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
    emitter.instruction("mov rsp, rbp");                                        // release the helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    abi::emit_call_label(emitter, "__rt_throw_current");                        // unwind to the nearest PHP catch handler
    emitter.instruction("ret");                                                 // unreachable: throw_current does not return here
}

/// Allocates and initializes one compact x86_64 Throwable in `rax`.
fn emit_error_x86_64(emitter: &mut Emitter, class_id: &str, message: &str, length: usize) {
    emitter.instruction(&format!("mov rax, {THROWABLE_PAYLOAD_SIZE}"));         // request one compact Throwable payload
    abi::emit_call_label(emitter, "__rt_heap_alloc");                          // rax = fresh payload storage
    emitter.instruction(&format!(
        "mov r10, 0x{:x}",
        x86_64_heap_kind_word(6)
    ));                                                                         // materialize the x86_64 Throwable heap kind word
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the heap header before the payload
    abi::emit_call_label(emitter, "__rt_object_handle_acquire");               // bind the new object to its PHP handle
    abi::emit_load_symbol_to_reg(emitter, "r10", class_id, 0);                 // load the runtime Throwable class id
    emitter.instruction("mov QWORD PTR [rax], r10");                            // store the class id at payload offset zero
    abi::emit_symbol_address(emitter, "r10", message);                         // load the static message pointer
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // store the message pointer
    abi::emit_load_int_immediate(emitter, "r10", length as i64);               // load the message byte length
    emitter.instruction("mov QWORD PTR [rax + 16], r10");                       // store the message length
    emitter.instruction("mov QWORD PTR [rax + 24], 0");                         // exception code defaults to zero
    emit_throwable_creation_line_unknown(emitter, "rax");                      // no user `new` location for this internal throwable
    emitter.instruction("mov QWORD PTR [rax + 40], 0");                         // previous defaults to null
}
