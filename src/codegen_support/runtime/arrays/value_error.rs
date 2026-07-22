//! Purpose:
//! Emits shared runtime throw sequences for array helpers that raise `ValueError`.
//! Keeps exception object allocation details out of individual array helper loops.
//!
//! Called from:
//! - `crate::codegen_support::runtime::arrays::array_filter`.
//! - `crate::codegen_support::runtime::arrays::array_filter_refcounted`.
//! - `crate::codegen_support::runtime::strings::mb_strlen`.
//!
//! Key details:
//! - The emitted sequence does not return; it publishes `_exc_value` and enters the unwinder.

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
    emitter.instruction("mov x0, #56");                                         // request Throwable payload storage (message/code/previous)
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the ValueError object payload
    emitter.instruction("mov x9, #6");                                          // heap kind 6 = object instance
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp allocation as a runtime object
    abi::emit_symbol_address(emitter, "x9", "_spl_value_error_class_id");
    emitter.instruction("ldr x9, [x9]");                                        // load ValueError's runtime class id for this program
    emitter.instruction("str x9, [x0]");                                        // store class id at the object header
    abi::emit_symbol_address(emitter, "x9", message_symbol);
    emitter.instruction("str x9, [x0, #8]");                                    // store static ValueError message pointer
    emitter.instruction(&format!("mov x9, #{}", message_len));                  // load static ValueError message length
    emitter.instruction("str x9, [x0, #16]");                                   // store exception message length
    emitter.instruction("str xzr, [x0, #24]");                                  // exception code defaults to zero
    emitter.instruction("str xzr, [x0, #40]");                                  // previous defaults to null
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
    emitter.instruction("push rbp");                                            // preserve caller frame pointer for exception allocation
    emitter.instruction("mov rbp, rsp");                                        // establish aligned helper frame
    emitter.instruction("sub rsp, 16");                                         // keep the nested heap allocation call 16-byte aligned
    emitter.instruction("mov rax, 56");                                         // request Throwable payload storage (message/code/previous)
    emitter.instruction("call __rt_heap_alloc");                                // allocate the ValueError object payload
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(6))); // stamp the canonical x86_64 heap-kind word (magic + kind 6 throwable)
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp allocation as a runtime object
    abi::emit_load_symbol_to_reg(emitter, "r10", "_spl_value_error_class_id", 0); // load ValueError's runtime class id for this program
    emitter.instruction("mov QWORD PTR [rax], r10");                            // store class id at the object header
    abi::emit_symbol_address(emitter, "r10", message_symbol);                   // materialize static ValueError message pointer
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // store static ValueError message pointer
    emitter.instruction(&format!("mov QWORD PTR [rax + 16], {}", message_len)); // store static ValueError message length
    emitter.instruction("mov QWORD PTR [rax + 24], 0");                         // exception code defaults to zero
    emitter.instruction("mov QWORD PTR [rax + 40], 0");                         // previous defaults to null
    abi::emit_store_reg_to_symbol(emitter, "rax", "_exc_value", 0);             // publish the active exception object
    emitter.instruction("mov rsp, rbp");                                        // release helper frame before throwing
    emitter.instruction("pop rbp");                                             // restore caller frame pointer before throwing
    emitter.instruction("jmp __rt_throw_current");                              // enter the standard exception unwinder
}
