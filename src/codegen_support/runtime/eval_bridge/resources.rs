//! Purpose:
//! Exposes the shared resource inventory as an owned eval-compatible Mixed snapshot.
//!
//! Called from:
//! - `super::emit_eval_bridge_runtime()` when Magician support is linked.
//!
//! Key details:
//! - The filter selector follows __rt_get_resources; resource entries retain canonical cells.
//! - The wrapper adopts the returned hash instead of introducing another owner.

use super::*;
use crate::codegen_support::emit_box_current_owned_value_as_mixed;
use crate::types::PhpType;

/// Adapts the native inventory's C ABI result into the boxed-cell ABI used by eval.
pub(super) fn emit_resource_inventory_wrapper(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_resource_inventory");
    abi::emit_frame_prologue(emitter, 16);
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rax, rdi");                                    // resource inventory consumes its selector in the result register
    }
    abi::emit_call_label(emitter, "__rt_get_resources");
    emit_box_current_owned_value_as_mixed(emitter, &PhpType::AssocArray {
        key: Box::new(PhpType::Int), value: Box::new(PhpType::Mixed),
    });
    abi::emit_frame_restore(emitter, 16);
    emitter.instruction("ret");                                                 // transfer ownership of the boxed inventory to Magician
}
