//! Purpose:
//! Raises PHP's catchable Error when an associative array has exhausted automatic keys.
//!
//! Called from:
//! - Checked hash index lookup and native hash append after retiring an uninserted value.
//!
//! Key details:
//! - No array or value owner is transferred to this nonreturning entry.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use elephc_memoir::values::throwable_storage;

/// Constructs the standard Error object and enters the runtime unwinder on every supported target.
pub(super) fn emit(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    emitter.label_global("__rt_hash_append_error");
    emitter.instruction(if arm { "stp x29, x30, [sp, #-16]!" } else { "sub rsp, 8" }); // preserve linkage and align exception allocation
    throwable_storage::emit_allocate_cold(emitter, "eax");
    throwable_storage::emit_class_id(emitter, "_spl_error_class_id");
    let len = super::super::data::HASH_APPEND_ERROR_MSG.len() as i64;
    throwable_storage::emit_static_message(emitter, "_hash_append_err_msg", len, throwable_storage::LengthStore::Immediate);
    let payload = abi::int_result_reg(emitter);
    throwable_storage::emit_default_code(emitter, payload);
    throwable_storage::emit_empty_previous(emitter, payload);
    throwable_storage::emit_throwable_creation_line_unknown(emitter, payload);
    abi::emit_store_reg_to_symbol(emitter, if arm { "x0" } else { "rax" }, "_exc_value", 0);
    emitter.instruction(if arm { "ldp x29, x30, [sp], #16" } else { "add rsp, 8" }); // retire local linkage before entering the unwinder
    emitter.instruction(if arm { "b __rt_throw_current" } else { "jmp __rt_throw_current" }); // propagate Error to the nearest PHP handler
}
