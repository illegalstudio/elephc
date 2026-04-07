use crate::codegen::emit::Emitter;
use crate::codegen::{abi, context::Context, data_section::DataSection};
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{FunctionSig, PhpType};

const MAX_INT_ARG_REGS: usize = 8;
const MAX_FLOAT_ARG_REGS: usize = 8;
const STACK_ARG_SENTINEL: usize = usize::MAX;

pub(crate) fn has_named_args(args: &[Expr]) -> bool {
    args.iter()
        .any(|arg| matches!(arg.kind, ExprKind::NamedArg { .. }))
}

pub(crate) fn normalize_named_call_args(
    sig: &FunctionSig,
    args: &[Expr],
    regular_param_count: usize,
) -> Vec<Expr> {
    if !has_named_args(args) {
        return args.to_vec();
    }

    let mut resolved: Vec<Option<Expr>> = vec![None; regular_param_count];
    let mut variadic_args = Vec::new();
    let mut positional_idx = 0usize;

    for arg in args {
        match &arg.kind {
            ExprKind::NamedArg { name, value } => {
                if let Some(param_idx) = sig
                    .params
                    .iter()
                    .take(regular_param_count)
                    .position(|(param_name, _)| param_name == name)
                {
                    resolved[param_idx] = Some((**value).clone());
                }
            }
            _ => {
                if positional_idx < regular_param_count {
                    resolved[positional_idx] = Some(arg.clone());
                } else {
                    variadic_args.push(arg.clone());
                }
                positional_idx += 1;
            }
        }
    }

    let mut normalized = Vec::new();
    for (idx, slot) in resolved.into_iter().enumerate() {
        if let Some(arg) = slot {
            normalized.push(arg);
        } else if let Some(Some(default_expr)) = sig.defaults.get(idx) {
            normalized.push(default_expr.clone());
        }
    }
    normalized.extend(variadic_args);
    normalized
}

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
    match ty {
        PhpType::Bool
        | PhpType::Int
        | PhpType::Mixed
        | PhpType::Union(_)
        | PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Buffer(_)
        | PhpType::Callable
        | PhpType::Object(_)
        | PhpType::Packed(_)
        | PhpType::Pointer(_) => {
            emitter.instruction("str x0, [sp, #-16]!");                         // push int/bool/array/callable/pointer arg onto stack
        }
        PhpType::Float => {
            emitter.instruction("str d0, [sp, #-16]!");                         // push float arg onto stack
        }
        PhpType::Str => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push string ptr+len arg onto stack
        }
        PhpType::Void => {}
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
        .unwrap_or_else(|| source_repr.clone());
    let boxed_to_mixed = matches!(pushed_ty, PhpType::Mixed) && !matches!(source_repr, PhpType::Mixed);

    if source_repr != pushed_ty {
        super::super::coerce_result_to_type(emitter, ctx, data, &source_repr, &pushed_ty);
    }

    (pushed_ty, boxed_to_mixed)
}

pub(crate) fn push_expr_arg(
    arg: &crate::parser::ast::Expr,
    target_ty: Option<&PhpType>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let source_ty = super::super::emit_expr(arg, emitter, ctx, data);
    let (pushed_ty, boxed_to_mixed) =
        coerce_current_value_to_target(emitter, ctx, data, &source_ty, target_ty);
    if !boxed_to_mixed {
        super::super::retain_borrowed_heap_arg(emitter, arg, &source_ty);
    }
    push_arg_value(emitter, &pushed_ty);
    pushed_ty
}

pub(crate) fn load_array_element_to_result(
    emitter: &mut Emitter,
    source_elem_ty: &PhpType,
    data_base_reg: &str,
    byte_offset: usize,
) {
    match source_elem_ty.codegen_repr() {
        PhpType::Float => {
            emitter.instruction(&format!(                                       // load float element from the spread/callback array payload
                "ldr d0, [{}, #{}]",
                data_base_reg, byte_offset
            ));
        }
        PhpType::Str => {
            emitter.instruction(&format!(                                       // load string pointer from the spread/callback array payload
                "ldr x1, [{}, #{}]",
                data_base_reg, byte_offset
            ));
            emitter.instruction(&format!(                                       // load string length from the spread/callback array payload
                "ldr x2, [{}, #{}]",
                data_base_reg,
                byte_offset + 8
            ));
        }
        PhpType::Void => {}
        _ => {
            emitter.instruction(&format!(                                       // load scalar or boxed pointer element from the spread/callback array payload
                "ldr x0, [{}, #{}]",
                data_base_reg, byte_offset
            ));
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
        coerce_current_value_to_target(emitter, ctx, data, &source_repr, target_ty);
    if !boxed_to_mixed {
        abi::emit_incref_if_refcounted(emitter, &source_repr);
    }
    push_arg_value(emitter, &pushed_ty);
    pushed_ty
}

pub(crate) fn build_arg_assignments(
    arg_types: &[PhpType],
    initial_int_reg_idx: usize,
) -> Vec<(PhpType, usize, bool)> {
    let mut assignments = Vec::new();
    let mut int_reg_idx = initial_int_reg_idx;
    let mut float_reg_idx = 0usize;
    let mut int_stack_only = initial_int_reg_idx >= MAX_INT_ARG_REGS;
    let mut float_stack_only = false;
    for ty in arg_types {
        if ty.is_float_reg() {
            if !float_stack_only && float_reg_idx < MAX_FLOAT_ARG_REGS {
                assignments.push((ty.clone(), float_reg_idx, true));
                float_reg_idx += 1;
            } else {
                assignments.push((ty.clone(), STACK_ARG_SENTINEL, true));
                float_stack_only = true;
            }
        } else {
            let reg_count = ty.register_count();
            if !int_stack_only && int_reg_idx + reg_count <= MAX_INT_ARG_REGS {
                assignments.push((ty.clone(), int_reg_idx, false));
                int_reg_idx += reg_count;
            } else {
                assignments.push((ty.clone(), STACK_ARG_SENTINEL, false));
                int_stack_only = true;
            }
        }
    }
    assignments
}

fn arg_slot_size(ty: &PhpType) -> usize {
    match ty {
        PhpType::Void => 0,
        _ => 16,
    }
}

fn assignment_in_register(start_reg: usize) -> bool {
    start_reg != STACK_ARG_SENTINEL
}

fn emit_adjust_sp(emitter: &mut Emitter, amount: usize, subtract: bool) {
    let mut remaining = amount;
    while remaining > 0 {
        let chunk = remaining.min(4080);
        if subtract {
            emitter.instruction(&format!("sub sp, sp, #{}", chunk));            // reserve stack space for spilled call arguments
        } else {
            emitter.instruction(&format!("add sp, sp, #{}", chunk));            // release temporary call-argument stack space
        }
        remaining -= chunk;
    }
}

fn emit_sp_address(emitter: &mut Emitter, scratch: &str, offset: usize) {
    emitter.instruction(&format!("mov {}, sp", scratch));                       // start from the current stack pointer
    let mut remaining = offset;
    while remaining > 0 {
        let chunk = remaining.min(4080);
        emitter.instruction(&format!("add {}, {}, #{}", scratch, scratch, chunk)); // advance scratch to the desired stack slot
        remaining -= chunk;
    }
}

fn emit_load_from_sp(emitter: &mut Emitter, reg: &str, offset: usize) {
    if offset == 0 {
        emitter.instruction(&format!("ldr {}, [sp]", reg));                     // load directly from the top of the stack
    } else if offset <= 4095 {
        emitter.instruction(&format!("ldr {}, [sp, #{}]", reg, offset));        // load from a nearby stack slot with an immediate offset
    } else {
        emit_sp_address(emitter, "x9", offset);
        emitter.instruction(&format!("ldr {}, [x9]", reg));                     // load from a far stack slot through a scratch address
    }
}

fn emit_store_to_sp(emitter: &mut Emitter, reg: &str, offset: usize) {
    if offset == 0 {
        emitter.instruction(&format!("str {}, [sp]", reg));                     // store directly to the top of the stack
    } else if offset <= 4095 {
        emitter.instruction(&format!("str {}, [sp, #{}]", reg, offset));        // store to a nearby stack slot with an immediate offset
    } else {
        emit_sp_address(emitter, "x9", offset);
        emitter.instruction(&format!("str {}, [x9]", reg));                     // store to a far stack slot through a scratch address
    }
}

fn emit_copy_stack_arg_slot(emitter: &mut Emitter, ty: &PhpType, src_offset: usize, dst_offset: usize) {
    match ty {
        PhpType::Float => {
            emit_load_from_sp(emitter, "d15", src_offset);
            emit_store_to_sp(emitter, "d15", dst_offset);
        }
        PhpType::Str => {
            emit_load_from_sp(emitter, "x10", src_offset);
            emit_load_from_sp(emitter, "x11", src_offset + 8);
            emit_store_to_sp(emitter, "x10", dst_offset);
            emit_store_to_sp(emitter, "x11", dst_offset + 8);
        }
        PhpType::Void => {}
        _ => {
            emit_load_from_sp(emitter, "x10", src_offset);
            emit_store_to_sp(emitter, "x10", dst_offset);
        }
    }
}

pub(crate) fn materialize_call_args(
    emitter: &mut Emitter,
    assignments: &[(PhpType, usize, bool)],
    arg_count: usize,
) -> usize {
    let slot_sizes: Vec<usize> = assignments
        .iter()
        .take(arg_count)
        .map(|(ty, _, _)| arg_slot_size(ty))
        .collect();
    let total_temp_bytes: usize = slot_sizes.iter().sum();
    let mut temp_offsets = vec![0usize; arg_count];
    let mut running_offset = 0usize;
    for i in (0..arg_count).rev() {
        temp_offsets[i] = running_offset;
        running_offset += slot_sizes[i];
    }

    let overflow_indices: Vec<usize> = assignments
        .iter()
        .take(arg_count)
        .enumerate()
        .filter_map(|(idx, (_, start_reg, _))| (!assignment_in_register(*start_reg)).then_some(idx))
        .collect();
    let overflow_bytes: usize = overflow_indices.iter().map(|idx| slot_sizes[*idx]).sum();

    if overflow_bytes > 0 {
        emit_adjust_sp(emitter, overflow_bytes, true);
    }

    let base_shift = overflow_bytes;
    for i in 0..arg_count {
        let (ty, start_reg, _is_float) = &assignments[i];
        if !assignment_in_register(*start_reg) {
            continue;
        }
        let src_offset = base_shift + temp_offsets[i];
        match ty {
            PhpType::Bool
            | PhpType::Int
            | PhpType::Mixed
            | PhpType::Union(_)
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Buffer(_)
            | PhpType::Callable
            | PhpType::Object(_)
            | PhpType::Packed(_)
            | PhpType::Pointer(_) => {
                emit_load_from_sp(emitter, &format!("x{}", start_reg), src_offset);
            }
            PhpType::Float => {
                emit_load_from_sp(emitter, &format!("d{}", start_reg), src_offset);
            }
            PhpType::Str => {
                emit_load_from_sp(emitter, &format!("x{}", start_reg), src_offset);
                emit_load_from_sp(emitter, &format!("x{}", start_reg + 1), src_offset + 8);
            }
            PhpType::Void => {}
        }
    }

    if overflow_bytes > 0 {
        let mut dst_offset = total_temp_bytes;
        for idx in &overflow_indices {
            let (ty, _, _) = &assignments[*idx];
            let src_offset = overflow_bytes + temp_offsets[*idx];
            emit_copy_stack_arg_slot(emitter, ty, src_offset, dst_offset);
            dst_offset += slot_sizes[*idx];
        }
    }

    if total_temp_bytes > 0 {
        emit_adjust_sp(emitter, total_temp_bytes, false);
    }

    overflow_bytes
}
