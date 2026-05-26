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
use crate::codegen::{
    abi,
    context::{Context, HeapOwnership},
    data_section::DataSection,
};
use crate::parser::ast::{BinOp, Expr, ExprKind};
use crate::types::{FunctionSig, PhpType};

/// Returns the declared target PHP type for a parameter, considering explicit type annotations.
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

/// Returns the effective call-target PHP type for a parameter, optionally including inferred types.
pub(crate) fn call_target_ty<'a>(
    sig: Option<&'a FunctionSig>,
    param_idx: usize,
    include_inferred: bool,
) -> Option<&'a PhpType> {
    if include_inferred {
        sig.and_then(|sig| sig.params.get(param_idx).map(|(_, ty)| ty))
    } else {
        declared_target_ty(sig, param_idx)
    }
}

/// Pushes the current value in the result register onto the argument stack for the ABI.
pub(crate) fn push_arg_value(emitter: &mut Emitter, ty: &PhpType) {
    abi::emit_push_result_value(emitter, ty);
}

/// Emits the address of a variable for a by-reference argument and returns whether the variable is valid.
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

/// Coerces the current value to the target PHP type, returning the pushed type and whether boxing occurred.
pub(crate) fn coerce_current_value_to_target(
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    source_ty: &PhpType,
    target_ty: Option<&PhpType>,
) -> (PhpType, bool) {
    let source_repr = source_ty.codegen_repr();
    let pushed_ty = target_ty
        .filter(|target_ty| {
            super::super::super::can_coerce_result_to_type(source_ty, target_ty)
        })
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

/// Evaluates an argument expression, coerces it to the target type, and pushes it as a call argument.
pub(crate) fn push_expr_arg(
    arg: &Expr,
    target_ty: Option<&PhpType>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let source_ty = super::super::super::emit_expr(arg, emitter, ctx, data);
    let release_mixed_after_coerce =
        should_release_owned_mixed_after_arg_coerce(arg, &source_ty, target_ty);
    if release_mixed_after_coerce {
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
    }
    let (pushed_ty, boxed_to_mixed) =
        coerce_current_value_to_target(emitter, ctx, data, &source_ty, target_ty);
    if release_mixed_after_coerce {
        release_preserved_mixed_after_arg_coercion(emitter, &pushed_ty);
    }
    if !boxed_to_mixed && source_ty.codegen_repr() == pushed_ty {
        super::super::super::retain_borrowed_heap_arg(emitter, arg, &source_ty);
    }
    push_arg_value(emitter, &pushed_ty);
    pushed_ty
}

/// Allocates a by-reference cell for a non-variable argument and pushes its address.
pub(crate) fn push_non_variable_ref_arg_address(
    arg: &Expr,
    target_ty: Option<&PhpType>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let pushed_ty = push_expr_arg(arg, target_ty, emitter, ctx, data);
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 16);
    abi::emit_call_label(emitter, "__rt_heap_alloc");                         // allocate a stable 16-byte by-reference cell for a default or temporary argument
    let cell_reg = abi::symbol_scratch_reg(emitter);
    emitter.instruction(&format!("mov {}, {}", cell_reg, abi::int_result_reg(emitter))); // keep the allocated reference cell address while storing the initial value
    store_pushed_value_to_ref_cell(emitter, cell_reg, &pushed_ty);
    abi::emit_push_reg(emitter, cell_reg);
    PhpType::Int
}

/// Pushes current result ref arg address onto the temporary call stack or synthetic metadata list.
pub(crate) fn push_current_result_ref_arg_address(
    source_ty: &PhpType,
    target_ty: Option<&PhpType>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let source_repr = source_ty.codegen_repr();
    let (pushed_ty, boxed_to_mixed) =
        coerce_current_value_to_target(emitter, ctx, data, source_ty, target_ty);
    if !boxed_to_mixed {
        abi::emit_incref_if_refcounted(emitter, &source_repr);
    }
    push_arg_value(emitter, &pushed_ty);
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 16);
    abi::emit_call_label(emitter, "__rt_heap_alloc");                         // allocate a stable 16-byte by-reference cell for a dynamic callback argument
    let cell_reg = abi::symbol_scratch_reg(emitter);
    emitter.instruction(&format!("mov {}, {}", cell_reg, abi::int_result_reg(emitter))); // keep the allocated callback reference cell while storing the loaded argument
    store_pushed_value_to_ref_cell(emitter, cell_reg, &pushed_ty);
    abi::emit_push_reg(emitter, cell_reg);
    PhpType::Int
}

/// Stores the value currently on the ABI result register into a by-reference heap cell.
/// The cell is organized as: [value_pointer, type_tag] with tag values matching PhpType
/// variants (e.g., 4=Array, 6=Object, 7=Mixed/Union/Iterable, 9=Resource).
/// Takes ownership of the value on the result register.
fn store_pushed_value_to_ref_cell(emitter: &mut Emitter, cell_reg: &str, val_ty: &PhpType) {
    let temp_reg = abi::temp_int_reg(emitter.target);
    match val_ty.codegen_repr() {
        PhpType::Bool
        | PhpType::Int
        | PhpType::Callable
        | PhpType::Pointer(_)
        | PhpType::Buffer(_)
        | PhpType::Packed(_) => {
            abi::emit_pop_reg(emitter, temp_reg);
            abi::emit_store_to_address(emitter, temp_reg, cell_reg, 0);
            abi::emit_store_zero_to_address(emitter, cell_reg, 8);
        }
        PhpType::Resource(_) => {
            abi::emit_pop_reg(emitter, temp_reg);
            abi::emit_store_to_address(emitter, temp_reg, cell_reg, 0);
            abi::emit_load_int_immediate(emitter, temp_reg, 9);
            abi::emit_store_to_address(emitter, temp_reg, cell_reg, 8);
        }
        PhpType::Mixed | PhpType::Union(_) | PhpType::Iterable => {
            abi::emit_pop_reg(emitter, temp_reg);
            abi::emit_store_to_address(emitter, temp_reg, cell_reg, 0);
            abi::emit_load_int_immediate(emitter, temp_reg, 7);
            abi::emit_store_to_address(emitter, temp_reg, cell_reg, 8);
        }
        PhpType::Array(_) => {
            abi::emit_pop_reg(emitter, temp_reg);
            abi::emit_store_to_address(emitter, temp_reg, cell_reg, 0);
            abi::emit_load_int_immediate(emitter, temp_reg, 4);
            abi::emit_store_to_address(emitter, temp_reg, cell_reg, 8);
        }
        PhpType::AssocArray { .. } => {
            abi::emit_pop_reg(emitter, temp_reg);
            abi::emit_store_to_address(emitter, temp_reg, cell_reg, 0);
            abi::emit_load_int_immediate(emitter, temp_reg, 5);
            abi::emit_store_to_address(emitter, temp_reg, cell_reg, 8);
        }
        PhpType::Object(_) => {
            abi::emit_pop_reg(emitter, temp_reg);
            abi::emit_store_to_address(emitter, temp_reg, cell_reg, 0);
            abi::emit_load_int_immediate(emitter, temp_reg, 6);
            abi::emit_store_to_address(emitter, temp_reg, cell_reg, 8);
        }
        PhpType::Float => {
            abi::emit_pop_float_reg(emitter, abi::float_result_reg(emitter));
            abi::emit_store_to_address(emitter, abi::float_result_reg(emitter), cell_reg, 0);
            abi::emit_store_zero_to_address(emitter, cell_reg, 8);
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_pop_reg_pair(emitter, ptr_reg, len_reg);
            abi::emit_push_reg(emitter, cell_reg);
            abi::emit_call_label(emitter, "__rt_str_persist");                 // detach temporary string storage before putting it in the reference cell
            abi::emit_pop_reg(emitter, cell_reg);
            abi::emit_store_to_address(emitter, ptr_reg, cell_reg, 0);
            abi::emit_store_to_address(emitter, len_reg, cell_reg, 8);
        }
        PhpType::Void | PhpType::Never => {
            abi::emit_store_zero_to_address(emitter, cell_reg, 0);
            abi::emit_store_zero_to_address(emitter, cell_reg, 8);
        }
    }
}

/// Determines whether an owned Mixed or Union value must be preserved on the temporary
/// stack and released after argument coercion rather than immediately released.
/// Returns true when the source is heap-owned Mixed/Union, the target is a concrete non-Mixed
/// type, and coercion is applicable. Arithmetic binary ops are treated as heap-owned to
/// handle their intermediate results correctly.
fn should_release_owned_mixed_after_arg_coerce(
    arg: &Expr,
    source_ty: &PhpType,
    target_ty: Option<&PhpType>,
) -> bool {
    let source_repr = source_ty.codegen_repr();
    let Some(target_repr) = target_ty.map(PhpType::codegen_repr) else {
        return false;
    };
    matches!(source_repr, PhpType::Mixed | PhpType::Union(_))
        && !matches!(target_repr, PhpType::Mixed | PhpType::Union(_))
        && target_ty.is_some_and(|target_ty| {
            super::super::super::can_coerce_result_to_type(source_ty, target_ty)
        })
        && (super::super::super::expr_result_heap_ownership(arg) == HeapOwnership::Owned
            || matches!(
                arg.kind,
                ExprKind::BinaryOp {
                    op: BinOp::Add | BinOp::Sub | BinOp::Mul,
                    ..
                }
            ))
}

/// Releases a preserved Mixed value after coercion when the target type is not Mixed.
pub(crate) fn release_preserved_mixed_after_arg_coercion(
    emitter: &mut Emitter,
    target_ty: &PhpType,
) {
    match target_ty.codegen_repr() {
        PhpType::Float => {
            abi::emit_push_float_reg(emitter, abi::float_result_reg(emitter));
            abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), 16);
            abi::emit_decref_if_refcounted(emitter, &PhpType::Mixed);
            abi::emit_pop_float_reg(emitter, abi::float_result_reg(emitter));
            abi::emit_release_temporary_stack(emitter, 16);
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_call_label(emitter, "__rt_str_persist");                        // detach string casts from the mixed cell before releasing the boxed owner
            abi::emit_push_reg_pair(emitter, ptr_reg, len_reg);
            abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), 16);
            abi::emit_decref_if_refcounted(emitter, &PhpType::Mixed);
            abi::emit_pop_reg_pair(emitter, ptr_reg, len_reg);
            abi::emit_release_temporary_stack(emitter, 16);
        }
        _ => {
            abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
            abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), 16);
            abi::emit_decref_if_refcounted(emitter, &PhpType::Mixed);
            abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));
            abi::emit_release_temporary_stack(emitter, 16);
        }
    }
}
