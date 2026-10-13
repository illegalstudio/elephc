//! Purpose:
//! Normalizes boxed descriptor array arguments into their callee's Mixed element storage.
//!
//! Called from:
//! - `super::coerce_result_to_type()` for Mixed-to-container argument binding.
//!
//! Key details:
//! - Retains a callee lease before COW conversion, preserving the borrowed source box's owner.
//! - Returns one owned parameter value; callers must not retain it a second time.
//! - Hash conversions preserve numeric/string keys instead of extracting array_values.

use super::*;

/// Identifies container parameter layouts whose stored element representation must become Mixed.
pub(super) fn needs_mixed_slot_normalization(target: &PhpType) -> bool {
    match target {
        PhpType::Array(element) => element.codegen_repr() == PhpType::Mixed,
        PhpType::AssocArray { value, .. } => value.codegen_repr() == PhpType::Mixed,
        _ => false,
    }
}

/// Acquires and normalizes a container lease while preserving Hash keys and the borrowed source owner.
/// A Hash passed to an indexed-only array ABI is preserved, not silently replaced with array_values.
pub(super) fn normalize_owned_mixed_container(emitter: &mut Emitter, ctx: &mut InvokerEmitContext, target: &PhpType) {
    let result = abi::int_result_reg(emitter);
    let tag = abi::secondary_scratch_reg(emitter);
    let payload = if emitter.target.arch == Arch::AArch64 { "x1" } else { "rdi" };
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    abi::emit_push_reg_pair(emitter, result, payload);
    abi::emit_reg_move(emitter, result, payload);
    abi::emit_incref_if_refcounted(emitter, &PhpType::Iterable);
    abi::emit_pop_reg_pair(emitter, tag, result);
    let indexed = ctx.next_label("parameter_container_indexed");
    let done = ctx.next_label("parameter_container_normalized");
    let scratch = abi::symbol_scratch_reg(emitter);
    abi::emit_load_int_immediate(emitter, scratch, 4);
    emitter.instruction(&format!("cmp {tag}, {scratch}"));                      // choose conversion from the actual boxed PHP-array shape
    emitter.instruction(&format!("{} {indexed}", if emitter.target.arch == Arch::AArch64 { "b.eq" } else { "je" })); // never read indexed metadata from a Hash
    if matches!(target, PhpType::AssocArray { .. }) {
        if emitter.target.arch == Arch::X86_64 { abi::emit_reg_move(emitter, "rdi", result); }
        abi::emit_call_label(emitter, "__rt_hash_to_mixed");
    }
    // Passing a Hash to an indexed-only generic `array` ABI remains an open shape-contract
    // problem. Preserve its keys here; never apply indexed-header conversion to Hash storage.
    abi::emit_jump(emitter, &done);
    emitter.label(&indexed);
    let current_tag = abi::int_arg_reg_name(emitter.target, 1);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("ldr {current_tag}, [{result}, #-8]")), // inspect the actual indexed element metadata before widening slots
        Arch::X86_64 => {
            abi::emit_reg_move(emitter, "rdi", result);
            emitter.instruction(&format!("mov {current_tag}, QWORD PTR [{result} - 8]")); // inspect the actual indexed element metadata before widening slots
        }
    }
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("lsr {current_tag}, {current_tag}, #8")); // move the element tag into its helper ABI position
            emitter.instruction(&format!("and {current_tag}, {current_tag}, #0x7f")); // exclude COW/GC flags from the element tag
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("shr {current_tag}, 8"));              // move the element tag into its helper ABI position
            emitter.instruction(&format!("and {current_tag}, 0x7f"));           // exclude COW/GC flags from the element tag
        }
    }
    abi::emit_call_label(emitter, "__rt_array_to_mixed");
    if matches!(target, PhpType::AssocArray { .. }) {
        abi::emit_push_reg(emitter, result);
        if emitter.target.arch == Arch::X86_64 { abi::emit_reg_move(emitter, "rdi", result); }
        abi::emit_call_label(emitter, "__rt_array_to_hash");
        abi::emit_push_reg(emitter, result);
        abi::emit_load_temporary_stack_slot(emitter, result, 16);
        abi::emit_call_label(emitter, "__rt_decref_array");
        abi::emit_pop_reg(emitter, result);
        abi::emit_release_temporary_stack(emitter, 16);
    }
    emitter.label(&done);
}
