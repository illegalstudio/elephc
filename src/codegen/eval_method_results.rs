//! Purpose:
//! Boxes native method results for the eval ABI, including borrowed reference-cell payloads.
//!
//! Called from:
//! - Native method dispatch and property-get bridge emitters.
//!
//! Key details:
//! - Reference cells are dereferenced without consuming their owner.
//! - Array adoption requires an explicit callee ownership proof and preserves hash tags.

use crate::codegen::{abi, emit_box_current_value_as_mixed, emit_box_current_owned_value_as_mixed};
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::ir::Module;
use crate::types::PhpType;

/// Boxes the current native result, transferring only proven owners.
pub(super) fn emit_box_method_result(
    module: &Module, emitter: &mut Emitter, return_ty: &PhpType, owns_object: bool, owns_array: bool, by_ref_return: bool,
) {
    if by_ref_return {
        emit_box_reference_method_result(emitter, return_ty);
        return;
    }
    if return_ty.codegen_repr() == PhpType::Void {
        let null_symbol = module.target.extern_symbol("__elephc_eval_value_null");
        abi::emit_call_label(emitter, &null_symbol);
    } else if return_ty.codegen_repr() == PhpType::Str || owns_object {
        // Native string returns are persisted owners, as at the callable boundary.
        // Raw objects require the selected implementation's EIR ownership proof.
        emit_box_current_owned_value_as_mixed(emitter, return_ty);
    } else if matches!(return_ty.codegen_repr(), PhpType::Array(_)) {
        // A PHP `array` return declaration does not distinguish indexed arrays
        // from associative hashes. Probe the concrete heap kind at this eval
        // boundary so hash-returning methods are not mislabeled as tag 4.
        if owns_array {
            emit_box_current_owned_value_as_mixed(emitter, &PhpType::Iterable);
        } else {
            emit_box_current_value_as_mixed(emitter, &PhpType::Iterable);
        }
    } else if owns_array {
        emit_box_current_owned_value_as_mixed(emitter, return_ty);
    } else {
        emit_box_current_value_as_mixed(emitter, return_ty);
    }
}

/// Copies the borrowed payload of a returned reference cell into an owned Mixed value.
pub(super) fn emit_box_reference_method_result(emitter: &mut Emitter, ty: &PhpType) {
    let ty = ty.codegen_repr();
    if matches!(ty, PhpType::Void | PhpType::Never) {
        let symbol = emitter.target.extern_symbol("__elephc_eval_value_null");
        abi::emit_call_label(emitter, &symbol);
        return;
    }
    let pointer = abi::symbol_scratch_reg(emitter);
    abi::emit_reg_move(emitter, pointer, abi::int_result_reg(emitter));
    match &ty {
        PhpType::Str => {
            let (ptr, len) = abi::string_result_regs(emitter);
            abi::emit_load_from_address(emitter, ptr, pointer, 0);
            abi::emit_load_from_address(emitter, len, pointer, 8);
        }
        PhpType::Float => {
            abi::emit_load_from_address(emitter, abi::float_result_reg(emitter), pointer, 0);
        }
        PhpType::TaggedScalar => {
            abi::emit_load_from_address(emitter, abi::int_result_reg(emitter), pointer, 0);
            abi::emit_load_from_address(emitter,
                crate::codegen::sentinels::tagged_scalar_tag_reg(emitter), pointer, 8);
        }
        _ => abi::emit_load_from_address(emitter, abi::int_result_reg(emitter), pointer, 0),
    }
    if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_call_label(emitter, "__rt_mixed_unbox");
        if emitter.target.arch == Arch::X86_64 {
            abi::emit_reg_move(emitter, "rsi", "rdx");
        }
        abi::emit_call_label(emitter, "__rt_mixed_from_value");
    } else if matches!(ty, PhpType::Array(_)) {
        emit_box_current_value_as_mixed(emitter, &PhpType::Iterable);
    } else {
        emit_box_current_value_as_mixed(emitter, &ty);
    }
}
