//! Purpose:
//! Callable descriptor invocation and signature resolution.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.
//! - Descriptor calls carry physical-source strictness separately from their PHP arguments.

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
    let guard_description = super::closure_calls::instance_callable_guard_description(ctx, &callback);
    let callback = lower_expr(ctx, callback_expr);
    Some(emit_rooted_expr_call(
        ctx,
        callback,
        signature.as_ref(),
        callback_args,
        result_type,
        expr.span,
        guard_description,
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
    guard_description: Option<String>,
) -> LoweredValue {
    let result_staging = prepublish_call_result(ctx, &result_type, span);
    let (callback, callback_owner) = root_owned_call_operand(ctx, callback, span);
    let mut operands = vec![callback.value];
    let reference_property_guard_mark = ctx.reference_property_guards.len();
    // A checked bound method lowers its arguments in an argument-evaluation scope, as the
    // ordinary bound-method call does: each owned argument is rooted while a later one is
    // evaluated or checked, so a refused argument's `TypeError` releases them.
    let checked = guard_description.is_some();
    if checked {
        begin_call_argument_evaluation(ctx);
    }
    let mut arg_values =
        super::closure_calls::lower_bound_method_args(ctx, signature, args, guard_description, span);
    let evaluation_intermediates = if checked {
        finish_call_argument_evaluation(ctx, &mut arg_values)
    } else {
        Vec::new()
    };
    operands.extend(arg_values);
    let mut conversion_roots = root_unpublished_argument_conversions(ctx, &mut operands[1..], span);
    // `ExprCall` borrows its arguments, so an owned temporary the argument scope handed to the
    // call (a list rebuilt as a hash, a fresh array) is rooted here and released after it.
    if checked {
        for operand in operands[1..].iter_mut() {
            let value = LoweredValue { value: *operand, ir_type: ctx.builder.value_type(*operand) };
            if !ctx.value_is_owning_temporary(value) {
                continue;
            }
            let (rooted, slot) = root_owned_call_operand(ctx, value, span);
            *operand = rooted.value;
            conversion_roots.extend(slot);
        }
    }
    // `ExprCall` can dispatch a first-class `__construct` callable; publish the invocation
    // site's `strict_types` after the arguments are evaluated.
    super::object_construction::publish_call_strictness(ctx, span);
    let call = ctx.emit_value(
        Op::ExprCall,
        operands,
        callable_profile_immediate(),
        result_type,
        Op::ExprCall.default_effects(),
        Some(span),
    );
    stage_call_result(ctx, result_staging.as_ref(), call, span);
    for slot in conversion_roots.into_iter().rev() {
        retire_owned_call_operand(ctx, slot, span);
    }
    retire_call_argument_intermediates(ctx, &evaluation_intermediates);
    super::enum_argument_guards::drain_reference_property_guards(
        ctx,
        reference_property_guard_mark,
        span,
    );
    if let Some(slot) = callback_owner {
        retire_owned_call_operand(ctx, slot, span);
    }
    take_prepublished_call_result(ctx, result_staging, call, span)
}

/// Emits a call known to return through the descriptor invoker's owned Mixed box.
///
/// General string-producing call opcodes stay `MaybeOwned` because their result can borrow
/// scratch or argument storage. The descriptor ABI is narrower: concrete string unboxing
/// detaches the buffer before consuming the result box, so this producer owns that buffer.
pub(super) fn emit_descriptor_invoker_value(
    ctx: &mut LoweringContext<'_, '_>,
    op: Op,
    operands: Vec<crate::ir::ValueId>,
    immediate: Option<Immediate>,
    result_type: PhpType,
    span: Span,
) -> LoweredValue {
    let result = ctx.emit_value(
        op,
        operands,
        immediate,
        result_type,
        op.default_effects(),
        Some(span),
    );
    if ctx.builder.value_php_type(result.value).codegen_repr() == PhpType::Str {
        ctx.builder
            .set_value_ownership(result.value, Ownership::Owned);
    }
    result
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
        return lower_call_user_func_descriptor_invoke_from_value(ctx, callback, args, sig, expr, true);
    }
    if !crate::types::call_args::has_named_args(args) && !args.iter().any(is_spread_arg) {
        return emit_rooted_expr_call(ctx, callback, None, args, PhpType::Mixed, expr.span, None);
    }
    let callback = coerce_descriptor_invoker_mixed_value(ctx, callback, expr.span);
    lower_call_user_func_descriptor_invoke_from_value(ctx, callback, args, sig, expr, true)
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
    let result_type = descriptor_invoker_result_type(signature.as_ref());
    let callback = root_descriptor_callback(ctx, callback, result_type, expr.span);
    let arg_array = match descriptor_invoker_ref_marker_array_items(arg_array_expr) {
        Some(items) => lower_descriptor_invoker_arg_array_for_call_user_func_array(
            ctx,
            &items,
            signature.as_ref(),
            arg_array_expr.span,
        ),
        None if matches!(arg_array_expr.kind, ExprKind::ArrayLiteralAssoc(_)) => {
            let ExprKind::ArrayLiteralAssoc(pairs) = &arg_array_expr.kind else { unreachable!() };
            lower_descriptor_invoker_assoc_arg_array(ctx, pairs, arg_array_expr)
        }
        None => lower_expr(ctx, arg_array_expr),
    };
    Some(emit_callable_descriptor_invoke(ctx, callback, arg_array, true, expr.span))
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

/// Returns the concrete result storage carried by a known descriptor signature.
///
/// Keeping the fallback boxed is essential for runtime-opaque callbacks. Once the checker has
/// propagated a target signature, however, emitting `Mixed` would force a later return-boundary
/// cast to detach refcounted payloads without the exact ownership metadata the descriptor ABI
/// provides for the concrete result.
pub(super) fn descriptor_invoker_result_type(sig: Option<&FunctionSig>) -> PhpType {
    sig.map(|sig| normalize_value_php_type(crate::types::eir_value_php_type(&sig.return_type)))
        .unwrap_or(PhpType::Mixed)
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
        is_generator: false,
    }
}

/// Returns the literal `call_user_func_array()` items the published builder must construct.
///
/// This is a pure syntactic decision; a spread-free array literal; taken BEFORE the callback is
/// published, so choosing between the builder and a plain array expression never abandons emitted
/// instructions.
///
/// It deliberately does NOT require a by-reference item. The builder is what publishes the
/// container in the operand-owner chain and reloads it after every insertion, so a later item
/// expression that throws cannot strand the items already inserted. Tying that to reference
/// markers meant the decision moved with the CALLEE's signature: the checker back-propagates a
/// bound closure's signature onto a `callable` parameter
/// (`Checker::register_bound_callable_param_sig`), every parameter is then by value, and
/// `call_user_func_array($callback, [$c, new Marker(), failing()])` fell back to an unrooted
/// `array_new` literal that leaked its whole partial contents on the throw. Marker preservation
/// is unaffected; the builder still asks `invoker_ref_arg_variable` per item.
pub(super) fn descriptor_invoker_ref_marker_array_items(
    arg_array: &Expr,
) -> Option<Vec<Expr>> {
    let ExprKind::ArrayLiteral(items) = &arg_array.kind else {
        return None;
    };
    if items.iter().any(is_spread_arg) {
        return None;
    }
    Some(items.clone())
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
        let reference_source = match &item.kind {
            ExprKind::Reference(inner) => Some(inner.as_ref()),
            _ => None,
        };
        let value_expr = reference_source.unwrap_or(item);
        let value = if reference_source.is_some() {
            lower_signature_invoker_ref_arg_marker(ctx, sig, index, value_expr, false)
                .unwrap_or_else(|| {
                    // An inline `[&$x]` marker binds the pushed entry to `$x`, exactly like the
                    // ordinary array-literal builder; lower_expr alone would drop the marker.
                    ctx.begin_quiet_undefined_reads(value_expr);
                    let value = super::assoc_array_literals::lower_reference_literal_value(ctx, value_expr)
                        .unwrap_or_else(|| lower_expr(ctx, value_expr));
                    ctx.end_quiet_undefined_reads();
                    coerce_variadic_tail_value(ctx, value, &array_ty, item.span)
                })
        } else {
            // A plain entry is a value copy. PHP warns and uses a temporary for a
            // by-reference parameter; only `[&$x]` binds the caller's cell.
            let value = lower_expr(ctx, value_expr);
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
        if let Some(source) = reference_source {
            let key = ctx.emit_value(
                Op::ConstI64,
                Vec::new(),
                Some(Immediate::I64(index as i64)),
                PhpType::Int,
                Op::ConstI64.default_effects(),
                Some(item.span),
            );
            let array = load_published_container(ctx, owner, array_ty.clone(), item.span);
            let cell = ctx.emit_value(
                Op::LoadArrayElemRefCell,
                vec![array.value, key.value],
                None,
                PhpType::Pointer(None),
                Op::LoadArrayElemRefCell.default_effects(),
                Some(item.span),
            );
            super::assoc_array_literals::bind_reference_literal_source(
                ctx,
                source,
                cell,
                PhpType::Mixed,
                item.span,
            );
        }
        ctx.refresh_argument_array_guard(array, item.span);
        crate::ir_lower::stmt::release_indexed_array_write_operand(ctx, Some(&elem_ty), value, item.span);
    }
    take_published_container(ctx, owner, array_ty, span)
}

/// Publishes an associative callback argument array before evaluating values that can throw.
fn lower_descriptor_invoker_assoc_arg_array(
    ctx: &mut LoweringContext<'_, '_>,
    pairs: &[(Expr, Expr)],
    expr: &Expr,
) -> LoweredValue {
    let hash_ty = assoc_array_literal_type_for_ir(ctx, pairs, expr);
    let hash = ctx.emit_value(
        Op::HashNew,
        Vec::new(),
        Some(Immediate::Capacity(pairs.len() as u32)),
        hash_ty.clone(),
        Op::HashNew.default_effects(),
        Some(expr.span),
    );
    let owner = publish_constructed_container(ctx, hash, expr.span);
    for (key, value) in pairs {
        let entry_span = value.span;
        let reference_source = match &value.kind {
            ExprKind::Reference(inner) => Some(inner.as_ref()),
            _ => None,
        };
        let value_expr = reference_source.unwrap_or(value);
        let key = lower_expr(ctx, key);
        let value = if reference_source.is_some() {
            ctx.begin_quiet_undefined_reads(value_expr);
            let value = super::assoc_array_literals::lower_reference_literal_value(ctx, value_expr)
                .unwrap_or_else(|| lower_expr(ctx, value_expr));
            ctx.end_quiet_undefined_reads();
            value
        } else {
            lower_expr(ctx, value_expr)
        };
        let hash = load_published_container(ctx, owner, hash_ty.clone(), entry_span);
        ctx.emit_void(
            Op::HashSet,
            vec![hash.value, key.value, value.value],
            None,
            Op::HashSet.default_effects(),
            Some(entry_span),
        );
        if let Some(source) = reference_source {
            super::assoc_array_literals::bind_reference_literal_entry(
                ctx,
                hash.value,
                key.value,
                source,
                value_expr.span,
            );
        }
        ctx.refresh_argument_array_guard(hash, entry_span);
    }
    take_published_container(ctx, owner, hash_ty, expr.span)
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
    // A callback with by-reference parameters still goes through the descriptor invoker:
    // `call_user_func*` binds every argument by value, so the invoker materializes a
    // temporary cell instead of requiring caller storage to match the parameter layout.
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

/// Emits `CallableDescriptorInvoke` for an already evaluated descriptor callback.
///
/// `warn_by_ref` selects the explicit `call_user_func*` by-value contract. Direct dynamic
/// callable and dynamic method invocations pass `false`, so a by-reference argument keeps the
/// caller's cell instead of being copied into a temporary.
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
    warn_by_ref: bool,
) -> LoweredValue {
    let result_type = descriptor_invoker_result_type(sig);
    let mut callback = root_descriptor_callback(ctx, callback, result_type, expr.span);
    let native_references = !matches!(&expr.kind, ExprKind::FunctionCall { name, .. }
        if matches!(
            php_symbol_key(name.as_str().trim_start_matches('\\')).as_str(),
            "call_user_func" | "call_user_func_array"
        ));
    let arg_container = if native_references
        && args.iter().any(|arg| may_be_native_selection_reference_argument(ctx, arg))
    {
        lower_untyped_descriptor_invoker_arg_container_for_callable(ctx, &mut callback, args, expr.span)
    } else {
        lower_descriptor_invoker_arg_container_for_call_user_func(ctx, args, sig, native_references, expr.span)
    };
    emit_callable_descriptor_invoke(ctx, callback, arg_container, warn_by_ref, expr.span)
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
    pub(super) value: LoweredValue,
    /// Owner slot to retire after the invocation, absent for a borrowed callback.
    owner: Option<crate::ir::LocalSlotId>,
    /// Result type the invocation must be emitted with, and the staging declared for it.
    result_type: PhpType,
    /// Staging holding the owned result while the callback and container records retire.
    result: Option<PrepublishedCallResult>,
    /// Selection cells published outside the argument container and retired after dispatch.
    pub(super) selection_aliases: Vec<(String, Span)>,
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
    RootedDescriptorCallback { value, owner, result_type, result, selection_aliases: Vec::new() }
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
    warn_by_ref: bool,
    span: Span,
) -> LoweredValue {
    let RootedDescriptorCallback {
        value: callback,
        owner: callback_owner,
        result_type,
        result: result_staging,
        selection_aliases,
    } = callback;
    // The backend borrows this container and owns either its normalized copy
    // or a separate retain of a prebuilt Mixed box. Root both raw and boxed
    // owners so a throw cannot bypass their EIR retirement.
    let (arg_container, container_owner) = root_owned_call_operand(ctx, arg_container, span);
    // The descriptor may target a first-class `__construct` callable; php-src binds the
    // invocation site's `strict_types`, so publish it once the arguments are evaluated.
    super::object_construction::publish_call_strictness(ctx, span);
    let strict_types = ctx.builder.emit_const_bool(crate::source::current_strict_types());
    let result = emit_descriptor_invoker_value(
        ctx,
        Op::CallableDescriptorInvoke,
        vec![callback.value, arg_container.value, strict_types],
        callable_invoke_immediate(warn_by_ref),
        result_type,
        span,
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
    for (alias, alias_span) in selection_aliases.into_iter().rev() {
        let slot = ctx.ref_cell_owner_slot(&alias).expect("published selection cell");
        unregister_owned_call_operand(ctx, slot, alias_span);
        ctx.release_ref_cell_owner(&alias, Some(alias_span));
    }
    if let Some(slot) = callback_owner {
        retire_owned_call_operand(ctx, slot, span);
    }
    take_prepublished_call_result(ctx, result_staging, result, span)
}
