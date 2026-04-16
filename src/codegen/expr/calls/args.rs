use crate::codegen::emit::Emitter;
use crate::codegen::{abi, context::Context, data_section::DataSection, functions};
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{FunctionSig, PhpType};

pub(crate) struct PreparedCallArgs {
    pub(crate) all_args: Vec<Expr>,
    pub(crate) variadic_args: Vec<Expr>,
    pub(crate) spread_arg: Option<Expr>,
    pub(crate) spread_at_index: usize,
    pub(crate) regular_param_count: usize,
    pub(crate) is_variadic: bool,
    pub(crate) spread_into_named: bool,
}

pub(crate) fn has_named_args(args: &[Expr]) -> bool {
    args.iter()
        .any(|arg| matches!(arg.kind, ExprKind::NamedArg { .. }))
}

pub(crate) fn regular_param_count(sig: Option<&FunctionSig>, fallback_arg_count: usize) -> usize {
    sig.map(|sig| {
        if sig.variadic.is_some() {
            sig.params.len().saturating_sub(1)
        } else {
            sig.params.len()
        }
    })
    .unwrap_or(fallback_arg_count)
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

pub(crate) fn prepare_call_args(
    sig: Option<&FunctionSig>,
    args_exprs: &[Expr],
    regular_param_count: usize,
) -> PreparedCallArgs {
    let is_variadic = sig.map(|s| s.variadic.is_some()).unwrap_or(false);
    let normalized_args = sig
        .map(|sig| normalize_named_call_args(sig, args_exprs, regular_param_count))
        .unwrap_or_else(|| args_exprs.to_vec());

    let mut regular_args = Vec::new();
    let mut variadic_args = Vec::new();
    let mut spread_arg = None;
    let mut spread_at_index = 0usize;

    for (idx, arg) in normalized_args.iter().enumerate() {
        if let ExprKind::Spread(inner) = &arg.kind {
            spread_arg = Some((**inner).clone());
            spread_at_index = regular_args.len();
        } else if is_variadic && idx >= regular_param_count {
            variadic_args.push(arg.clone());
        } else {
            regular_args.push(arg.clone());
        }
    }

    let spread_into_named = spread_arg.is_some() && !is_variadic;
    let mut all_args = regular_args;
    if !spread_into_named {
        if let Some(sig) = sig {
            for idx in all_args.len()..regular_param_count {
                if let Some(Some(default)) = sig.defaults.get(idx) {
                    all_args.push(default.clone());
                }
            }
        }
    }

    PreparedCallArgs {
        all_args,
        variadic_args,
        spread_arg,
        spread_at_index,
        regular_param_count,
        is_variadic,
        spread_into_named,
    }
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

pub(crate) fn emit_pushed_non_variadic_args(
    all_args: &[Expr],
    sig: Option<&FunctionSig>,
    ref_arg_context_label: &str,
    retain_non_variable_ref_args: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Vec<PhpType> {
    let mut arg_types = Vec::new();

    for (idx, arg) in all_args.iter().enumerate() {
        let is_ref = sig
            .and_then(|sig| sig.ref_params.get(idx))
            .copied()
            .unwrap_or(false);
        let target_ty = declared_target_ty(sig, idx);

        if is_ref {
            if let ExprKind::Variable(var_name) = &arg.kind {
                if !emit_ref_arg_variable_address(var_name, ref_arg_context_label, emitter, ctx) {
                    continue;
                }
            } else {
                let source_ty = super::super::emit_expr(arg, emitter, ctx, data);
                if retain_non_variable_ref_args {
                    super::super::retain_borrowed_heap_arg(emitter, arg, &source_ty);
                }
            }
            push_arg_value(emitter, &PhpType::Int);
            arg_types.push(PhpType::Int);
        } else {
            let pushed_ty = push_expr_arg(arg, target_ty, emitter, ctx, data);
            arg_types.push(pushed_ty);
        }
    }

    arg_types
}

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
        coerce_current_value_to_target(emitter, ctx, data, &source_repr, target_ty);
    if !boxed_to_mixed {
        abi::emit_incref_if_refcounted(emitter, &source_repr);
    }
    push_arg_value(emitter, &pushed_ty);
    pushed_ty
}

fn spread_source_elem_ty(spread_ty: &PhpType) -> PhpType {
    match spread_ty {
        PhpType::Array(elem) => (**elem).clone(),
        PhpType::AssocArray { value, .. } => (**value).clone(),
        _ => PhpType::Int,
    }
}

fn store_current_array_element(
    emitter: &mut Emitter,
    array_reg: &str,
    elem_idx: usize,
    elem_ty: &PhpType,
) {
    match elem_ty.codegen_repr() {
        PhpType::Float => {
            abi::emit_store_to_address(emitter, abi::float_result_reg(emitter), array_reg, 24 + elem_idx * 8); // store float element into the variadic array payload
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_store_to_address(emitter, ptr_reg, array_reg, 24 + elem_idx * 16); // store variadic string pointer into the array payload
            abi::emit_store_to_address(emitter, len_reg, array_reg, 24 + elem_idx * 16 + 8); // store variadic string length next to the payload pointer
        }
        PhpType::Void => {}
        _ => {
            abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), array_reg, 24 + elem_idx * 8); // store scalar or boxed variadic payload into the array data area
        }
    }
}

pub(crate) fn emit_spread_into_named_params(
    spread_expr: &Expr,
    sig: Option<&FunctionSig>,
    spread_at_index: usize,
    regular_param_count: usize,
    context_label: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    arg_types: &mut Vec<PhpType>,
) {
    let remaining = regular_param_count.saturating_sub(spread_at_index);
    if remaining == 0 {
        return;
    }

    emitter.comment(&format!("unpack spread into {} {}", remaining, context_label));
    let spread_ty = functions::infer_contextual_type(spread_expr, ctx);
    let source_elem_ty = spread_source_elem_ty(&spread_ty);
    let elem_stride = array_element_stride(&source_elem_ty);
    let _ = super::super::emit_expr(spread_expr, emitter, ctx, data);
    let array_data_reg = match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => "x20",
        crate::codegen::platform::Arch::X86_64 => "r10",
    };
    emitter.instruction(&format!("mov {}, {}", array_data_reg, abi::int_result_reg(emitter))); // preserve the spread array pointer across boxing or incref helper calls
    match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            emitter.instruction(&format!("add {}, {}, #24", array_data_reg, array_data_reg)); // skip the array header to point at the first spread element
        }
        crate::codegen::platform::Arch::X86_64 => {
            emitter.instruction(&format!("add {}, 24", array_data_reg));        // skip the array header to point at the first spread element
        }
    }
    for idx in 0..remaining {
        let target_ty = declared_target_ty(sig, spread_at_index + idx);
        load_array_element_to_result(emitter, &source_elem_ty, array_data_reg, idx * elem_stride);
        let pushed_ty =
            push_loaded_array_element_arg(&source_elem_ty, target_ty, emitter, ctx, data);
        arg_types.push(pushed_ty);
    }
}

pub(crate) fn emit_spread_variadic_array_arg(
    spread_expr: &Expr,
    context_label: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment(context_label);
    let spread_ty = super::super::emit_expr(spread_expr, emitter, ctx, data);
    super::super::retain_borrowed_heap_arg(emitter, spread_expr, &spread_ty);
    abi::emit_push_result_value(emitter, &spread_ty);
    spread_ty
}

pub(crate) fn emit_empty_variadic_array_arg(context_label: &str, emitter: &mut Emitter) -> PhpType {
    emitter.comment(context_label);
    let (capacity_reg, elem_size_reg) = match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => ("x0", "x1"),
        crate::codegen::platform::Arch::X86_64 => ("rdi", "rsi"),
    };
    abi::emit_load_int_immediate(emitter, capacity_reg, 4);
    abi::emit_load_int_immediate(emitter, elem_size_reg, 8);
    abi::emit_call_label(emitter, "__rt_array_new");
    abi::emit_push_result_value(emitter, &PhpType::Array(Box::new(PhpType::Int)));
    PhpType::Array(Box::new(PhpType::Int))
}

pub(crate) fn emit_variadic_array_arg_from_exprs(
    variadic_args: &[Expr],
    context_label: &str,
    retain_heap_values: bool,
    stamp_value_type: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let elem_count = variadic_args.len();
    let first_elem_ty = functions::infer_contextual_type(&variadic_args[0], ctx);
    let elem_size = match first_elem_ty.codegen_repr() {
        PhpType::Str => 16,
        _ => 8,
    };
    let (capacity_reg, elem_size_reg, peek_reg, len_reg) = match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => ("x0", "x1", "x9", "x10"),
        crate::codegen::platform::Arch::X86_64 => ("rdi", "rsi", "r11", "r10"),
    };

    emitter.comment(&format!("{} ({} elements)", context_label, elem_count));
    abi::emit_load_int_immediate(emitter, capacity_reg, elem_count as i64);
    abi::emit_load_int_immediate(emitter, elem_size_reg, elem_size as i64);
    abi::emit_call_label(emitter, "__rt_array_new");
    abi::emit_push_result_value(emitter, &PhpType::Array(Box::new(first_elem_ty.clone())));

    for (idx, variadic_arg) in variadic_args.iter().enumerate() {
        let elem_ty = super::super::emit_expr(variadic_arg, emitter, ctx, data);
        if retain_heap_values {
            super::super::retain_borrowed_heap_arg(emitter, variadic_arg, &elem_ty);
        }
        match emitter.target.arch {
            crate::codegen::platform::Arch::AArch64 => {
                emitter.instruction(&format!("ldr {}, [sp]", peek_reg));         // peek the variadic array pointer without removing it from the stack
            }
            crate::codegen::platform::Arch::X86_64 => {
                emitter.instruction(&format!("mov {}, QWORD PTR [rsp]", peek_reg)); // peek the variadic array pointer without removing it from the stack
            }
        }
        if stamp_value_type && idx == 0 {
            super::super::arrays::emit_array_value_type_stamp(emitter, peek_reg, &elem_ty);
        }
        store_current_array_element(emitter, peek_reg, idx, &elem_ty);
        abi::emit_load_int_immediate(emitter, len_reg, (idx + 1) as i64);
        abi::emit_store_to_address(emitter, len_reg, peek_reg, 0);
    }

    PhpType::Array(Box::new(first_elem_ty))
}
