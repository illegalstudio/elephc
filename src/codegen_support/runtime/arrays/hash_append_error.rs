//! Purpose:
//! Raises PHP's catchable Error when an associative array has exhausted automatic keys.
//!
//! Called from:
//! - Checked hash index lookup and native hash append after retiring an uninserted value.
//!
//! Key details:
//! - No array or value owner is transferred to this nonreturning entry.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch, sentinels};

/// Constructs the standard Error object and enters the runtime unwinder on every supported target.
pub(super) fn emit(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    emitter.label_global("__rt_hash_append_error");
    emitter.instruction(if arm { "stp x29, x30, [sp, #-16]!" } else { "sub rsp, 8" }); // preserve linkage and align exception allocation
    emitter.instruction(if arm { "mov x0, #56" } else { "mov eax, 56" });       // request the compact Throwable payload
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    if arm {
        emitter.instruction("mov x9, #6");                                      // stamp a runtime object owner
        emitter.instruction("str x9, [x0, #-8]");                               // publish the heap kind before acquiring an object handle
    } else {
        abi::emit_load_int_immediate(emitter, "r10", sentinels::x86_64_heap_kind_word(6) as i64);
        emitter.instruction("mov QWORD PTR [rax - 8], r10");                    // publish the canonical object heap kind
    }
    abi::emit_call_label(emitter, "__rt_object_handle_acquire");
    abi::emit_load_symbol_to_reg(emitter, if arm { "x9" } else { "r10" }, "_spl_error_class_id", 0);
    emitter.instruction(if arm { "str x9, [x0]" } else { "mov QWORD PTR [rax], r10" }); // identify the catchable PHP Error class
    abi::emit_symbol_address(emitter, if arm { "x9" } else { "r10" }, "_hash_append_err_msg");
    emitter.instruction(if arm { "str x9, [x0, #8]" } else { "mov QWORD PTR [rax + 8], r10" }); // borrow the immutable diagnostic bytes
    let len = super::super::data::HASH_APPEND_ERROR_MSG.len();
    if arm {
        emitter.instruction(&format!("mov x9, #{len}"));                        // preserve the exact PHP diagnostic length
        emitter.instruction("str x9, [x0, #16]");                               // publish message length beside its pointer
        emitter.instruction("str xzr, [x0, #24]");                              // default the exception code to zero
        emitter.instruction("str xzr, [x0, #40]");                              // initialize the previous exception to null
    } else {
        emitter.instruction(&format!("mov QWORD PTR [rax + 16], {len}"));       // publish the exact PHP diagnostic length
        emitter.instruction("mov QWORD PTR [rax + 24], 0");                     // default the exception code to zero
        emitter.instruction("mov QWORD PTR [rax + 40], 0");                     // initialize the previous exception to null
    }
    sentinels::emit_throwable_creation_line_unknown(emitter, if arm { "x0" } else { "rax" });
    abi::emit_store_reg_to_symbol(emitter, if arm { "x0" } else { "rax" }, "_exc_value", 0);
    emitter.instruction(if arm { "ldp x29, x30, [sp], #16" } else { "add rsp, 8" }); // retire local linkage before entering the unwinder
    emitter.instruction(if arm { "b __rt_throw_current" } else { "jmp __rt_throw_current" }); // propagate Error to the nearest PHP handler
}
