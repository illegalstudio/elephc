//! Purpose:
//! Callable descriptor argument-container and reference-marker lowering.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Returns true when the EIR backend has descriptor dispatch for this callback type.
///
/// A `Mixed`/`Union` callback (e.g. a callable read back from an untyped property)
/// is routed here too: the codegen `callable_descriptor_invoke` unboxes it and
/// dispatches by runtime tag (string function name or closure descriptor), so the
/// robust descriptor path is preferred over the `Op::ExprCall` fallback, which has
/// no Mixed arm.
pub(super) fn descriptor_callback_php_type_supported(php_type: &PhpType) -> bool {
    matches!(
        php_type,
        PhpType::Str
            | PhpType::Callable
            | PhpType::Array(_)
            | PhpType::Object(_)
            | PhpType::Mixed
            | PhpType::Union(_)
    )
}

/// Builds the descriptor-invoker argument container for `call_user_func()`.
///
/// Every argument shape has a container form, so this never declines. Named arguments and
/// spreads build a key-normalized boxed hash; plain positional arguments build an indexed
/// array. Callers depend on that totality, because the callback is already published in the
/// unwind chain by the time the container is built and abandoning the lowering here would either
/// leave an owner record behind or re-evaluate the callback expression on a fallback path.
pub(super) fn lower_descriptor_invoker_arg_container_for_call_user_func(
    ctx: &mut LoweringContext<'_, '_>,
    args: &[Expr],
    sig: Option<&FunctionSig>,
    span: Span,
) -> LoweredValue {
    if crate::types::call_args::has_named_args(args)
        || descriptor_args_need_runtime_unpack_keys(args)
    {
        return lower_named_descriptor_invoker_arg_container(ctx, args, sig, span);
    }
    lower_indexed_descriptor_invoker_arg_array(ctx, args, sig, span)
}

/// Builds an indexed `array<mixed>` container for spread-free positional arguments.
///
/// The array is published for the whole construction: it is the only owner of every argument
/// already inserted, and a later argument expression can throw into a catch in this same frame.
/// It is reloaded from the published slot after every insertion, because growth reallocates the
/// payload and writes the new pointer back into that slot.
pub(super) fn lower_indexed_descriptor_invoker_arg_array(
    ctx: &mut LoweringContext<'_, '_>,
    args: &[Expr],
    sig: Option<&FunctionSig>,
    span: Span,
) -> LoweredValue {
    let elem_ty = PhpType::Mixed;
    let array_ty = PhpType::Array(Box::new(elem_ty.clone()));
    let array = ctx.emit_value(
        Op::ArrayNew,
        Vec::new(),
        Some(Immediate::Capacity(args.len() as u32)),
        array_ty.clone(),
        Op::ArrayNew.default_effects(),
        Some(span),
    );
    let owner = publish_constructed_container(ctx, array, span);
    let mut positional_index = 0usize;
    for arg in args {
        let value = if let Some(var_name) = invoker_ref_arg_variable(ctx, sig, positional_index, arg) {
            lower_invoker_ref_arg_marker(ctx, var_name, arg.span)
        } else {
            let value = lower_expr(ctx, arg);
            coerce_variadic_tail_value(ctx, value, &array_ty, arg.span)
        };
        let array = load_published_container(ctx, owner, array_ty.clone(), arg.span);
        ctx.emit_void(
            Op::ArrayPush,
            vec![array.value, value.value],
            None,
            Op::ArrayPush.default_effects(),
            Some(arg.span),
        );
        crate::ir_lower::stmt::release_indexed_array_write_operand(ctx, Some(&elem_ty), value, arg.span);
        positional_index += 1;
    }
    take_published_container(ctx, owner, array_ty, span)
}

/// Builds a boxed hash argument container for named `call_user_func()` args.
///
/// The hash is published exactly like the indexed container, and reloaded before every insertion.
/// A spread is merged in through the shared descriptor unpack walk, which reads any physical
/// array representation and binds integer keys positionally and string keys by name, so the typed
/// builder covers `f(...$a, name: $x)` without abandoning the callback the caller has already
/// published. Reference markers and the string-transfer rule stay signature-driven.
pub(super) fn lower_named_descriptor_invoker_arg_container(
    ctx: &mut LoweringContext<'_, '_>,
    args: &[Expr],
    sig: Option<&FunctionSig>,
    span: Span,
) -> LoweredValue {
    let hash_ty = PhpType::AssocArray {
        key: Box::new(PhpType::Mixed),
        value: Box::new(PhpType::Mixed),
    };
    let hash = ctx.emit_value(
        Op::HashNew,
        Vec::new(),
        Some(Immediate::Capacity(args.len() as u32)),
        hash_ty.clone(),
        Op::HashNew.default_effects(),
        Some(span),
    );
    let owner = publish_constructed_container(ctx, hash, span);
    let state = begin_descriptor_unpack(ctx, owner, hash_ty.clone(), span);
    // The runtime key counter owns the argument numbering. This compile-time index only
    // selects a by-reference signature slot and, exactly as before, counts explicit
    // positional arguments rather than unpacked entries.
    let mut positional_index = 0usize;
    for arg in args {
        match &arg.kind {
            ExprKind::Spread(inner) => {
                let source = lower_expr(ctx, inner);
                lower_descriptor_unpack_source(ctx, &state, source, arg.span);
            }
            ExprKind::NamedArg { name, value } => {
                let key = lower_string_literal(ctx, name, arg);
                let param_index = sig.and_then(|sig| {
                    let regular_param_count = crate::types::call_args::regular_param_count(sig);
                    crate::types::call_args::named_param_index(sig, regular_param_count, name)
                });
                let value = if let Some(index) = param_index {
                    invoker_ref_arg_variable(ctx, sig, index, value)
                        .map(|var_name| lower_invoker_ref_arg_marker(ctx, var_name, value.span))
                } else {
                    None
                }
                .unwrap_or_else(|| lower_expr(ctx, value));
                bind_descriptor_unpack_named(ctx, &state, key, value, arg.span);
            }
            _ => {
                let value = if let Some(var_name) =
                    invoker_ref_arg_variable(ctx, sig, positional_index, arg)
                {
                    lower_invoker_ref_arg_marker(ctx, var_name, arg.span)
                } else {
                    lower_expr(ctx, arg)
                };
                positional_index += 1;
                bind_descriptor_unpack_positional(ctx, &state, value, arg.span);
            }
        }
    }
    let hash = load_published_container(ctx, owner, hash_ty, span);
    let boxed = ctx.box_value_as_mixed(hash, PhpType::Mixed, Some(span));
    retire_owned_call_operand(ctx, owner, span);
    boxed
}

/// Returns the variable name when this literal argument should be passed by reference.
pub(super) fn invoker_ref_arg_variable<'a>(
    _ctx: &LoweringContext<'_, '_>,
    sig: Option<&FunctionSig>,
    index: usize,
    item: &'a Expr,
) -> Option<&'a str> {
    let ExprKind::Variable(name) = &item.kind else {
        return None;
    };
    if let Some(sig) = sig {
        if !sig.ref_params.get(index).copied().unwrap_or(false) {
            return None;
        }
    }
    Some(name.as_str())
}

/// Returns true when a local slot can be passed directly to a descriptor ref param.
pub(super) fn invoker_ref_arg_storage_compatible(
    ctx: &LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    var_name: &str,
) -> bool {
    let Some((_, param_ty)) = sig.params.get(index) else {
        return true;
    };
    value_ir_type(&param_ty.codegen_repr()) == value_ir_type(&ctx.local_type(var_name).codegen_repr())
}

/// Emits an invoker reference-cell marker for a local variable argument.
pub(super) fn lower_invoker_ref_arg_marker(
    ctx: &mut LoweringContext<'_, '_>,
    var_name: &str,
    span: Span,
) -> LoweredValue {
    let php_type = ctx.local_type(var_name);
    let slot = ctx.declare_local(var_name, php_type);
    ctx.emit_value(
        Op::InvokerRefArg,
        Vec::new(),
        Some(Immediate::LocalSlot(slot)),
        PhpType::Mixed,
        Op::InvokerRefArg.default_effects(),
        Some(span),
    )
}
