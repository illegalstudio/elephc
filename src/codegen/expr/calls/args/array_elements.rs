//! Purpose:
//! Lowers argument values sourced from spread array elements.
//! Converts evaluated PHP argument expressions into temporary values ready for ABI assignment.
//!
//! Called from:
//! - `crate::codegen::expr::calls::args`
//!
//! Key details:
//! - Argument checks must happen at PHP-observable points without skipping later side effects.

use crate::codegen::emit::Emitter;
use crate::codegen::{abi, context::Context, data_section::DataSection};
use crate::types::PhpType;

use super::common::{
    coerce_current_value_to_target, push_arg_value, push_current_result_ref_arg_address,
    release_preserved_mixed_after_arg_coercion,
};

pub(crate) fn load_array_element_to_result(
    emitter: &mut Emitter,
    source_elem_ty: &PhpType,
    data_base_reg: &str,
    byte_offset: usize,
) {
    match source_elem_ty.codegen_repr() {
        PhpType::Float => {
            abi::emit_load_from_address(emitter, abi::float_result_reg(emitter), data_base_reg, byte_offset); // load float element from the spread/callback array payload
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_load_from_address(emitter, ptr_reg, data_base_reg, byte_offset); // load string pointer from the spread/callback array payload
            abi::emit_load_from_address(emitter, len_reg, data_base_reg, byte_offset + 8); // load string length from the spread/callback array payload
        }
        PhpType::Void => {}
        _ => {
            abi::emit_load_from_address(emitter, abi::int_result_reg(emitter), data_base_reg, byte_offset); // load scalar or boxed pointer element from the spread/callback array payload
        }
    }
}

pub(crate) fn array_element_stride(source_elem_ty: &PhpType) -> usize {
    match source_elem_ty.codegen_repr() {
        PhpType::Str => 16,
        PhpType::Void => 0,
        _ => 8,
    }
}

pub(crate) fn push_loaded_array_element_arg(
    source_elem_ty: &PhpType,
    target_ty: Option<&PhpType>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let source_repr = source_elem_ty.codegen_repr();
    let (pushed_ty, boxed_to_mixed) =
        coerce_current_value_to_target(emitter, ctx, data, source_elem_ty, target_ty);
    if !boxed_to_mixed {
        abi::emit_incref_if_refcounted(emitter, &source_repr);
    }
    push_arg_value(emitter, &pushed_ty);
    pushed_ty
}

pub(crate) fn emit_hash_lookup_for_param_or_index(
    hash_base_reg: &str,
    param_name: Option<&str>,
    numeric_idx: usize,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let key_ptr_reg = abi::int_arg_reg_name(emitter.target, 1);
    let key_len_reg = abi::int_arg_reg_name(emitter.target, 2);
    let found_label = param_name.map(|_| ctx.next_label("assoc_spread_key_found"));

    if let Some(name) = param_name {
        let (key_label, key_len) = data.add_string(name.as_bytes());
        match emitter.target.arch {
            crate::codegen::platform::Arch::AArch64 => {
                emitter.instruction(&format!("mov x0, {}", hash_base_reg));     // pass the associative spread hash to the named-key lookup
                abi::emit_symbol_address(emitter, key_ptr_reg, &key_label);
                abi::emit_load_int_immediate(emitter, key_len_reg, key_len as i64);
            }
            crate::codegen::platform::Arch::X86_64 => {
                emitter.instruction(&format!("mov rdi, {}", hash_base_reg));    // pass the associative spread hash to the named-key lookup
                abi::emit_symbol_address(emitter, key_ptr_reg, &key_label);
                abi::emit_load_int_immediate(emitter, key_len_reg, key_len as i64);
            }
        }
        abi::emit_call_label(emitter, "__rt_hash_get");
        if let Some(found_label) = &found_label {
            abi::emit_branch_if_int_result_nonzero(emitter, found_label);
        }
    }

    match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            emitter.instruction(&format!("mov x0, {}", hash_base_reg));         // pass the associative spread hash to the numeric-key lookup
        }
        crate::codegen::platform::Arch::X86_64 => {
            emitter.instruction(&format!("mov rdi, {}", hash_base_reg));        // pass the associative spread hash to the numeric-key lookup
        }
    }
    abi::emit_load_int_immediate(emitter, key_ptr_reg, numeric_idx as i64);
    abi::emit_load_int_immediate(emitter, key_len_reg, -1);
    abi::emit_call_label(emitter, "__rt_hash_get");

    if let Some(found_label) = found_label {
        emitter.label(&found_label);
    }
}

pub(crate) fn push_loaded_hash_value_arg(
    source_elem_ty: &PhpType,
    target_ty: Option<&PhpType>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    materialize_hash_value_to_result(emitter, source_elem_ty);
    if !matches!(source_elem_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        return push_loaded_array_element_arg(source_elem_ty, target_ty, emitter, ctx, data);
    }

    let source_repr = source_elem_ty.codegen_repr();
    let release_mixed_after_coerce = target_ty.is_some_and(|target_ty| {
        !matches!(target_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
            && super::super::super::can_coerce_result_to_type(&source_repr, target_ty)
    });
    if release_mixed_after_coerce {
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));              // preserve the boxed hash payload while coercing it for the call
    }
    let (pushed_ty, _boxed_to_mixed) =
        coerce_current_value_to_target(emitter, ctx, data, &source_repr, target_ty);
    if release_mixed_after_coerce {
        release_preserved_mixed_after_arg_coercion(emitter, &pushed_ty);
    }
    push_arg_value(emitter, &pushed_ty);
    pushed_ty
}

pub(crate) fn push_loaded_hash_value_ref_arg(
    source_elem_ty: &PhpType,
    target_ty: Option<&PhpType>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    materialize_hash_value_to_result(emitter, source_elem_ty);
    push_current_result_ref_arg_address(source_elem_ty, target_ty, emitter, ctx, data)
}

fn materialize_hash_value_to_result(emitter: &mut Emitter, source_elem_ty: &PhpType) {
    match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => match source_elem_ty.codegen_repr() {
            PhpType::Int | PhpType::Bool => {
                emitter.instruction("mov x0, x1");                              // move the hash scalar payload into the standard result register
            }
            PhpType::Str => {}
            PhpType::Float => {
                emitter.instruction("fmov d0, x1");                             // move the hash float bits into the standard result register
            }
            PhpType::Mixed | PhpType::Union(_) => {
                crate::codegen::emit_box_runtime_payload_as_mixed(emitter, "x3", "x1", "x2");
            }
            _ => {
                emitter.instruction("mov x0, x1");                              // move the hash pointer payload into the standard result register
            }
        },
        crate::codegen::platform::Arch::X86_64 => match source_elem_ty.codegen_repr() {
            PhpType::Int | PhpType::Bool => {
                emitter.instruction("mov rax, rdi");                            // move the hash scalar payload into the standard result register
            }
            PhpType::Str => {
                emitter.instruction("mov rax, rdi");                            // move the hash string pointer into the standard result register
                emitter.instruction("mov rdx, rsi");                            // move the hash string length into the paired result register
            }
            PhpType::Float => {
                emitter.instruction("movq xmm0, rdi");                          // move the hash float bits into the standard result register
            }
            PhpType::Mixed | PhpType::Union(_) => {
                crate::codegen::emit_box_runtime_payload_as_mixed(emitter, "rcx", "rdi", "rsi");
            }
            _ => {
                emitter.instruction("mov rax, rdi");                            // move the hash pointer payload into the standard result register
            }
        },
    }
}

pub(super) fn spread_source_elem_ty(spread_ty: &PhpType) -> PhpType {
    match spread_ty {
        PhpType::Array(elem) => (**elem).clone(),
        PhpType::AssocArray { value, .. } => (**value).clone(),
        _ => PhpType::Int,
    }
}
