//! Purpose:
//! Emits shared runtime throw sequences for helpers that raise a Throwable with a static
//! message (`ValueError`, or any class through the class-parameterized variants).
//! Keeps exception object allocation details out of individual helper loops.
//!
//! Called from:
//! - `crate::codegen_support::runtime::strings::mb_strlen`.
//! - `crate::codegen_support::runtime::strings::str_offset_set` (plain `Error`).
//!
//! Key details:
//! - The emitted sequence does not return; it publishes `_exc_value` and enters the unwinder.

use elephc_memoir::values::throwable_storage;
use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;

/// Emits an ARM64 `ValueError` throw using a static message symbol.
///
/// Allocates a 32-byte Throwable payload, stamps the per-program `ValueError`
/// class id, stores the message pointer/length and zero code, then jumps to
/// `__rt_throw_current`.
pub(in crate::codegen_support::runtime) fn emit_throw_value_error_aarch64(
    emitter: &mut Emitter,
    message_symbol: &str,
    message_len: usize,
) {
    emit_throw_static_message_aarch64(
        emitter,
        "_spl_value_error_class_id",
        message_symbol,
        message_len,
    );
}

/// Emits an ARM64 throw of the Throwable class named by `class_id_symbol` with a static message.
///
/// Allocates the Throwable payload, stamps the per-program class id read from
/// `class_id_symbol`, stores the message pointer/length and zero code, then jumps to
/// `__rt_throw_current`. Never returns.
pub(in crate::codegen_support::runtime) fn emit_throw_static_message_aarch64(
    emitter: &mut Emitter,
    class_id_symbol: &str,
    message_symbol: &str,
    message_len: usize,
) {
    throwable_storage::emit_allocate(emitter);
    abi::emit_symbol_address(emitter, "x9", class_id_symbol);
    emitter.instruction("ldr x9, [x9]");                                        // load the Throwable's runtime class id for this program
    emitter.instruction("str x9, [x0]");                                        // store class id at the object header
    abi::emit_symbol_address(emitter, "x9", message_symbol);
    abi::emit_store_to_address(emitter, "x9", "x0", throwable_storage::MESSAGE_POINTER);
    emitter.instruction(&format!("mov x9, #{}", message_len));                  // load static ValueError message length
    abi::emit_store_to_address(emitter, "x9", "x0", throwable_storage::MESSAGE_LENGTH);
    throwable_storage::emit_default_fields(emitter, "x0");
    abi::emit_symbol_address(emitter, "x9", "_exc_value");
    emitter.instruction("str x0, [x9]");                                        // publish the active exception object
    emitter.instruction("b __rt_throw_current");                                // enter the standard exception unwinder
}

/// Emits an x86_64 Linux `ValueError` throw using a static message symbol.
///
/// Preserves `rbp`, aligns the nested allocation call, writes the standard
/// Throwable payload layout, and jumps to `__rt_throw_current` without returning.
pub(in crate::codegen_support::runtime) fn emit_throw_value_error_x86_64(
    emitter: &mut Emitter,
    message_symbol: &str,
    message_len: usize,
) {
    emit_throw_static_message_x86_64(
        emitter,
        "_spl_value_error_class_id",
        message_symbol,
        message_len,
    );
}

/// Emits an x86_64 Linux throw of the Throwable class named by `class_id_symbol` with a
/// static message.
///
/// Preserves `rbp`, aligns the nested allocation call, writes the standard Throwable payload
/// layout, and jumps to `__rt_throw_current` without returning.
pub(in crate::codegen_support::runtime) fn emit_throw_static_message_x86_64(
    emitter: &mut Emitter,
    class_id_symbol: &str,
    message_symbol: &str,
    message_len: usize,
) {
    emitter.instruction("push rbp");                                            // preserve caller frame pointer for exception allocation
    emitter.instruction("mov rbp, rsp");                                        // establish aligned helper frame
    emitter.instruction("sub rsp, 16");                                         // keep the nested heap allocation call 16-byte aligned
    throwable_storage::emit_allocate(emitter);
    abi::emit_load_symbol_to_reg(emitter, "r10", class_id_symbol, 0);           // load the Throwable's runtime class id for this program
    emitter.instruction("mov QWORD PTR [rax], r10");                            // store class id at the object header
    abi::emit_symbol_address(emitter, "r10", message_symbol);                   // materialize static ValueError message pointer
    abi::emit_store_to_address(emitter, "r10", "rax", throwable_storage::MESSAGE_POINTER);
    emitter.instruction(&format!("mov QWORD PTR [rax + {}], {}", throwable_storage::MESSAGE_LENGTH, message_len)); // store static ValueError message length
    throwable_storage::emit_default_fields(emitter, "rax");
    abi::emit_store_reg_to_symbol(emitter, "rax", "_exc_value", 0);             // publish the active exception object
    emitter.instruction("mov rsp, rbp");                                        // release helper frame before throwing
    emitter.instruction("pop rbp");                                             // restore caller frame pointer before throwing
    emitter.instruction("jmp __rt_throw_current");                              // enter the standard exception unwinder
}
