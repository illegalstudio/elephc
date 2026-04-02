use crate::codegen::emit::Emitter;
use crate::codegen::{abi, context::Context, data_section::DataSection};
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{FunctionSig, PhpType};

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
    for ty in arg_types {
        if ty.is_float_reg() {
            assignments.push((ty.clone(), float_reg_idx, true));
            float_reg_idx += 1;
        } else {
            assignments.push((ty.clone(), int_reg_idx, false));
            int_reg_idx += ty.register_count();
        }
    }
    assignments
}

pub(crate) fn load_arg_assignments(
    emitter: &mut Emitter,
    assignments: &[(PhpType, usize, bool)],
    arg_count: usize,
) {
    for i in (0..arg_count).rev() {
        let (ty, start_reg, _is_float) = &assignments[i];
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
                emitter.instruction(&format!("ldr x{}, [sp], #16", start_reg)); // pop int-like arg into register
            }
            PhpType::Float => {
                emitter.instruction(&format!("ldr d{}, [sp], #16", start_reg)); // pop float arg into float register
            }
            PhpType::Str => {
                emitter.instruction(&format!(                                   // pop string ptr+len arg into consecutive registers
                    "ldp x{}, x{}, [sp], #16",
                    start_reg,
                    start_reg + 1
                ));
            }
            PhpType::Void => {}
        }
    }
}
