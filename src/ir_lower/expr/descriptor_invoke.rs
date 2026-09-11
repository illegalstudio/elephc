//! Purpose:
//! Callable descriptor invocation and signature resolution.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers `call_user_func*` for receiver-bound first-class callables through `expr_call`.
///
/// The callback is a receiver-bound descriptor the caller's expression just built, so it is
/// published before the argument expressions run, exactly like the descriptor-invoke path does.
pub(super) fn lower_instance_callable_call_user_func(
    ctx: &mut LoweringContext<'_, '_>,
    callback_expr: &Expr,
    callback: StaticCallableBinding,
    callback_args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let result_type = static_callable_return_type(ctx, &callback);
    let signature = instance_callable_signature(&callback).cloned();
    let callback = lower_expr(ctx, callback_expr);
    Some(emit_rooted_expr_call(
        ctx,
        callback,
        signature.as_ref(),
        callback_args,
        result_type,
        expr.span,
    ))
}

/// Emits `Op::ExprCall` with the callback published for the whole argument evaluation.
///
/// The callback value is owned by nothing else while the arguments are lowered, and the owned
/// result is owned by nothing else while that published callback is retired, so both get a
/// record in the unwind chain, nested strictly: result first, callback inside it.
pub(super) fn emit_rooted_expr_call(
    ctx: &mut LoweringContext<'_, '_>,
    callback: LoweredValue,
    signature: Option<&FunctionSig>,
    args: &[Expr],
    result_type: PhpType,
    span: Span,
) -> LoweredValue {
    let result_staging = prepublish_call_result(ctx, &result_type, span);
    let (callback, callback_owner) = root_owned_call_operand(ctx, callback, span);
    let mut operands = vec![callback.value];
    operands.extend(lower_args_with_signature(ctx, signature, args));
    let call = ctx.emit_value(
        Op::ExprCall,
        operands,
        callable_profile_immediate(),
        result_type,
        Op::ExprCall.default_effects(),
        Some(span),
    );
    stage_call_result(ctx, result_staging.as_ref(), call, span);
    if let Some(slot) = callback_owner {
        retire_owned_call_operand(ctx, slot, span);
    }
    take_prepublished_call_result(ctx, result_staging, call, span)
}

/// Lowers dynamic `call_user_func()` callbacks through descriptor invocation.
pub(super) fn lower_dynamic_call_user_func(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(name.trim_start_matches('\\')) != "call_user_func" || args.is_empty() {
        return None;
    }
    if matches!(args[0].kind, ExprKind::NamedArg { .. } | ExprKind::Spread(_)) {
        return None;
    }
    let signature = callable_descriptor_signature_for_expr(ctx, &args[0]);
    let callback = lower_expr(ctx, &args[0]);
    Some(lower_call_user_func_from_lowered_callback(
        ctx,
        callback,
        &args[1..],
        signature.as_ref(),
        expr,
    ))
}

/// Lowers `call_user_func()` for an already evaluated callback, never abandoning that evaluation.
///
/// Every decision left at this point is total. A descriptor-dispatchable storage shape goes
/// through descriptor invocation; a plain positional call of any other shape goes through the
/// value-call opcode with the SAME callback value; and a named or spread call of a shape with no
/// descriptor arm is boxed into `Mixed`, which the backend invoker unboxes and dispatches by
/// runtime tag. Returning `None` from here instead would make the caller re-lower the callback
/// expression and run its side effects twice.
pub(super) fn lower_call_user_func_from_lowered_callback(
    ctx: &mut LoweringContext<'_, '_>,
    callback: LoweredValue,
    args: &[Expr],
    sig: Option<&FunctionSig>,
    expr: &Expr,
) -> LoweredValue {
    if descriptor_callback_php_type_supported(
        &ctx.builder.value_php_type(callback.value).codegen_repr(),
    ) {
        return lower_call_user_func_descriptor_invoke_from_value(ctx, callback, args, sig, expr);
    }
    if !crate::types::call_args::has_named_args(args) && !args.iter().any(is_spread_arg) {
        return emit_rooted_expr_call(ctx, callback, None, args, PhpType::Mixed, expr.span);
    }
    let callback = coerce_descriptor_invoker_mixed_value(ctx, callback, expr.span);
    lower_call_user_func_descriptor_invoke_from_value(ctx, callback, args, sig, expr)
}

/// Lowers dynamic `call_user_func_array()` through the descriptor-invoker EIR path.
pub(super) fn lower_dynamic_call_user_func_array(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(name.trim_start_matches('\\')) != "call_user_func_array" {
        return None;
    }
    let [callback_expr, arg_array_expr] = args else {
        return None;
    };
    if crate::types::call_args::has_named_args(args) || args.iter().any(is_spread_arg) {
        return None;
    }
    let signature = callable_descriptor_signature_for_expr(ctx, callback_expr);
    let callback = lower_expr(ctx, callback_expr);
    // The decision between the ref-marker builder and a plain array expression is made from the
    // argument SYNTAX, before either one emits anything, so the callback can be published first.
    let callback = root_descriptor_callback(ctx, callback, PhpType::Mixed, expr.span);
    let arg_array = match descriptor_invoker_ref_marker_array_items(ctx, arg_array_expr, signature.as_ref()) {
        Some(items) => lower_descriptor_invoker_arg_array_for_call_user_func_array(
            ctx,
            &items,
            signature.as_ref(),
            arg_array_expr.span,
        ),
        None => lower_expr(ctx, arg_array_expr),
    };
    Some(emit_callable_descriptor_invoke(ctx, callback, arg_array, expr.span))
}

/// Returns the callable signature available to descriptor-invoker argument lowering.
pub(super) fn callable_descriptor_signature_for_expr(
    ctx: &LoweringContext<'_, '_>,
    callback: &Expr,
) -> Option<FunctionSig> {
    match &callback.kind {
        ExprKind::Ternary { then_expr, else_expr, .. } => {
            let left = callable_descriptor_signature_for_expr(ctx, then_expr)?;
            let right = callable_descriptor_signature_for_expr(ctx, else_expr)?;
            compatible_descriptor_signature(left, &right)
        }
        ExprKind::ShortTernary { value, default } => {
            let left = callable_descriptor_signature_for_expr(ctx, value)?;
            let right = callable_descriptor_signature_for_expr(ctx, default)?;
            compatible_descriptor_signature(left, &right)
        }
        ExprKind::Variable(name) => ctx
            .callable_param_signature(name)
            .cloned()
            .or_else(|| ctx.static_callable_local(name).and_then(|target| {
                signature_for_static_callable_binding(ctx, target)
            })),
        _ => static_callable_binding_for_expr(ctx, callback)
            .and_then(|target| signature_for_static_callable_binding(ctx, target))
            .or_else(|| invokable_object_signature_for_expr(ctx, callback)),
    }
}

/// Returns the `__invoke` signature for an invokable object callback expression.
pub(super) fn invokable_object_signature_for_expr(
    ctx: &LoweringContext<'_, '_>,
    callback: &Expr,
) -> Option<FunctionSig> {
    let class_name = instance_callable_object_class(ctx, callback)?;
    class_method_signature(ctx, &class_name, "__invoke").cloned()
}

/// Keeps a descriptor signature only when two runtime branches have the same callable ABI.
pub(super) fn compatible_descriptor_signature(left: FunctionSig, right: &FunctionSig) -> Option<FunctionSig> {
    (left == *right).then_some(left)
}

/// Extracts a callable signature from a statically understood callable binding.
pub(super) fn signature_for_static_callable_binding(
    ctx: &LoweringContext<'_, '_>,
    target: StaticCallableBinding,
) -> Option<FunctionSig> {
    match target {
        StaticCallableBinding::UserFunction(name) => ctx.functions.get(&name).cloned(),
        StaticCallableBinding::ExternFunction(name) => ctx
            .extern_functions
            .get(&name)
            .map(function_sig_from_extern_for_descriptor),
        StaticCallableBinding::Builtin(_) => None,
        StaticCallableBinding::Closure { signature, .. } => Some(signature),
        StaticCallableBinding::StaticMethod { receiver, method }
        | StaticCallableBinding::StaticMethodDescriptor { receiver, method } => {
            static_method_implementation_signature(ctx, &receiver, &method).cloned()
        }
        StaticCallableBinding::InstanceMethod { signature, .. } => Some(signature),
    }
}

/// Converts an extern signature into the PHP-facing descriptor invoker signature.
pub(super) fn function_sig_from_extern_for_descriptor(sig: &ExternFunctionSig) -> FunctionSig {
    FunctionSig {
        params: sig.params.clone(),
        param_type_exprs: vec![None; sig.params.len()],
        param_attributes: Vec::new(),
        defaults: vec![None; sig.params.len()],
        return_type: sig.return_type.clone(),
        declared_return: true,
        by_ref_return: false,
        ref_params: vec![false; sig.params.len()],
        declared_params: vec![true; sig.params.len()],
        variadic: None,
        deprecation: None,
    }
}

/// Returns the literal `call_user_func_array()` items that need invoker reference markers.
///
/// This is a pure syntactic decision: an array literal with no spread and at least one literal
/// variable bound to a by-reference parameter. Callers consult it BEFORE publishing the callback,
/// so choosing between the marker builder and a plain array expression never abandons emitted
/// instructions.
pub(super) fn descriptor_invoker_ref_marker_array_items(
    ctx: &LoweringContext<'_, '_>,
    arg_array: &Expr,
    sig: Option<&FunctionSig>,
) -> Option<Vec<Expr>> {
    let ExprKind::ArrayLiteral(items) = &arg_array.kind else {
        return None;
    };
    if items.iter().any(is_spread_arg) {
        return None;
    }
    items
        .iter()
        .enumerate()
        .any(|(index, item)| invoker_ref_arg_variable(ctx, sig, index, item).is_some())
        .then(|| items.clone())
}

/// Builds an invoker argument array that preserves by-reference literal variables.
///
/// The array is published for the whole construction and reloaded after every insertion, so a
/// later item expression that throws cannot strand the items already inserted and a growth
/// reallocation is picked up from the slot rather than from the stale `array_new` pointer.
pub(super) fn lower_descriptor_invoker_arg_array_for_call_user_func_array(
    ctx: &mut LoweringContext<'_, '_>,
    items: &[Expr],
    sig: Option<&FunctionSig>,
    span: Span,
) -> LoweredValue {
    let elem_ty = PhpType::Mixed;
    let array_ty = PhpType::Array(Box::new(elem_ty.clone()));
    let array = ctx.emit_value(
        Op::ArrayNew,
        Vec::new(),
        Some(Immediate::Capacity(items.len() as u32)),
        array_ty.clone(),
        Op::ArrayNew.default_effects(),
        Some(span),
    );
    let owner = publish_constructed_container(ctx, array, span);
    for (index, item) in items.iter().enumerate() {
        let value = if let Some(var_name) = invoker_ref_arg_variable(ctx, sig, index, item) {
            lower_invoker_ref_arg_marker(ctx, var_name, item.span)
        } else {
            let value = lower_expr(ctx, item);
            coerce_variadic_tail_value(ctx, value, &array_ty, item.span)
        };
        let array = load_published_container(ctx, owner, array_ty.clone(), item.span);
        ctx.emit_void(
            Op::ArrayPush,
            vec![array.value, value.value],
            None,
            Op::ArrayPush.default_effects(),
            Some(item.span),
        );
        crate::ir_lower::stmt::release_indexed_array_write_operand(ctx, Some(&elem_ty), value, item.span);
    }
    take_published_container(ctx, owner, array_ty, span)
}

/// Returns true when `call_user_func()` must keep runtime descriptor semantics.
pub(super) fn call_user_func_should_use_descriptor(
    ctx: &LoweringContext<'_, '_>,
    callback: &Expr,
    args: &[Expr],
    sig: Option<&FunctionSig>,
) -> bool {
    let has_named_or_spread =
        crate::types::call_args::has_named_args(args) || args.iter().any(is_spread_arg);
    if has_named_or_spread {
        return true;
    }
    if call_user_func_has_incompatible_ref_marker_arg(ctx, args, sig) {
        return false;
    }
    if sig.is_some_and(|sig| sig.ref_params.iter().any(|is_ref| *is_ref)) {
        return true;
    }
    match &callback.kind {
        ExprKind::ArrayLiteral(_)
        | ExprKind::ArrayLiteralAssoc(_)
        | ExprKind::Closure { .. }
        | ExprKind::NewObject { .. }
        | ExprKind::NewDynamicObject { .. }
        | ExprKind::Ternary { .. }
        | ExprKind::ShortTernary { .. }
        | ExprKind::FirstClassCallable(CallableTarget::Method { .. }) => true,
        ExprKind::Variable(name) => {
            if let Some(target) = ctx.static_callable_local(name) {
                return matches!(
                    target,
                    StaticCallableBinding::Closure { .. }
                        | StaticCallableBinding::StaticMethodDescriptor { .. }
                        | StaticCallableBinding::InstanceMethod { .. }
                );
            }
            matches!(
                ctx.local_type(name).codegen_repr(),
                PhpType::Callable | PhpType::Array(_) | PhpType::Object(_)
            )
        }
        _ => false,
    }
}

/// Returns true when direct descriptor ref markers cannot represent an argument.
pub(super) fn call_user_func_has_incompatible_ref_marker_arg(
    ctx: &LoweringContext<'_, '_>,
    args: &[Expr],
    sig: Option<&FunctionSig>,
) -> bool {
    let Some(sig) = sig else {
        return false;
    };
    args.iter().enumerate().any(|(index, arg)| {
        if !sig.ref_params.get(index).copied().unwrap_or(false) {
            return false;
        }
        let ExprKind::Variable(name) = &arg.kind else {
            return false;
        };
        !invoker_ref_arg_storage_compatible(ctx, sig, index, name)
    })
}

/// Lowers `call_user_func()` into a descriptor invoke, reusing the evaluated callback.
///
/// The callback expression is lowered exactly once. A storage shape with no descriptor arm does
/// not decline here, because the caller would then re-lower the same expression; it reuses the
/// value it already has through `lower_call_user_func_from_lowered_callback`.
pub(super) fn lower_call_user_func_descriptor_invoke(
    ctx: &mut LoweringContext<'_, '_>,
    callback_expr: &Expr,
    args: &[Expr],
    sig: Option<&FunctionSig>,
    expr: &Expr,
) -> Option<LoweredValue> {
    let callback = lower_expr(ctx, callback_expr);
    Some(lower_call_user_func_from_lowered_callback(ctx, callback, args, sig, expr))
}

/// Emits `CallableDescriptorInvoke` for an already evaluated `call_user_func()` callback.
///
/// The result type is resolved before the callback is published, because the result staging the
/// invocation needs is the OUTERMOST record of this call and has to be declared with the exact
/// type the invocation produces.
pub(super) fn lower_call_user_func_descriptor_invoke_from_value(
    ctx: &mut LoweringContext<'_, '_>,
    callback: LoweredValue,
    args: &[Expr],
    sig: Option<&FunctionSig>,
    expr: &Expr,
) -> LoweredValue {
    let result_type = sig
        .map(|sig| normalize_value_php_type(sig.return_type.codegen_repr()))
        .unwrap_or(PhpType::Mixed);
    let callback = root_descriptor_callback(ctx, callback, result_type, expr.span);
    let arg_container =
        lower_descriptor_invoker_arg_container_for_call_user_func(ctx, args, sig, expr.span);
    emit_callable_descriptor_invoke(ctx, callback, arg_container, expr.span)
}

/// A descriptor callback already published in the unwind chain, before its arguments were lowered.
///
/// Building the argument container runs PHP expressions that can throw, and a freshly evaluated
/// callback, such as a computed function name or a callable array holding a `new` receiver,
/// is owned by nothing else while they do.
///
/// The invocation's result staging is published here too, one level FURTHER OUT, because the
/// callback and container records are retired while that result is still only an SSA temporary.
pub(super) struct RootedDescriptorCallback {
    /// Operand the invocation passes, which is the published lease when one was taken.
    value: LoweredValue,
    /// Owner slot to retire after the invocation, absent for a borrowed callback.
    owner: Option<crate::ir::LocalSlotId>,
    /// Result type the invocation must be emitted with, and the staging declared for it.
    result_type: PhpType,
    /// Staging holding the owned result while the callback and container records retire.
    result: Option<PrepublishedCallResult>,
}

/// Publishes a descriptor invocation's result staging and then its callback.
///
/// A borrowed callback, such as a plain local load, owns nothing and is handed through
/// unchanged, which keeps the backend's view of the callback's identity exactly as it was.
/// Taking `result_type` here rather than at the invocation is what guarantees the staged slot
/// and the emitted call agree on one storage type.
pub(super) fn root_descriptor_callback(
    ctx: &mut LoweringContext<'_, '_>,
    callback: LoweredValue,
    result_type: PhpType,
    span: Span,
) -> RootedDescriptorCallback {
    let result = prepublish_call_result(ctx, &result_type, span);
    let (value, owner) = root_owned_call_operand(ctx, callback, span);
    RootedDescriptorCallback { value, owner, result_type, result }
}

/// Roots the temporary container owner across a descriptor call and retires every owner on return.
///
/// The records nest strictly: result staging OUTSIDE the callback, callback OUTSIDE the argument
/// container. They are therefore retired container first, callback next and result last, which is
/// the only order the runtime's LIFO pop can express.
pub(super) fn emit_callable_descriptor_invoke(
    ctx: &mut LoweringContext<'_, '_>,
    callback: RootedDescriptorCallback,
    arg_container: LoweredValue,
    span: Span,
) -> LoweredValue {
    let RootedDescriptorCallback {
        value: callback,
        owner: callback_owner,
        result_type,
        result: result_staging,
    } = callback;
    // The backend borrows this container and owns either its normalized copy
    // or a separate retain of a prebuilt Mixed box. Root both raw and boxed
    // owners so a throw cannot bypass their EIR retirement.
    let (arg_container, container_owner) = root_owned_call_operand(ctx, arg_container, span);
    let result = ctx.emit_value(
        Op::CallableDescriptorInvoke,
        vec![callback.value, arg_container.value],
        callable_profile_immediate(),
        result_type,
        Op::CallableDescriptorInvoke.default_effects(),
        Some(span),
    );
    // Retiring the container or the callback destroys a captured object or an argument the
    // container still owns, and those destructors run PHP code that can throw into a catch in
    // this same frame. The result is already staged when they do.
    stage_call_result(ctx, result_staging.as_ref(), result, span);
    if let Some(slot) = container_owner {
        retire_owned_call_operand(ctx, slot, span);
    } else if ctx.value_is_owning_temporary(arg_container) {
        crate::ir_lower::ownership::release_if_owned(ctx, arg_container, Some(span));
    }
    if let Some(slot) = callback_owner {
        retire_owned_call_operand(ctx, slot, span);
    }
    take_prepublished_call_result(ctx, result_staging, result, span)
}
