use crate::codegen::emit::Emitter;
use crate::codegen::{abi, context::Context, data_section::DataSection, functions};
use crate::parser::ast::{Expr, ExprKind};
use crate::span::Span;
use crate::types::call_args::{self, SpreadBoundsCheck};
use crate::types::{FunctionSig, PhpType};

mod named;

pub(crate) use named::pushed_temp_bytes;

pub(crate) struct NormalizedCallArgs {
    pub(crate) args: Vec<Expr>,
    pub(crate) spread_length_checks: Vec<SpreadBoundsCheck>,
}

pub(crate) struct PreparedCallArgs {
    pub(crate) all_args: Vec<Expr>,
    pub(crate) variadic_args: Vec<Expr>,
    pub(crate) spread_arg: Option<Expr>,
    pub(crate) spread_at_index: usize,
    pub(crate) regular_param_count: usize,
    pub(crate) is_variadic: bool,
    pub(crate) spread_into_named: bool,
    pub(crate) spread_length_checks: Vec<SpreadBoundsCheck>,
}

pub(crate) struct EmittedCallArgs {
    pub(crate) arg_types: Vec<PhpType>,
    pub(crate) source_temp_bytes: usize,
}

pub(crate) fn has_named_args(args: &[Expr]) -> bool {
    call_args::has_named_args(args)
}

pub(crate) fn regular_param_count(sig: Option<&FunctionSig>, fallback_arg_count: usize) -> usize {
    sig.map(call_args::regular_param_count)
    .unwrap_or(fallback_arg_count)
}

pub(crate) fn named_call_arg_temp_name(call_span: Span, idx: usize) -> String {
    format!(
        "__elephc_named_arg_{}_{}_{}",
        call_span.line, call_span.col, idx
    )
}

pub(crate) fn named_call_prefix_temp_name(call_span: Span) -> String {
    format!("__elephc_named_prefix_{}_{}", call_span.line, call_span.col)
}

pub(crate) fn normalize_named_call_args_with_checks(
    sig: &FunctionSig,
    args: &[Expr],
    regular_param_count: usize,
) -> NormalizedCallArgs {
    normalize_call_args(sig, args, regular_param_count, false, true)
}

pub(crate) fn normalize_builtin_call_args_with_checks(
    sig: &FunctionSig,
    args: &[Expr],
) -> NormalizedCallArgs {
    normalize_call_args(
        sig,
        args,
        regular_param_count(Some(sig), args.len()),
        true,
        false,
    )
}

pub(crate) fn preevaluate_named_call_args_to_temps(
    sig: &FunctionSig,
    args: &[Expr],
    call_span: Span,
    regular_param_count: usize,
    trim_trailing_defaults: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> NormalizedCallArgs {
    let expanded_args = call_args::expand_static_assoc_spread_args(args);
    let args = expanded_args.as_slice();

    if !has_named_args(args) {
        return normalize_call_args(
            sig,
            args,
            regular_param_count,
            trim_trailing_defaults,
            false,
        );
    }

    let rewritten = if args.iter().any(|arg| matches!(arg.kind, ExprKind::Spread(_))) {
        preevaluate_named_spread_args_to_temps(sig, args, call_span, regular_param_count, emitter, ctx, data)
    } else {
        preevaluate_named_non_spread_args_to_temps(
            sig,
            args,
            call_span,
            regular_param_count,
            emitter,
            ctx,
            data,
        )
    };
    normalize_call_args(
        sig,
        &rewritten,
        regular_param_count,
        trim_trailing_defaults,
        false,
    )
}

fn preevaluate_named_spread_args_to_temps(
    sig: &FunctionSig,
    args: &[Expr],
    call_span: Span,
    regular_param_count: usize,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Vec<Expr> {
    let first_named_pos = args
        .iter()
        .position(|arg| matches!(arg.kind, ExprKind::NamedArg { .. }))
        .unwrap_or(args.len());
    let prefix_args = args[..first_named_pos].to_vec();
    let prefix_span = prefix_args
        .first()
        .map(|arg| arg.span)
        .unwrap_or(call_span);
    let prefix_name = named_call_prefix_temp_name(call_span);
    let prefix_expr = single_spread_inner(&prefix_args)
        .unwrap_or_else(|| Expr::new(ExprKind::ArrayLiteral(prefix_args), prefix_span));
    crate::codegen::stmt::emit_assign_stmt(&prefix_name, &prefix_expr, emitter, ctx, data);

    let mut rewritten = vec![Expr::new(
        ExprKind::Spread(Box::new(Expr::new(
            ExprKind::Variable(prefix_name),
            prefix_span,
        ))),
        prefix_span,
    )];

    for (idx, arg) in args.iter().enumerate().skip(first_named_pos) {
        if let ExprKind::NamedArg { name, value } = &arg.kind {
            let rewritten_value =
                preevaluate_named_value_if_needed(sig, regular_param_count, call_span, idx, name, value, emitter, ctx, data);
            rewritten.push(Expr::new(
                ExprKind::NamedArg {
                    name: name.clone(),
                    value: Box::new(rewritten_value),
                },
                arg.span,
            ));
        }
    }

    rewritten
}

fn preevaluate_named_non_spread_args_to_temps(
    sig: &FunctionSig,
    args: &[Expr],
    call_span: Span,
    regular_param_count: usize,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Vec<Expr> {
    let mut rewritten = Vec::new();
    let mut positional_idx = 0usize;

    for (idx, arg) in args.iter().enumerate() {
        match &arg.kind {
            ExprKind::NamedArg { name, value } => {
                let rewritten_value =
                    preevaluate_named_value_if_needed(sig, regular_param_count, call_span, idx, name, value, emitter, ctx, data);
                rewritten.push(Expr::new(
                    ExprKind::NamedArg {
                        name: name.clone(),
                        value: Box::new(rewritten_value),
                    },
                    arg.span,
                ));
            }
            _ => {
                let is_ref = sig
                    .ref_params
                    .get(positional_idx)
                    .copied()
                    .unwrap_or(false);
                if is_ref || is_side_effect_free_literal(arg) {
                    rewritten.push(arg.clone());
                } else {
                    let temp_name = named_call_arg_temp_name(call_span, idx);
                    crate::codegen::stmt::emit_assign_stmt(&temp_name, arg, emitter, ctx, data);
                    rewritten.push(Expr::new(ExprKind::Variable(temp_name), arg.span));
                }
                positional_idx += 1;
            }
        }
    }

    rewritten
}

#[allow(clippy::too_many_arguments)]
fn preevaluate_named_value_if_needed(
    sig: &FunctionSig,
    regular_param_count: usize,
    call_span: Span,
    arg_idx: usize,
    name: &str,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Expr {
    let is_ref = call_args::named_param_index(sig, regular_param_count, name)
        .and_then(|param_idx| sig.ref_params.get(param_idx))
        .copied()
        .unwrap_or(false);
    if is_ref || is_side_effect_free_literal(value) {
        return value.clone();
    }

    let temp_name = named_call_arg_temp_name(call_span, arg_idx);
    crate::codegen::stmt::emit_assign_stmt(&temp_name, value, emitter, ctx, data);
    Expr::new(ExprKind::Variable(temp_name), value.span)
}

fn single_spread_inner(prefix_args: &[Expr]) -> Option<Expr> {
    if let [arg] = prefix_args {
        if let ExprKind::Spread(inner) = &arg.kind {
            return Some((**inner).clone());
        }
    }
    None
}

fn is_side_effect_free_literal(expr: &Expr) -> bool {
    matches!(
        expr.kind,
        ExprKind::StringLiteral(_)
            | ExprKind::IntLiteral(_)
            | ExprKind::FloatLiteral(_)
            | ExprKind::BoolLiteral(_)
            | ExprKind::Null
    )
}

fn normalize_call_args(
    sig: &FunctionSig,
    args: &[Expr],
    regular_param_count: usize,
    trim_trailing_defaults: bool,
    allow_unknown_named_variadic: bool,
) -> NormalizedCallArgs {
    let plan = call_args::plan_call_args_with_regular_param_count(
        sig,
        args,
        Span::dummy(),
        regular_param_count,
        trim_trailing_defaults,
        allow_unknown_named_variadic,
    )
    .expect("codegen received invalid call arguments after type checking");
    NormalizedCallArgs {
        args: plan.normalized_args(),
        spread_length_checks: plan.spread_bounds_checks,
    }
}

pub(crate) fn prepare_call_args(
    sig: Option<&FunctionSig>,
    args_exprs: &[Expr],
    regular_param_count: usize,
) -> PreparedCallArgs {
    let is_variadic = sig.map(|s| s.variadic.is_some()).unwrap_or(false);
    let normalized = sig
        .map(|sig| normalize_named_call_args_with_checks(sig, args_exprs, regular_param_count))
        .unwrap_or_else(|| NormalizedCallArgs {
            args: args_exprs.to_vec(),
            spread_length_checks: Vec::new(),
        });
    let spread_length_checks = normalized.spread_length_checks;
    let normalized_args = normalized.args;

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

    let spread_into_named = spread_arg.is_some() && spread_at_index < regular_param_count;
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
        spread_length_checks,
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
        let coerce_source_ty = if matches!(pushed_ty, PhpType::Mixed) {
            source_ty
        } else {
            &source_repr
        };
        super::super::coerce_result_to_type(emitter, ctx, data, coerce_source_ty, &pushed_ty);
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

pub(crate) fn emit_pushed_call_args(
    args_exprs: &[Expr],
    sig: Option<&FunctionSig>,
    regular_param_count: usize,
    ref_arg_context_label: &str,
    retain_non_variable_ref_args: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> EmittedCallArgs {
    let expanded_args = call_args::expand_static_assoc_spread_args(args_exprs);
    let args_exprs = expanded_args.as_slice();

    if let Some(sig) = sig {
        if has_named_args(args_exprs) {
            return named::emit_source_order_named_call_args(
                args_exprs,
                sig,
                regular_param_count,
                ref_arg_context_label,
                retain_non_variable_ref_args,
                emitter,
                ctx,
                data,
            );
        }
    }

    let prepared = prepare_call_args(sig, args_exprs, regular_param_count);
    emit_spread_length_checks(&prepared.spread_length_checks, emitter, ctx, data);
    let mut arg_types = emit_pushed_non_variadic_args(
        &prepared.all_args,
        sig,
        ref_arg_context_label,
        retain_non_variable_ref_args,
        emitter,
        ctx,
        data,
    );

    if prepared.spread_into_named {
        if let Some(spread_expr) = prepared.spread_arg.as_ref() {
            emit_spread_into_named_params(
                spread_expr,
                sig,
                prepared.spread_at_index,
                prepared.regular_param_count,
                "named params",
                emitter,
                ctx,
                data,
                &mut arg_types,
            );
        }
    }

    if prepared.is_variadic {
        if let Some(spread_expr) = prepared.spread_arg.as_ref() {
            let tail_start = prepared
                .regular_param_count
                .saturating_sub(prepared.spread_at_index);
            let variadic_ty = emit_spread_tail_variadic_array_arg(
                spread_expr,
                tail_start,
                "spread tail as variadic param",
                emitter,
                ctx,
                data,
            );
            arg_types.push(variadic_ty);
        } else if prepared.variadic_args.is_empty() {
            arg_types.push(emit_empty_variadic_array_arg("empty variadic array", emitter));
        } else {
            let variadic_ty = emit_variadic_array_arg_from_exprs(
                &prepared.variadic_args,
                "build variadic array",
                true,
                true,
                emitter,
                ctx,
                data,
            );
            arg_types.push(variadic_ty);
        }
    }

    EmittedCallArgs {
        arg_types,
        source_temp_bytes: 0,
    }
}

pub(crate) fn emit_spread_length_checks(
    checks: &[SpreadBoundsCheck],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    for check in checks {
        let ok_label = ctx.next_label("named_spread_len_ok");
        let fail_label = ctx.next_label("named_spread_len_fail");
        emitter.comment("validate named-argument spread length");
        let _ = super::super::emit_expr(&check.spread_expr, emitter, ctx, data);
        match emitter.target.arch {
            crate::codegen::platform::Arch::AArch64 => {
                emitter.instruction("ldr x9, [x0]");                            // load the logical spread-array length before using synthetic positional reads
                emit_array_length_bounds_check("x9", check.min_len, check.max_len, &fail_label, &ok_label, emitter);
            }
            crate::codegen::platform::Arch::X86_64 => {
                emitter.instruction("mov r10, QWORD PTR [rax]");                // load the logical spread-array length before using synthetic positional reads
                emit_array_length_bounds_check("r10", check.min_len, check.max_len, &fail_label, &ok_label, emitter);
            }
        }
        emitter.label(&fail_label);
        emit_named_spread_length_abort(emitter, data);
        emitter.label(&ok_label);
    }
}

fn emit_array_length_bounds_check(
    length_reg: &str,
    min_len: usize,
    max_len: Option<usize>,
    fail_label: &str,
    ok_label: &str,
    emitter: &mut Emitter,
) {
    match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            abi::emit_load_int_immediate(emitter, "x10", min_len as i64);
            emitter.instruction(&format!("cmp {}, x10", length_reg));           // ensure the array covers every required positional slot
            emitter.instruction(&format!("b.lt {}", fail_label));               // report a missing required argument instead of reading past the payload
            if let Some(max_len) = max_len {
                abi::emit_load_int_immediate(emitter, "x10", max_len as i64);
                emitter.instruction(&format!("cmp {}, x10", length_reg));       // ensure the array does not overwrite the next named slot
                emitter.instruction(&format!("b.le {}", ok_label));             // continue when the array length is within the allowed bounds
            } else {
                emitter.instruction(&format!("b {}", ok_label));                // variadic calls allow remaining spread values to flow into ...$rest
            }
        }
        crate::codegen::platform::Arch::X86_64 => {
            abi::emit_load_int_immediate(emitter, "r11", min_len as i64);
            emitter.instruction(&format!("cmp {}, r11", length_reg));           // ensure the array covers every required positional slot
            emitter.instruction(&format!("jl {}", fail_label));                 // report a missing required argument instead of reading past the payload
            if let Some(max_len) = max_len {
                abi::emit_load_int_immediate(emitter, "r11", max_len as i64);
                emitter.instruction(&format!("cmp {}, r11", length_reg));       // ensure the array does not overwrite the next named slot
                emitter.instruction(&format!("jle {}", ok_label));              // continue when the array length is within the allowed bounds
            } else {
                emitter.instruction(&format!("jmp {}", ok_label));              // variadic calls allow remaining spread values to flow into ...$rest
            }
        }
    }
}

fn emit_named_spread_length_abort(emitter: &mut Emitter, data: &mut DataSection) {
    let (message_label, message_len) =
        data.add_string(b"Fatal error: named argument spread length mismatch\n");
    match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            emitter.instruction("mov x0, #2");                                  // write the named-argument spread diagnostic to stderr
            emitter.adrp("x1", &message_label);
            emitter.add_lo12("x1", "x1", &message_label);
            emitter.instruction(&format!("mov x2, #{}", message_len));          // pass the diagnostic byte length to write()
            emitter.syscall(4);
            abi::emit_exit(emitter, 1);
        }
        crate::codegen::platform::Arch::X86_64 => {
            emitter.instruction("mov edi, 2");                                  // write the named-argument spread diagnostic to stderr
            abi::emit_symbol_address(emitter, "rsi", &message_label);
            emitter.instruction(&format!("mov edx, {}", message_len));          // pass the diagnostic byte length to write()
            emitter.instruction("mov eax, 1");                                  // Linux x86_64 syscall 1 = write
            emitter.instruction("syscall");                                     // emit the fatal named-argument spread diagnostic
            abi::emit_exit(emitter, 1);
        }
    }
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
        coerce_current_value_to_target(emitter, ctx, data, source_elem_ty, target_ty);
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

fn variadic_container_elem_ty(elem_ty: &PhpType) -> PhpType {
    if matches!(elem_ty.codegen_repr(), PhpType::Iterable) {
        PhpType::Mixed
    } else {
        elem_ty.clone()
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
    let array_base_reg = match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => "x20",
        crate::codegen::platform::Arch::X86_64 => "r12",
    };
    emitter.instruction(&format!("mov {}, {}", array_base_reg, abi::int_result_reg(emitter))); // preserve the spread array pointer across boxing or incref helper calls
    let min_required = (0..remaining)
        .filter(|idx| {
            sig.and_then(|sig| sig.defaults.get(spread_at_index + idx))
                .and_then(|default| default.as_ref())
                .is_none()
        })
        .map(|idx| idx + 1)
        .max()
        .unwrap_or(0);
    if min_required > 0 {
        emit_spread_required_length_check(array_base_reg, min_required, emitter, ctx, data);
    }
    for idx in 0..remaining {
        let target_ty = declared_target_ty(sig, spread_at_index + idx);
        let default = sig
            .and_then(|sig| sig.defaults.get(spread_at_index + idx))
            .and_then(|default| default.as_ref());
        let pushed_ty = push_spread_element_or_default_arg(
            array_base_reg,
            idx,
            elem_stride,
            &source_elem_ty,
            default,
            target_ty,
            emitter,
            ctx,
            data,
        );
        arg_types.push(pushed_ty);
    }
}

fn emit_spread_required_length_check(
    array_base_reg: &str,
    min_len: usize,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let ok_label = ctx.next_label("spread_required_len_ok");
    let fail_label = ctx.next_label("spread_required_len_fail");
    emitter.comment("validate spread covers required parameters");
    match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            emitter.instruction(&format!("ldr x9, [{}]", array_base_reg));      // load spread length before reading required unpacked parameters
            abi::emit_load_int_immediate(emitter, "x10", min_len as i64);
            emitter.instruction("cmp x9, x10");                                 // ensure the spread provides every required positional parameter
            emitter.instruction(&format!("b.ge {}", ok_label));                 // continue when all required spread slots are available
        }
        crate::codegen::platform::Arch::X86_64 => {
            emitter.instruction(&format!("mov r10, QWORD PTR [{}]", array_base_reg)); // load spread length before reading required unpacked parameters
            abi::emit_load_int_immediate(emitter, "r11", min_len as i64);
            emitter.instruction("cmp r10, r11");                                // ensure the spread provides every required positional parameter
            emitter.instruction(&format!("jge {}", ok_label));                  // continue when all required spread slots are available
        }
    }
    emitter.label(&fail_label);
    emit_spread_too_few_args_abort(emitter, data);
    emitter.label(&ok_label);
}

fn emit_spread_too_few_args_abort(emitter: &mut Emitter, data: &mut DataSection) {
    let (message_label, message_len) =
        data.add_string(b"Fatal error: too few arguments for spread call\n");
    match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            emitter.instruction("mov x0, #2");                                  // write the spread arity diagnostic to stderr
            emitter.adrp("x1", &message_label);
            emitter.add_lo12("x1", "x1", &message_label);
            emitter.instruction(&format!("mov x2, #{}", message_len));          // pass the diagnostic byte length to write()
            emitter.syscall(4);
            abi::emit_exit(emitter, 1);
        }
        crate::codegen::platform::Arch::X86_64 => {
            emitter.instruction("mov edi, 2");                                  // write the spread arity diagnostic to stderr
            abi::emit_symbol_address(emitter, "rsi", &message_label);
            emitter.instruction(&format!("mov edx, {}", message_len));          // pass the diagnostic byte length to write()
            emitter.instruction("mov eax, 1");                                  // Linux x86_64 syscall 1 = write
            emitter.instruction("syscall");                                     // emit the fatal spread arity diagnostic
            abi::emit_exit(emitter, 1);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn push_spread_element_or_default_arg(
    array_base_reg: &str,
    element_idx: usize,
    elem_stride: usize,
    source_elem_ty: &PhpType,
    default: Option<&Expr>,
    target_ty: Option<&PhpType>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    if let Some(default) = default {
        let use_default = ctx.next_label("spread_default");
        let done = ctx.next_label("spread_done");
        emit_branch_if_spread_element_missing(array_base_reg, element_idx, &use_default, emitter);
        load_array_element_to_result(
            emitter,
            source_elem_ty,
            array_base_reg,
            24 + element_idx * elem_stride,
        );
        let loaded_ty =
            push_loaded_array_element_arg(source_elem_ty, target_ty, emitter, ctx, data);
        abi::emit_jump(emitter, &done);
        emitter.label(&use_default);
        let default_ty = push_expr_arg(default, target_ty, emitter, ctx, data);
        emitter.label(&done);
        return super::super::widen_codegen_type(&loaded_ty, &default_ty);
    }

    load_array_element_to_result(
        emitter,
        source_elem_ty,
        array_base_reg,
        24 + element_idx * elem_stride,
    );
    push_loaded_array_element_arg(source_elem_ty, target_ty, emitter, ctx, data)
}

fn emit_branch_if_spread_element_missing(
    array_base_reg: &str,
    element_idx: usize,
    label: &str,
    emitter: &mut Emitter,
) {
    match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            emitter.instruction(&format!("ldr x9, [{}]", array_base_reg));      // load spread length before choosing spread element or default
            abi::emit_load_int_immediate(emitter, "x10", element_idx as i64);
            emitter.instruction("cmp x9, x10");                                 // check whether this optional spread element exists
            emitter.instruction(&format!("b.le {}", label));                    // use the default when the spread is too short for this slot
        }
        crate::codegen::platform::Arch::X86_64 => {
            emitter.instruction(&format!("mov r10, QWORD PTR [{}]", array_base_reg)); // load spread length before choosing spread element or default
            abi::emit_load_int_immediate(emitter, "r11", element_idx as i64);
            emitter.instruction("cmp r10, r11");                                // check whether this optional spread element exists
            emitter.instruction(&format!("jle {}", label));                     // use the default when the spread is too short for this slot
        }
    }
}

pub(crate) fn emit_spread_tail_variadic_array_arg(
    spread_expr: &Expr,
    tail_start: usize,
    context_label: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment(context_label);
    let spread_ty = super::super::emit_expr(spread_expr, emitter, ctx, data);
    let source_elem_ty = spread_source_elem_ty(&spread_ty);
    let container_elem_ty = variadic_container_elem_ty(&source_elem_ty);
    let offset_reg = abi::int_arg_reg_name(emitter.target, 1);
    let length_reg = abi::int_arg_reg_name(emitter.target, 2);
    abi::emit_load_int_immediate(emitter, offset_reg, tail_start as i64);
    abi::emit_load_int_immediate(emitter, length_reg, -1);
    let helper = if source_elem_ty.codegen_repr().is_refcounted() {
        "__rt_array_slice_refcounted"
    } else {
        "__rt_array_slice"
    };
    abi::emit_call_label(emitter, helper);
    super::super::arrays::emit_array_value_type_stamp(
        emitter,
        abi::int_result_reg(emitter),
        &container_elem_ty,
    );
    abi::emit_push_result_value(emitter, &PhpType::Array(Box::new(container_elem_ty.clone())));
    PhpType::Array(Box::new(container_elem_ty))
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
    let container_elem_ty = variadic_container_elem_ty(&first_elem_ty);
    let elem_size = match container_elem_ty.codegen_repr() {
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
    abi::emit_push_result_value(emitter, &PhpType::Array(Box::new(container_elem_ty.clone())));

    for (idx, variadic_arg) in variadic_args.iter().enumerate() {
        let mut elem_ty = super::super::emit_expr(variadic_arg, emitter, ctx, data);
        let boxed_for_container = if matches!(container_elem_ty, PhpType::Mixed)
            && !matches!(elem_ty, PhpType::Mixed | PhpType::Union(_))
        {
            crate::codegen::emit_box_current_value_as_mixed(emitter, &elem_ty);
            elem_ty = PhpType::Mixed;
            true
        } else {
            false
        };
        if retain_heap_values && !boxed_for_container {
            super::super::retain_borrowed_heap_arg(emitter, variadic_arg, &elem_ty);
        }
        match emitter.target.arch {
            crate::codegen::platform::Arch::AArch64 => {
                emitter.instruction(&format!("ldr {}, [sp]", peek_reg));        // peek the variadic array pointer without removing it from the stack
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

    PhpType::Array(Box::new(container_elem_ty))
}
