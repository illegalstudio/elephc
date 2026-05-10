//! Purpose:
//! Lowers shared call-argument coercion, push, and by-reference helpers.
//! Converts evaluated PHP argument expressions into temporary values ready for ABI assignment.
//!
//! Called from:
//! - `crate::codegen::expr::calls::args`
//!
//! Key details:
//! - Argument checks must happen at PHP-observable points without skipping later side effects.

use crate::codegen::emit::Emitter;
use crate::codegen::{abi, context::Context, data_section::DataSection};
use crate::parser::ast::Expr;
use crate::types::{FunctionSig, PhpType};

pub(crate) fn declared_target_ty<'a>(
    sig: Option<&'a FunctionSig>,
    param_idx: usize,
) -> Option<&'a PhpType> {
    sig.and_then(|sig| {
        let target_ty = sig.params.get(param_idx).map(|(_, ty)| ty)?;
        if sig
            .declared_params
            .get(param_idx)
            .copied()
            .unwrap_or(false)
            || matches!(target_ty.codegen_repr(), PhpType::Mixed)
        {
            Some(target_ty)
        } else {
            None
        }
    })
}

pub(crate) fn push_arg_value(emitter: &mut Emitter, ty: &PhpType) {
    abi::emit_push_result_value(emitter, ty);
}

pub(crate) fn emit_ref_arg_variable_address(
    var_name: &str,
    context_label: &str,
    emitter: &mut Emitter,
    ctx: &Context,
) -> bool {
    if ctx.global_vars.contains(var_name) {
        let label = format!("_gvar_{}", var_name);
        emitter.comment(&format!("{}: address of global ${}", context_label, var_name));
        abi::emit_symbol_address(emitter, abi::int_result_reg(emitter), &label);
        true
    } else if ctx.ref_params.contains(var_name) {
        let Some(var) = ctx.variables.get(var_name) else {
            emitter.comment(&format!("WARNING: undefined ref variable ${}", var_name));
            return false;
        };
        emitter.comment(&format!(
            "{}: forward underlying reference for ${}",
            context_label, var_name
        ));
        abi::load_at_offset(emitter, abi::int_result_reg(emitter), var.stack_offset); // load the existing by-reference pointer from the current frame slot
        true
    } else {
        let Some(var) = ctx.variables.get(var_name) else {
            emitter.comment(&format!("WARNING: undefined variable ${}", var_name));
            return false;
        };
        emitter.comment(&format!("{}: address of ${}", context_label, var_name));
        abi::emit_frame_slot_address(emitter, abi::int_result_reg(emitter), var.stack_offset); // compute the local variable's frame-slot address through the ABI helper
        true
    }
}

pub(crate) fn coerce_current_value_to_target(
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    source_ty: &PhpType,
    target_ty: Option<&PhpType>,
) -> (PhpType, bool) {
    let source_repr = source_ty.codegen_repr();
    let pushed_ty = target_ty
        .map(PhpType::codegen_repr)
        .or_else(|| {
            if matches!(source_repr, PhpType::Void) {
                Some(PhpType::Int)
            } else {
                None
            }
        })
        .unwrap_or_else(|| source_repr.clone());
    let boxed_to_mixed = matches!(pushed_ty, PhpType::Mixed) && !matches!(source_repr, PhpType::Mixed);

    if source_repr != pushed_ty {
        let coerce_source_ty = if matches!(pushed_ty, PhpType::Mixed) {
            source_ty
        } else {
            &source_repr
        };
        super::super::super::coerce_result_to_type(emitter, ctx, data, coerce_source_ty, &pushed_ty);
    }

    (pushed_ty, boxed_to_mixed)
}

pub(crate) fn push_expr_arg(
    arg: &Expr,
    target_ty: Option<&PhpType>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let source_ty = super::super::super::emit_expr(arg, emitter, ctx, data);
    let (pushed_ty, boxed_to_mixed) =
        coerce_current_value_to_target(emitter, ctx, data, &source_ty, target_ty);
    if !boxed_to_mixed {
        super::super::super::retain_borrowed_heap_arg(emitter, arg, &source_ty);
    }
    push_arg_value(emitter, &pushed_ty);
    pushed_ty
}
