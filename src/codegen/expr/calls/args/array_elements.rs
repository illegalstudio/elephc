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

use super::common::{coerce_current_value_to_target, push_arg_value};

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

pub(super) fn spread_source_elem_ty(spread_ty: &PhpType) -> PhpType {
    match spread_ty {
        PhpType::Array(elem) => (**elem).clone(),
        PhpType::AssocArray { value, .. } => (**value).clone(),
        _ => PhpType::Int,
    }
}
