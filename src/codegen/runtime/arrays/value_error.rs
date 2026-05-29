//! Purpose:
//! Emits shared runtime throw sequences for array helpers that raise `ValueError`.
//! Keeps exception object allocation details out of individual array helper loops.
//!
//! Called from:
//! - `crate::codegen::runtime::arrays::array_filter`.
//! - `crate::codegen::runtime::arrays::array_filter_refcounted`.
//!
//! Key details:
//! - The emitted sequence does not return; it publishes `_exc_value` and enters the unwinder.

use crate::codegen::abi;
use crate::codegen::emit::Emitter;

/// Emits an ARM64 `ValueError` throw using a static message symbol.
///
/// Allocates a 32-byte Throwable payload, stamps the per-program `ValueError`
/// class id, stores the message pointer/length and zero code, then jumps to
/// `__rt_throw_current`.
pub(super) fn emit_throw_value_error_aarch64(
    emitter: &mut Emitter,
    message_symbol: &str,
    message_len: usize,
) {
    emitter.instruction("mov x0, #32");                                         // request Throwable payload storage
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
    abi::emit_symbol_address(emitter, "x9", "_exc_value");
    emitter.instruction("str x0, [x9]");                                        // publish the active exception object
    emitter.instruction("b __rt_throw_current");                                // enter the standard exception unwinder
}

/// Emits an x86_64 Linux `ValueError` throw using a static message symbol.
///
/// Preserves `rbp`, aligns the nested allocation call, writes the standard
/// Throwable payload layout, and jumps to `__rt_throw_current` without returning.
pub(super) fn emit_throw_value_error_x86_64(
    emitter: &mut Emitter,
    message_symbol: &str,
    message_len: usize,
) {
    emitter.instruction("push rbp");                                            // preserve caller frame pointer for exception allocation
    emitter.instruction("mov rbp, rsp");                                        // establish aligned helper frame
    emitter.instruction("sub rsp, 16");                                         // keep the nested heap allocation call 16-byte aligned
    emitter.instruction("mov rax, 32");                                         // request Throwable payload storage
    emitter.instruction("call __rt_heap_alloc");                                // allocate the ValueError object payload
    emitter.instruction("mov r10, 0x4548504c00000006");                         // x86_64 heap-kind word: HE LP magic + kind 6 object
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp allocation as a runtime object
    emitter.instruction("mov r10, QWORD PTR [rip + _spl_value_error_class_id]"); // load ValueError's runtime class id for this program
    emitter.instruction("mov QWORD PTR [rax], r10");                            // store class id at the object header
    emitter.instruction(&format!("lea r10, [rip + {}]", message_symbol));       // materialize static ValueError message pointer
    emitter.instruction("mov QWORD PTR [rax + 8], r10");                        // store static ValueError message pointer
    emitter.instruction(&format!("mov QWORD PTR [rax + 16], {}", message_len)); // store static ValueError message length
    emitter.instruction("mov QWORD PTR [rax + 24], 0");                         // exception code defaults to zero
    emitter.instruction("mov QWORD PTR [rip + _exc_value], rax");               // publish the active exception object
    emitter.instruction("mov rsp, rbp");                                        // release helper frame before throwing
    emitter.instruction("pop rbp");                                             // restore caller frame pointer before throwing
    emitter.instruction("jmp __rt_throw_current");                              // enter the standard exception unwinder
}
