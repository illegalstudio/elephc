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
pub(super) fn lower_descriptor_invoker_arg_container_for_call_user_func(
    ctx: &mut LoweringContext<'_, '_>,
    args: &[Expr],
    sig: Option<&FunctionSig>,
    span: Span,
) -> Option<LoweredValue> {
    if crate::types::call_args::has_named_args(args) {
        if args.iter().any(is_spread_arg) {
            return None;
        }
        return Some(lower_named_descriptor_invoker_arg_container(ctx, args, sig, span));
    }
    Some(lower_indexed_descriptor_invoker_arg_array(ctx, args, sig, span))
}

/// Builds an indexed `array<mixed>` argument container, expanding positional spreads.
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
    let mut positional_index = 0usize;
    for arg in args {
        if let ExprKind::Spread(inner) = &arg.kind {
            let source = lower_expr(ctx, inner);
            lower_indexed_array_spread_into_array(ctx, array, source, Some(&elem_ty), arg.span);
            continue;
        }
        let value = if let Some(var_name) = invoker_ref_arg_variable(ctx, sig, positional_index, arg) {
            lower_invoker_ref_arg_marker(ctx, var_name, arg.span)
        } else {
            let value = lower_expr(ctx, arg);
            coerce_variadic_tail_value(ctx, value, &array_ty, arg.span)
        };
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
    array
}

/// Builds a boxed hash argument container for named `call_user_func()` args.
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
        hash_ty,
        Op::HashNew.default_effects(),
        Some(span),
    );
    let mut next_positional_key = 0i64;
    for arg in args {
        match &arg.kind {
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
                ctx.emit_void(
                    Op::HashSet,
                    vec![hash.value, key.value, value.value],
                    None,
                    Op::HashSet.default_effects(),
                    Some(arg.span),
                );
            }
            _ => {
                let key = emit_i64_at_span(ctx, next_positional_key, arg.span);
                let value = if let Some(var_name) =
                    invoker_ref_arg_variable(ctx, sig, next_positional_key as usize, arg)
                {
                    lower_invoker_ref_arg_marker(ctx, var_name, arg.span)
                } else {
                    lower_expr(ctx, arg)
                };
                next_positional_key += 1;
                ctx.emit_void(
                    Op::HashSet,
                    vec![hash.value, key.value, value.value],
                    None,
                    Op::HashSet.default_effects(),
                    Some(arg.span),
                );
            }
        }
    }
    ctx.box_value_as_mixed(hash, PhpType::Mixed, Some(span))
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
///
/// The marker points at the local's own storage, so the callee writes straight back into the
/// caller's slot at the slot's declared type. That is right for a NAMED by-reference parameter:
/// `f(&$v)` declares what it writes, and the checker holds the caller's local to it.
pub(super) fn lower_invoker_ref_arg_marker(
    ctx: &mut LoweringContext<'_, '_>,
    var_name: &str,
    span: Span,
) -> LoweredValue {
    lower_invoker_ref_arg_marker_at(ctx, var_name, RefArgStorage::AsDeclared, span)
}

/// Emits the marker for an argument absorbed by a BY-REFERENCE VARIADIC tail, widening the
/// local to a boxed `Mixed` reference cell first.
///
/// `&...$items` is the one by-reference form PHP does not constrain. The callee writes through
/// `$items[n]`, an untyped element of an `array<mixed>`, so it may store a value of any type
/// into a caller local currently holding something else -- `function r(&...$items) { $items[0] =
/// "s"; }` called with an `int` local is ordinary PHP. Pointing the marker at the local's
/// concrete storage made that write land in a slot sized and typed for the OLD value: a string
/// written over an `I64` slot read back as the string's pointer and printed as an integer, with
/// no diagnostic (issue #1062).
///
/// Widening is confined to this form on purpose. A named by-reference parameter keeps concrete
/// storage on both sides, and boxing the caller's local there would hand the callee a box
/// pointer where it expects the value itself.
///
/// `promote_local_mixed_ref_cell` is idempotent and is the same helper PDO's `bindColumn` /
/// `bindParam` use for exactly this situation.
pub(super) fn lower_invoker_ref_variadic_arg_marker(
    ctx: &mut LoweringContext<'_, '_>,
    var_name: &str,
    span: Span,
) -> LoweredValue {
    lower_invoker_ref_arg_marker_at(ctx, var_name, RefArgStorage::WidenedToMixed, span)
}

/// How wide the reference cell behind an invoker ref-arg marker has to be.
#[derive(Clone, Copy, PartialEq, Eq)]
enum RefArgStorage {
    /// The callee writes at the parameter's declared type, so the local keeps its own storage.
    AsDeclared,
    /// The callee may write any type, so the local is boxed before it is aliased.
    WidenedToMixed,
}

fn lower_invoker_ref_arg_marker_at(
    ctx: &mut LoweringContext<'_, '_>,
    var_name: &str,
    storage: RefArgStorage,
    span: Span,
) -> LoweredValue {
    if storage == RefArgStorage::WidenedToMixed {
        ctx.promote_local_mixed_ref_cell(var_name, Some(span));
    }
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

