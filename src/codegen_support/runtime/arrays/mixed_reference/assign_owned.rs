//! Purpose:
//! Emit assignment of an owned runtime value through a persistent Mixed reference wrapper.
//!
//! Called from:
//! - `super::emit_mixed_reference` and the persistent-reference path of hash_set.
//!
//! Key details:
//! - Input is reference in the result register, followed by low/high/tag value words.
//! - The new child is published before releasing the old child; no temporary owner survives
//!   the potentially throwing old-value destructor.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Consumes one owned value tuple and updates the shared reference without detaching its aliases.
pub(super) fn emit(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let base = abi::secondary_scratch_reg(emitter);
    let temporary = abi::symbol_scratch_reg(emitter);
    let (low, high, tag, owned_box) = match emitter.target.arch {
        Arch::AArch64 => ("x1", "x2", "x3", "__rt_hash_to_mixed_box_owned"),
        Arch::X86_64 => ("rdi", "rsi", "rdx", "__rt_hash_to_mixed_x86_box_owned"),
    };
    emitter.blank();
    emitter.label_global("__rt_reference_assign_owned");
    abi::emit_frame_prologue(emitter, 64);
    abi::store_at_offset(emitter, result, 8);
    abi::store_at_offset(emitter, low, 16);
    abi::store_at_offset(emitter, high, 24);
    abi::store_at_offset(emitter, tag, 32);
    abi::load_at_offset(emitter, result, 32);
    abi::load_at_offset(emitter, low, 16);
    abi::load_at_offset(emitter, high, 24);
    // Mixed tuples already transfer a complete owned box. Keep that box installed:
    // write-context fetches hold a borrowed pointer to it until hash_set returns.
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #7");                                  // distinguish a complete Mixed owner from a raw payload tuple
            emitter.instruction("b.ne __rt_reference_assign_box_tuple");        // raw payloads still need their owning Mixed wrapper
            emitter.instruction("mov x0, x1");                                  // transfer the existing boxed owner without freeing its caller-visible cell
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 7");                                  // distinguish a complete Mixed owner from a raw payload tuple
            emitter.instruction("jne __rt_reference_assign_box_tuple");         // raw payloads still need their owning Mixed wrapper
            emitter.instruction("mov rax, rdi");                                // transfer the existing boxed owner without freeing its caller-visible cell
        }
    }
    abi::emit_jump(emitter, "__rt_reference_assign_box_ready");
    emitter.label("__rt_reference_assign_box_tuple");
    abi::emit_call_label(emitter, owned_box);
    emitter.label("__rt_reference_assign_box_ready");
    abi::store_at_offset(emitter, result, 16);
    abi::emit_call_label(emitter, "__rt_reference_is");
    abi::emit_branch_if_int_result_zero(emitter, "__rt_reference_assign_publish");
    // A value assignment copies the logical value of another reference, never its identity.
    abi::load_at_offset(emitter, result, 16);
    abi::emit_call_label(emitter, "__rt_mixed_clone");
    abi::store_at_offset(emitter, result, 24);
    abi::load_at_offset(emitter, result, 16);
    abi::emit_call_label(emitter, "__rt_decref_any");
    abi::load_at_offset(emitter, result, 24);
    abi::store_at_offset(emitter, result, 16);
    emitter.label("__rt_reference_assign_publish");
    abi::load_at_offset(emitter, base, 8);
    abi::emit_load_from_address(emitter, result, base, 8);
    abi::load_at_offset(emitter, temporary, 16);
    abi::emit_store_to_address(emitter, temporary, base, 8);
    // The replacement is now owned by the reference; an old-value throw cannot leak it.
    abi::emit_call_label(emitter, "__rt_decref_any");
    abi::emit_frame_restore(emitter, 64);
    abi::emit_return(emitter);
}
