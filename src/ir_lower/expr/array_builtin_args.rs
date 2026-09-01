//! Purpose:
//! Array mutation and comparator-specific builtin argument lowering.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers `array_push($local, $value)` as a direct indexed-array mutation.
pub(super) fn lower_static_array_push(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(name.trim_start_matches('\\')) != "array_push" || args.len() != 2 {
        return None;
    }
    if crate::types::call_args::has_named_args(args) || args.iter().any(is_spread_arg) {
        return None;
    }
    let ExprKind::Variable(array_name) = &args[0].kind else {
        return None;
    };
    if !matches!(ctx.local_type(array_name).codegen_repr(), PhpType::Array(_)) {
        return None;
    }
    let array_value = ctx.load_local(array_name, Some(args[0].span));
    if array_value.ir_type != IrType::Heap(IrHeapKind::Array) {
        return None;
    }
    let value = lower_expr(ctx, &args[1]);
    let (array_value, updated_ty, needs_storeback) =
        if crate::ir_lower::stmt::ref_bound_mixed_indexed_array_write(ctx, array_name, value) {
            (array_value, Some(ctx.local_type(array_name)), true)
        } else {
            crate::ir_lower::stmt::prepare_indexed_array_local_write(ctx, array_value, value, expr.span)
        };
    ctx.emit_void(
        Op::ArrayPush,
        vec![array_value.value, value.value],
        None,
        Op::ArrayPush.default_effects(),
        Some(expr.span),
    );
    let elem_ty =
        crate::ir_lower::stmt::indexed_array_write_element_type(ctx, array_value, updated_ty.as_ref());
    crate::ir_lower::stmt::finish_indexed_array_local_write(
        ctx,
        array_name,
        array_value,
        updated_ty,
        needs_storeback,
        expr.span,
    );
    crate::ir_lower::stmt::release_indexed_array_write_operand(ctx, elem_ty.as_ref(), value, expr.span);
    Some(lower_null(ctx, expr))
}

/// Lowers builtin call operands, applying builtin-specific preservation where source order matters.
pub(super) fn lower_builtin_call_args(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    sig: Option<&FunctionSig>,
    args: &[Expr],
) -> Vec<crate::ir::ValueId> {
    if is_empty_static_indexed_spread_arg(args) && zero_arity_call_signature(name, sig) {
        return Vec::new();
    }
    let canonical = php_symbol_key(name.trim_start_matches('\\'));
    if canonical == "eval" {
        return lower_eval_args(ctx, sig, args);
    }
    if canonical == "json_encode"
        && !crate::types::call_args::has_named_args(args)
        && !args.iter().any(is_spread_arg)
    {
        return lower_json_encode_args(ctx, sig, args);
    }
    let argument_lowering = crate::builtins::registry::lookup(&canonical)
        .map(|def| def.spec.semantics.argument_lowering)
        .unwrap_or(crate::builtins::semantics::BuiltinArgumentLowering::Standard);
    match argument_lowering {
        crate::builtins::semantics::BuiltinArgumentLowering::Count => {
            lower_count_args(ctx, sig, args)
        }
        crate::builtins::semantics::BuiltinArgumentLowering::Date => {
            lower_date_args(ctx, sig, args)
        }
        crate::builtins::semantics::BuiltinArgumentLowering::JsonDecode => {
            lower_json_decode_args(ctx, sig, args)
        }
        crate::builtins::semantics::BuiltinArgumentLowering::PregReplaceCallback
            if !crate::types::call_args::has_named_args(args)
                && !args.iter().any(is_spread_arg) =>
        {
            lower_preg_replace_callback_args(ctx, sig, args)
        }
        crate::builtins::semantics::BuiltinArgumentLowering::PositionalRegex
            if !crate::types::call_args::has_named_args(args)
                && !args.iter().any(is_spread_arg) =>
        {
            lower_args(ctx, args)
        }
        crate::builtins::semantics::BuiltinArgumentLowering::UserValueSort
            if !crate::types::call_args::has_named_args(args)
                && !args.iter().any(is_spread_arg) =>
        {
            lower_user_value_sort_args(ctx, sig, args)
        }
        crate::builtins::semantics::BuiltinArgumentLowering::ReverseKeySort => {
            lower_reverse_key_sort_args(ctx, sig, args)
        }
        crate::builtins::semantics::BuiltinArgumentLowering::OpensslEncrypt => {
            prepare_openssl_encrypt_tag_local(ctx, args);
            if !crate::types::call_args::has_named_args(args)
                && !args.iter().any(is_spread_arg)
            {
                lower_positional_builtin_args_with_signature(ctx, sig, args)
            } else {
                lower_args_with_signature(ctx, sig, args)
            }
        }
        crate::builtins::semantics::BuiltinArgumentLowering::ArraySplice
            if !args.iter().any(is_spread_arg) =>
        {
            lower_array_splice_args(ctx, sig, args)
        }
        _ if !crate::types::call_args::has_named_args(args)
            && !args.iter().any(is_spread_arg) =>
        {
            lower_positional_builtin_args_with_signature(ctx, sig, args)
        }
        _ => lower_args_with_signature(ctx, sig, args),
    }
}

/// Projects DatePeriod's php-src virtual property shape before JSON object encoding.
fn lower_json_encode_args(
    ctx: &mut LoweringContext<'_, '_>,
    sig: Option<&FunctionSig>,
    args: &[Expr],
) -> Vec<ValueId> {
    args.iter()
        .enumerate()
        .map(|(index, arg)| {
            let value = if let Some(sig) = sig {
                let value = lower_arg_with_signature(ctx, sig, index, arg);
                LoweredValue {
                    value,
                    ir_type: ctx.builder.value_type(value),
                }
            } else {
                lower_expr(ctx, arg)
            };
            if index != 0
                || singular_object_class(&ctx.builder.value_php_type(value.value)).is_none_or(
                    |(class_name, _)| !class_extends_class(ctx, class_name, "DatePeriod"),
                )
            {
                return value.value;
            }
            let release_source = ctx.value_is_owning_temporary(value)
                && !ctx.value_is_owned_unboxed_local_load(value.value);
            let properties = lower_json_date_object_from_value(ctx, value.value, arg.span)
                .unwrap_or_else(|error| panic!("checked DatePeriod JSON projection failed: {error}"));
            if release_source {
                crate::ir_lower::ownership::release_if_owned(ctx, value, Some(arg.span));
            }
            properties.value
        })
        .collect()
}

/// Promotes the OpenSSL encrypt tag target to string-capable storage before lowering its load.
fn prepare_openssl_encrypt_tag_local(ctx: &mut LoweringContext<'_, '_>, args: &[Expr]) {
    let expanded = crate::types::call_args::expand_static_assoc_spread_args(args);
    let tag = expanded
        .iter()
        .find_map(|arg| match &arg.kind {
            ExprKind::NamedArg { name, value } if php_symbol_key(name) == "tag" => {
                Some(value.as_ref())
            }
            _ => None,
        })
        .or_else(|| {
            expanded
                .get(5)
                .filter(|arg| !matches!(arg.kind, ExprKind::NamedArg { .. }))
        });
    let Some(Expr {
        kind: ExprKind::Variable(name),
        ..
    }) = tag
    else {
        return;
    };
    ctx.set_local_type(name, PhpType::Str);
}

/// Lowers plain positional builtin operands without materializing omitted defaults or packing tails.
///
/// Runtime helpers consume the caller-provided arity, while the registry signature still supplies
/// by-reference handling and scalar storage coercions for every visible regular parameter.
pub(super) fn lower_positional_builtin_args_with_signature(
    ctx: &mut LoweringContext<'_, '_>,
    sig: Option<&FunctionSig>,
    args: &[Expr],
) -> Vec<crate::ir::ValueId> {
    let Some(sig) = sig else {
        return lower_args(ctx, args);
    };
    let regular_param_count = crate::types::call_args::regular_param_count(sig);
    args.iter()
        .enumerate()
        .map(|(index, arg)| {
            if index < regular_param_count {
                lower_arg_with_signature(ctx, sig, index, arg)
            } else {
                lower_expr(ctx, arg).value
            }
        })
        .collect()
}

/// Promotes a packed local before `krsort()` so descending iteration can preserve integer keys.
///
/// Packed storage has no independent iteration-order metadata: reversing its slots would also
/// change `$array[0]`. Converting the by-reference local to hash storage keeps each key/value pair
/// intact while allowing the runtime helper to reorder only the insertion-order links.
fn lower_reverse_key_sort_args(
    ctx: &mut LoweringContext<'_, '_>,
    sig: Option<&FunctionSig>,
    args: &[Expr],
) -> Vec<crate::ir::ValueId> {
    let Some(sig) = sig else {
        return lower_args(ctx, args);
    };
    if args.len() == 1 && !args.iter().any(is_spread_arg) {
        let arg = match &args[0].kind {
            ExprKind::NamedArg { value, .. } => value.as_ref(),
            _ => &args[0],
        };
        if let Some(value) = lower_indexed_array_ref_arg_to_hash(ctx, sig, 0, arg) {
            return vec![value];
        }
    }
    lower_args_with_signature(ctx, Some(sig), args)
}

/// Converts one packed by-reference local argument into key-preserving associative storage.
fn lower_indexed_array_ref_arg_to_hash(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    arg: &Expr,
) -> Option<crate::ir::ValueId> {
    if !sig.ref_params.get(index).copied().unwrap_or(false) {
        return None;
    }
    let ExprKind::Variable(name) = &arg.kind else {
        return None;
    };
    let PhpType::Array(elem_ty) = ctx.local_type(name).codegen_repr() else {
        return None;
    };
    let assoc_ty = PhpType::AssocArray {
        key: Box::new(PhpType::Int),
        value: elem_ty,
    };
    let array = ctx.load_local(name, Some(arg.span));
    ctx.prepare_mutated_local_owner(name, array, assoc_ty.clone(), Some(arg.span));
    let hash = ctx.emit_value(
        Op::ArrayToHash,
        vec![array.value],
        None,
        assoc_ty.clone(),
        Op::ArrayToHash.default_effects(),
        Some(arg.span),
    );
    ctx.store_prepared_mutated_local(name, hash, assoc_ty, Some(arg.span));
    Some(ctx.load_local(name, Some(arg.span)).value)
}

/// Lowers `count()` arguments, dropping a statically-default mode argument.
///
/// The EIR backend implements only `COUNT_NORMAL`; a literal `0` mode (named
/// or positional) is semantically a no-op and would otherwise trip the unary
/// count contract in codegen.
pub(super) fn lower_count_args(
    ctx: &mut LoweringContext<'_, '_>,
    sig: Option<&FunctionSig>,
    args: &[Expr],
) -> Vec<crate::ir::ValueId> {
    let pruned: Vec<Expr> = args
        .iter()
        .enumerate()
        .filter(|(index, arg)| !count_arg_is_static_default_mode(*index, arg))
        .map(|(_, arg)| arg.clone())
        .collect();
    let mut operands = lower_args_with_signature(ctx, sig, &pruned);
    // Named and spread plans re-materialize the optional `mode` default even
    // after the AST prune; a trailing constant-zero mode stays a no-op for
    // the unary count contract, so drop the operand (DCE reclaims the const).
    if operands.len() == 2 {
        let trailing_zero_mode = ctx
            .builder
            .value_defining_instruction(operands[1])
            .is_some_and(|inst| {
                inst.op == Op::ConstI64
                    && matches!(inst.immediate, Some(crate::ir::Immediate::I64(0)))
            });
        if trailing_zero_mode {
            operands.pop();
        }
    }
    operands
}

/// Returns true when a `count()` argument is a statically-zero mode.
pub(super) fn count_arg_is_static_default_mode(index: usize, arg: &Expr) -> bool {
    match &arg.kind {
        ExprKind::NamedArg { name, value } => {
            name == "mode" && matches!(value.kind, ExprKind::IntLiteral(0))
        }
        ExprKind::IntLiteral(0) => index == 1,
        _ => false,
    }
}

/// Lowers eval's code operand and coerces it through PHP string-conversion rules.
pub(super) fn lower_eval_args(
    ctx: &mut LoweringContext<'_, '_>,
    sig: Option<&FunctionSig>,
    args: &[Expr],
) -> Vec<crate::ir::ValueId> {
    let operands = lower_args_with_signature(ctx, sig, args);
    let Some(code) = operands.first().copied() else {
        return operands;
    };
    let code_value = LoweredValue {
        value: code,
        ir_type: ctx.builder.value_type(code),
    };
    let span = args.first().map(|arg| arg.span);
    vec![coerce_to_string_at_span(ctx, code_value, span).value]
}

/// Lowers `usort`/`uasort` arguments, typing an unannotated comparator closure
/// against the array's object element type.
///
/// `usort`/`uasort` compare values, so a comparator over an array of objects must
/// see each element as the object handle — for `<=>` instant comparison and for
/// property/method access — not the raw pointer-sized integer the runtime stores
/// in each slot. The array operand is lowered exactly as the default positional
/// path would (positional builtin calls reach here with no signature); only an
/// unannotated closure comparator over an object-element array is specialized,
/// matching the element-type hint the checker applied to the comparator body.
pub(super) fn lower_user_value_sort_args(
    ctx: &mut LoweringContext<'_, '_>,
    sig: Option<&FunctionSig>,
    args: &[Expr],
) -> Vec<crate::ir::ValueId> {
    if args.len() != 2 || !matches!(&args[1].kind, ExprKind::Closure { .. }) {
        return lower_args_with_signature(ctx, sig, args);
    }
    // The mutating sort keeps its by-reference local storeback in the EIR backend,
    // so the array operand only has to resolve to the array's value here.
    let array = match sig {
        Some(sig) => lower_arg_with_signature(ctx, sig, 0, &args[0]),
        None => lower_expr(ctx, &args[0]).value,
    };
    let elem_ty = match ctx.builder.value_php_type(array).codegen_repr() {
        PhpType::Array(elem) => elem.codegen_repr(),
        _ => PhpType::Int,
    };
    // Only an object-element array needs the comparator parameters re-typed; scalar
    // comparators already lower correctly through the default path.
    let callback = if matches!(elem_ty, PhpType::Object(_)) {
        lower_value_sort_comparator_closure(ctx, &args[1], elem_ty)
    } else {
        match sig {
            Some(sig) => lower_arg_with_signature(ctx, sig, 1, &args[1]),
            None => lower_expr(ctx, &args[1]).value,
        }
    };
    vec![array, callback]
}

/// Lowers a value-sort comparator closure with both parameters typed as the array element.
///
/// Falls back to the plain closure lowering for any non-closure callback operand,
/// though callers only reach this path with a closure comparator.
pub(super) fn lower_value_sort_comparator_closure(
    ctx: &mut LoweringContext<'_, '_>,
    callback: &Expr,
    elem_ty: PhpType,
) -> crate::ir::ValueId {
    let ExprKind::Closure {
        params,
        variadic,
        variadic_by_ref,
        return_type,
        body,
        captures,
        capture_refs,
        is_static,
        ..
    } = &callback.kind
    else {
        return lower_expr(ctx, callback).value;
    };
    lower_closure_with_context(
        ctx,
        params,
        variadic.as_deref(),
        *variadic_by_ref,
        return_type.as_ref(),
        body,
        captures,
        capture_refs,
        callback,
        &[elem_ty.clone(), elem_ty],
        None,
        *is_static,
    )
    .value
}
