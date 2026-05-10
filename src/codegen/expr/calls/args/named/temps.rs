//! Purpose:
//! Lowers hidden temporary slots for named-argument preevaluation.
//! Works with the shared call-argument plan to preserve PHP named-argument semantics.
//!
//! Called from:
//! - `crate::codegen::expr::calls::args::named`
//!
//! Key details:
//! - Side effects occur in source order, while final argument materialization follows parameter and ABI order.

use crate::codegen::emit::Emitter;
use crate::codegen::{abi, context::Context, data_section::DataSection};
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{FunctionSig, PhpType};

use super::super::{
    declared_target_ty, emit_ref_arg_variable_address, push_arg_value, push_expr_arg,
};

pub(super) fn push_source_temp_type(source_temp_types: &mut Vec<PhpType>, ty: PhpType) -> usize {
    let idx = source_temp_types.len();
    source_temp_types.push(ty);
    idx
}

#[allow(clippy::too_many_arguments)]
pub(super) fn emit_source_temp_arg(
    arg: &Expr,
    sig: &FunctionSig,
    param_idx: Option<usize>,
    ref_arg_context_label: &str,
    retain_non_variable_ref_args: bool,
    source_temp_types: &mut Vec<PhpType>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> usize {
    let is_ref = param_idx
        .and_then(|idx| sig.ref_params.get(idx))
        .copied()
        .unwrap_or(false);
    let pushed_ty = if is_ref {
        if let ExprKind::Variable(var_name) = &arg.kind {
            emit_ref_arg_variable_address(var_name, ref_arg_context_label, emitter, ctx);
        } else {
            let source_ty = super::super::super::super::emit_expr(arg, emitter, ctx, data);
            if retain_non_variable_ref_args {
                super::super::super::super::retain_borrowed_heap_arg(emitter, arg, &source_ty);
            }
        }
        push_arg_value(emitter, &PhpType::Int);
        PhpType::Int
    } else {
        let target_ty = param_idx.and_then(|idx| declared_target_ty(Some(sig), idx));
        push_expr_arg(arg, target_ty, emitter, ctx, data)
    };
    push_source_temp_type(source_temp_types, pushed_ty)
}

pub(super) fn temp_slot_size(ty: &PhpType) -> usize {
    if matches!(ty, PhpType::Void | PhpType::Never) {
        0
    } else {
        16
    }
}

pub(crate) fn pushed_temp_bytes(types: &[PhpType]) -> usize {
    types.iter().map(temp_slot_size).sum()
}

fn temp_offsets(types: &[PhpType]) -> Vec<usize> {
    let mut offsets = vec![0usize; types.len()];
    let mut running = 0usize;
    for idx in (0..types.len()).rev() {
        offsets[idx] = running;
        running += temp_slot_size(&types[idx]);
    }
    offsets
}

pub(super) fn source_temp_offset(source_temp_types: &[PhpType], temp_idx: usize, extra_bytes: usize) -> usize {
    extra_bytes + temp_offsets(source_temp_types)[temp_idx]
}

pub(super) fn load_source_temp_to_result(
    temp_idx: usize,
    source_temp_types: &[PhpType],
    extra_bytes: usize,
    emitter: &mut Emitter,
) -> PhpType {
    let ty = source_temp_types[temp_idx].clone();
    let offset = source_temp_offset(source_temp_types, temp_idx, extra_bytes);
    match ty.codegen_repr() {
        PhpType::Float => {
            abi::emit_load_temporary_stack_slot(emitter, abi::float_result_reg(emitter), offset);
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_load_temporary_stack_slot(emitter, ptr_reg, offset);
            abi::emit_load_temporary_stack_slot(emitter, len_reg, offset + 8);
        }
        PhpType::Void | PhpType::Never => {}
        _ => {
            abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), offset);
        }
    }
    ty
}

pub(super) fn push_saved_source_temp_arg(
    temp_idx: usize,
    source_temp_types: &[PhpType],
    final_pushed_bytes: usize,
    emitter: &mut Emitter,
) -> PhpType {
    let ty = load_source_temp_to_result(temp_idx, source_temp_types, final_pushed_bytes, emitter);
    push_arg_value(emitter, &ty);
    ty
}
