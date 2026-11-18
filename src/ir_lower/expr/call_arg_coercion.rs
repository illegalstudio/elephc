//! Purpose:
//! General call argument lowering and parameter storage coercion.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;
use crate::ir_lower::context::{DeferredObjectParamCheck, ReferencePropertyGuard};

/// Lowers positional/named/spread call arguments in source order.
pub(super) fn lower_args(ctx: &mut LoweringContext<'_, '_>, args: &[Expr]) -> Vec<crate::ir::ValueId> {
    args.iter()
        .enumerate()
        .map(|(index, arg)| {
            let value = lower_expr(ctx, arg);
            if index + 1 < args.len() {
                root_evaluated_call_argument(ctx, value, arg.span).value
            } else {
                value.value
            }
        })
        .collect()
}

/// Lowers one argument while applying by-reference storage normalization from a signature.
pub(super) fn lower_arg_with_signature(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    arg: &Expr,
) -> crate::ir::ValueId {
    let value = lower_signature_argument(ctx, sig, index, arg);
    if sig.ref_params.get(index).copied().unwrap_or(false) {
        root_reference_value_view(ctx, value, arg.span);
    }
    value
}

/// Normalizes one argument while allowing recursive rewrites of addressable places.
fn lower_signature_argument(
    ctx: &mut LoweringContext<'_, '_>, sig: &FunctionSig, index: usize, arg: &Expr,
) -> crate::ir::ValueId {
    if sig.ref_params.get(index).copied().unwrap_or(false)
        && index < crate::types::call_args::regular_param_count(sig)
        && ctx.call_argument_evaluation_scopes.last().is_some_and(|scope|
            scope.live_property_reference_arguments && scope.expression_depth == ctx.expression_depth)
        && sig.params.get(index).is_some_and(|(_, ty)|
            super::ref_place_args::live_mixed_property_argument(ctx, arg, ty))
    {
        let alias = ctx.declare_synthetic_php_local(PhpType::Mixed);
        let owner = ctx.declare_ref_cell_owner(&alias, PhpType::Mixed);
        ctx.builder.mark_reference_argument_owner(owner);
        // Receiver cleanup can throw after the cell binds, including into a same-frame catch.
        publish_scoped_ref_receiver_alias(ctx, &alias, arg.span);
        lower_ref_assign_property(ctx, &alias, arg, arg.span);
        let variable = Expr::new(ExprKind::Variable(alias), arg.span);
        return lower_arg_with_signature(ctx, sig, index, &variable);
    }
    if sig.ref_params.get(index).copied().unwrap_or(false)
        && is_native_selection_reference_argument(ctx, arg)
    {
        let alias = stage_native_selection_reference(ctx, arg);
        let variable = Expr::new(ExprKind::Variable(alias), arg.span);
        return lower_arg_with_signature(ctx, sig, index, &variable);
    }
    // Named-argument wrappers are planner metadata, and `@` only changes diagnostics: the
    // storage place underneath is what the by-reference property and array-element rewrites
    // must see, while the argument itself still has to be lowered with suppression intact.
    let (place_arg, suppressed) = ref_argument_place(arg);
    let mut guarded_place: Option<Expr> = None;
    // PHP checks a property reference against a declared by-reference parameter at the call
    // boundary, after evaluating the place, and raises its argument `TypeError` before the
    // callee body runs. The checker recorded the declared parameter type, so read the property
    // once, test its CURRENT value strictly, and only continue to the ordinary copy-in/out
    // argument path when that value satisfies the parameter. A declared union may hold a
    // compatible member even though its static type also includes incompatible ones.
    if sig.ref_params.get(index).copied().unwrap_or(false) {
        let throw_site = ctx
            .throw_access_sites
            .get(&(ctx.loop_storage_scope.clone(), place_arg.span))
            .cloned();
        if let Some(info) = throw_site {
            if let ThrowAccessKind::ReferenceArgumentType {
                prefix,
                expected,
                conditional,
                coerces_weakly,
                given,
            } = &info.kind
            {
                if !*conditional {
                    // A concrete property type that cannot satisfy the parameter has no
                    // compatible current value. Read the place for its side effects now, but
                    // record the throw for the call's post-evaluation pass: PHP runs every
                    // later argument first. The pointer placeholder only satisfies EIR typing.
                    let read = lower_expr(ctx, place_arg);
                    crate::ir_lower::ownership::release_if_owned(
                        ctx,
                        read,
                        Some(place_arg.span),
                    );
                    let guard = crate::ir_lower::context::PendingPropertyTypeGuard {
                        place: None,
                        prefix: prefix.clone(),
                        expected: expected.clone(),
                        coerces_weakly: false,
                        temp: None,
                        callee_temp: None,
                        callee_temp_ty: None,
                        span: place_arg.span,
                        unconditional_message: Some(format!("{prefix}{given} given")),
                    };
                    if !defer_property_type_guard(ctx, guard.clone()) {
                        emit_property_type_guard(ctx, &guard);
                    }
                    return ctx
                        .emit_value(
                            Op::ConstI64,
                            Vec::new(),
                            Some(Immediate::I64(0)),
                            PhpType::Pointer(None),
                            Op::ConstI64.default_effects(),
                            Some(place_arg.span),
                        )
                        .value;
                }
                // Stabilize the receiver once, so the call-boundary re-read cannot repeat
                // a side effect from a nested receiver expression.
                let place = super::ref_place_args::stabilize_property_receiver_place(ctx, place_arg);
                guarded_place = Some(place.clone());
                let read = lower_expr(ctx, &place);
                // The snapshot is boxed: a later argument may replace the property with a
                // value of another type, which is exactly what the call-boundary check
                // must see.
                let temp = ctx.declare_synthetic_php_local(PhpType::Mixed);
                store_value_into_temp(ctx, &temp, PhpType::Mixed, read, place_arg.span);
                let guard = crate::ir_lower::context::PendingPropertyTypeGuard {
                    place: Some(place),
                    prefix: prefix.clone(),
                    expected: expected.clone(),
                    coerces_weakly: *coerces_weakly,
                    temp: Some(temp),
                    callee_temp: None,
                    callee_temp_ty: None,
                    span: place_arg.span,
                    unconditional_message: None,
                };
                if !defer_property_type_guard(ctx, guard.clone()) {
                    emit_property_type_guard(ctx, &guard);
                }
            }
        }
    }
    if sig.ref_params.get(index).copied().unwrap_or(false)
        && matches!(&place_arg.kind, ExprKind::ArrayAccess { array, .. }
            if instance_object_class_for_ir(ctx, array)
                .is_some_and(|class| class.eq_ignore_ascii_case("WeakMap")))
    {
        let alias = ctx.declare_synthetic_php_local(PhpType::Mixed);
        lower_ref_assign_array_elem(ctx, &alias, place_arg, place_arg.span);
        publish_scoped_ref_receiver_alias(ctx, &alias, place_arg.span);
        let variable = Expr::new(ExprKind::Variable(alias), place_arg.span);
        return lower_arg_with_signature(ctx, sig, index, &variable);
    }
    if sig.ref_params.get(index).copied().unwrap_or(false)
        && matches!(
            &place_arg.kind,
            ExprKind::ArrayAccess { array, .. }
                if !matches!(&array.kind, ExprKind::Variable(_))
        )
    {
        if let Some((alias, aliases)) = prepare_scoped_addressable_ref_array_receiver(ctx, place_arg) {
            let value = lower_arg_with_signature(ctx, sig, index, &alias);
            retire_scoped_ref_receiver_aliases(ctx, &aliases);
            return value;
        }
    }
    if sig.ref_params.get(index).copied().unwrap_or(false)
        && property_argument_is_declared(ctx, place_arg)
    {
        // A property's slot stores the property's own representation, so the cell cannot be
        // shared with a `mixed` parameter. Evaluate the receiver once, hand the callee a
        // temporary whose storage matches the parameter (boxed `Mixed` for mixed/union
        // parameters), and write the temporary back through the typed property store.
        let place = guarded_place
            .take()
            .unwrap_or_else(|| super::ref_place_args::stabilize_property_receiver_place(ctx, place_arg));
        // PHP binds the property for WRITE: a null receiver raises its modification Error
        // before the callee, and only then is the current value checked against the
        // parameter. Select that receiver in write context so intermediate null hops do not
        // degrade into read warnings.
        if let ExprKind::PropertyAccess { object, property } = &place.kind {
            let receiver = super::property_access::write_select::lower_property_write_receiver(
                ctx,
                object,
            );
            crate::ir_lower::stmt::property_write_receiver::guard_write_receiver_not_null(
                ctx,
                receiver,
                property,
                place_arg.span,
            );
            crate::ir_lower::ownership::release_if_owned(ctx, receiver, Some(place_arg.span));
        }
        let read_target = if suppressed {
            Expr::new(ExprKind::ErrorSuppress(Box::new(place.clone())), arg.span)
        } else {
            place.clone()
        };
        let read = lower_expr(ctx, &read_target);
        // An untyped parameter is represented as the argument's inferred type in
        // `sig.params`, but PHP gives the callee a boxed Mixed cell for it. Use Mixed so a
        // concrete property is staged through the boxed-temporary write-back path.
        let param_ty = if sig.declared_params.get(index).copied().unwrap_or(true) {
            sig.params
                .get(index)
                .map(|(_, ty)| ty.codegen_repr())
                .unwrap_or(PhpType::Mixed)
        } else {
            PhpType::Mixed
        };
        let temp_ty = match param_ty {
            PhpType::Mixed | PhpType::Union(_) => PhpType::Mixed,
            concrete => concrete,
        };
        let temp = ctx.declare_synthetic_php_local(temp_ty.clone());
        // The call-boundary guard must refresh the very temporary the callee receives:
        // PHP binds the live property, so a later argument's write is what the callee sees.
        if let Some(scope) = ctx.call_argument_evaluation_scopes.last_mut() {
            if let Some(guard) = scope
                .pending_property_type_guards
                .iter_mut()
                .rev()
                .find(|guard| guard.span == place_arg.span && guard.callee_temp.is_none())
            {
                guard.callee_temp = Some(temp.clone());
                guard.callee_temp_ty = Some(temp_ty.clone());
            }
        }
        // The property may hold a wider value than the parameter representation (an untyped
        // slot coerced to `int`, `string` or a class). Convert before publishing the
        // temporary the callee and the write-back share. A bool target needs PHP truthiness,
        // not the merge-temp integer cast: `"x"` is true and `"0.0"` is false.
        let read = if temp_ty.codegen_repr() == PhpType::Bool {
            super::unary_logic::lower_truthy_bool(ctx, read, Some(place_arg.span))
        } else {
            super::merge_temps::coerce_value_for_temp(ctx, read, &temp_ty, place_arg.span)
        };
        ctx.store_local(&temp, read, temp_ty, Some(place_arg.span));
        let variable = Expr::new(ExprKind::Variable(temp.clone()), place_arg.span);
        let value = lower_arg_with_signature(ctx, sig, index, &variable);
        let details = match &place.kind {
            ExprKind::PropertyAccess { object, property } => receiver_class_name(ctx, object)
                .map(|class| {
                    let (class_name, declared) =
                        match declared_property_type(ctx, &class, property) {
                            Some((declaring, ty)) => (declaring, Some(ty)),
                            None => (class, None),
                        };
                    (class_name, property.clone(), declared)
                }),
            _ => None,
        };
        if let Some((class_name, property, declared)) = details {
            ctx.reference_property_guards.push(ReferencePropertyGuard {
                temp,
                place,
                class_name,
                property,
                declared,
            });
        }
        return value;
    }
    if let Some(value) = lower_by_ref_array_element_arg_with_signature(ctx, sig, index, place_arg) {
        return value;
    }
    if let Some(value) = lower_by_ref_array_arg_with_signature(ctx, sig, index, place_arg) {
        return value;
    }
    guard_boxed_object_reference_argument(ctx, sig, index, place_arg);
    promote_boxed_reference_local_argument(ctx, sig, index, place_arg);
    promote_reference_return_local_argument(ctx, sig, index, place_arg);
    // A property by-reference rewrite stages `$temp = <place>` as the argument so the read
    // happens at this argument's source position. Execute that assignment now, then pass the
    // hidden local's storage exactly like an ordinary variable argument.
    if sig.ref_params.get(index).copied().unwrap_or(false) {
        if let ExprKind::Assignment { target, .. } = &place_arg.kind {
            if let ExprKind::Variable(name) = &target.kind {
                if name.starts_with(crate::names::SYNTHETIC_PLACE_LOCAL_STEM) {
                    let assigned = lower_expr(ctx, arg);
                    if ctx.value_is_owning_temporary(assigned) {
                        crate::ir_lower::ownership::release_if_owned(ctx, assigned, Some(arg.span));
                    }
                    let variable = Expr::new(ExprKind::Variable(name.clone()), place_arg.span);
                    return lower_arg_with_signature(ctx, sig, index, &variable);
                }
            }
        }
    }
    if let Some(lowered) = lower_tracked_callable_array_param(ctx, sig, index, place_arg) {
        return lowered.value;
    }
    let reference_local = sig.ref_params.get(index).copied().unwrap_or(false)
        .then(|| local_values::lower_reference_local(ctx, place_arg)).flatten();
    let lowered = reference_local.unwrap_or_else(|| match sig.params.get(index) {
        Some((_, param_ty)) => lower_expr_for_parameter(
            ctx, arg, param_ty, super::integer_destinations::parameter_is_source_typed(sig, index),
        ),
        None => lower_expr(ctx, arg),
    });
    record_pointer_reference_argument(ctx, sig, index, arg);
    coerce_scalar_arg_to_param_storage(ctx, sig, index, lowered, arg).value
}

/// Shares the checker's XML selection classification before any argument is evaluated.
pub(super) fn is_native_selection_reference_argument(
    ctx: &LoweringContext<'_, '_>, arg: &Expr,
) -> bool {
    if let ExprKind::NamedArg { value, .. } = &arg.kind {
        return is_native_selection_reference_argument(ctx, value);
    }
    let Some(receiver) = crate::types::param_binding::native_selection_reference_receiver(arg) else {
        return false;
    };
    let source = super::array_cast_source::array_cast_source_type_for_ir(ctx, receiver);
    let checked = ctx.member_receiver_types.get(&(ctx.loop_storage_scope.clone(), arg.span))
        .and_then(Option::as_ref).unwrap_or(&source);
    crate::types::param_binding::native_selection_reference_receiver_type(checked, ctx.classes)
        || is_native_selection_reference_argument(ctx, receiver)
}

/// Includes erased receivers whose native family must be selected at the argument's runtime position.
pub(super) fn may_be_native_selection_reference_argument(ctx: &LoweringContext<'_, '_>, arg: &Expr) -> bool {
    if let ExprKind::NamedArg { value, .. } = &arg.kind {
        return may_be_native_selection_reference_argument(ctx, value);
    }
    if is_native_selection_reference_argument(ctx, arg) { return true; }
    if !ctx.classes.contains_key("SimpleXMLElement") { return false; }
    let Some(receiver) = crate::types::param_binding::native_selection_reference_receiver(arg) else { return false; };
    let ty = super::array_cast_source::array_cast_source_type_for_ir(ctx, receiver);
    matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
        || singular_object_class(&ty).is_some_and(|(class, _)| class.is_empty() || ctx.interfaces.contains_key(class))
        || may_be_native_selection_reference_argument(ctx, receiver)
}

/// Moves the selected view into a scoped reference cell so no old-value lease spans the call.
pub(super) fn stage_native_selection_reference(
    ctx: &mut LoweringContext<'_, '_>, arg: &Expr,
) -> String {
    let mut selection = arg;
    while let ExprKind::NamedArg { value, .. } = &selection.kind { selection = value; }
    let lowered = property_access::lower_property_reference_receiver(ctx, selection);
    let alias = ctx.declare_synthetic_php_local(PhpType::Mixed);
    store_value_into_temp(ctx, &alias, PhpType::Mixed, lowered, arg.span);
    ctx.promote_local_mixed_ref_cell(&alias, Some(arg.span));
    publish_scoped_ref_receiver_alias(ctx, &alias, arg.span);
    alias
}

/// Strips planner named-argument wrappers and `@` from a by-reference argument, reporting
/// whether the original expression was error-suppressed.
fn ref_argument_place(arg: &Expr) -> (&Expr, bool) {
    let mut current = arg;
    let mut suppressed = false;
    loop {
        match &current.kind {
            ExprKind::NamedArg { value, .. } => current = value,
            ExprKind::ErrorSuppress(value) => {
                suppressed = true;
                current = value;
            }
            _ => return (current, suppressed),
        }
    }
}

/// Records a local passed to a pointer by-reference parameter for the final storage check.
fn record_pointer_reference_argument(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    arg: &Expr,
) {
    if !sig.ref_params.get(index).copied().unwrap_or(false) {
        return;
    }
    let Some((_, param_ty)) = sig.params.get(index) else {
        return;
    };
    if !matches!(param_ty.codegen_repr(), PhpType::Pointer(_)) {
        return;
    }
    // `@$x` and `p: $x` still bind `$x`'s own storage.
    let mut place = arg;
    while let ExprKind::NamedArg { value, .. } | ExprKind::ErrorSuppress(value) = &place.kind {
        place = value;
    }
    if let ExprKind::Variable(name) = &place.kind {
        retype_local_for_pointer_reference(ctx, name, param_ty);
        ctx.record_pointer_reference_binding(name, place.span, param_ty);
    }
}

/// Gives a plain frame local bound to a pointer by-reference parameter a pointer-word type.
///
/// The callee reads and writes a raw pointer word through the reference. A local that holds only
/// `null` leaves the call as the parameter's `?ptr<T>` (the callee may store an address, which
/// is also the type the checker records for it), and one that holds a packed record address
/// becomes that record's `ptr<T>`. The retype widens the frame slot to a pointer word, where a
/// `null` already stored is address 0 (`pointer_word_storage` in `crate::ir::builder`). Pointer
/// locals keep their own type, and a global, static or reference-bound name keeps its storage,
/// which `LoweringContext::refuse_pointer_reference_bindings` then refuses.
fn retype_local_for_pointer_reference(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    param_ty: &PhpType,
) {
    if !ctx.local_is_plain_frame_slot(name) {
        return;
    }
    let retyped = match ctx.local_type(name) {
        PhpType::Void | PhpType::Never if crate::types::is_nullable_pointer_type(param_ty) => {
            param_ty.clone()
        }
        PhpType::Packed(class) => PhpType::Pointer(Some(class)),
        _ => return,
    };
    ctx.set_local_type(name, retyped);
}

/// Captures a by-value runtime-parser argument while preserving by-reference places.
pub(super) fn lower_arg_with_signature_options(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    arg: &Expr,
    capture_values: bool,
) -> crate::ir::ValueId {
    if !capture_values {
        return lower_arg_with_signature(ctx, sig, index, arg);
    }
    if sig.ref_params.get(index).copied().unwrap_or(false) {
        promote_captured_reference_argument(ctx, arg);
        return lower_arg_with_signature(ctx, sig, index, arg);
    }
    let lowered = lower_expr(ctx, arg);
    capture_call_argument_value(ctx, lowered, index, arg.span).value
}

/// Promotes a runtime-parser output local at its source-order argument evaluation point.
/// The managed reference retains storage identity without capturing the previous PHP value.
pub(super) fn promote_captured_reference_argument(ctx: &mut LoweringContext<'_, '_>, arg: &Expr) {
    match &arg.kind {
        ExprKind::Variable(name) => {
            let was_ref_bound = ctx.is_ref_bound_local(name);
            ctx.promote_local_mixed_ref_cell(name, Some(arg.span));
            if was_ref_bound {
                // A previous branch can mark the local without promoting every incoming path.
                ctx.promote_local_ref_cell(name, Some(arg.span));
            }
        },
        ExprKind::NamedArg { value, .. } => promote_captured_reference_argument(ctx, value),
        _ => {},
    }
}

/// Prepares a write-only output in source order without changing later pre-call reads.
pub(super) fn prepare_captured_output_argument(
    ctx: &mut LoweringContext<'_, '_>, arg: &Expr, output_type: &PhpType,
) -> Option<(String, PhpType)> {
    let ExprKind::Variable(name) = &arg.kind else { return None; };
    let previous = ctx.local_type(name);
    if ctx.local_slots.get(name).is_some_and(|slot| ctx.slot_is_initialized(*slot)) {
        // Existing aliases already share the final boxed frame representation.
        // Reboxing their payload here would replace that shared cell's value.
        if previous.codegen_repr() != output_type.codegen_repr() {
            ctx.set_local_type(name, PhpType::Mixed);
        }
        Some((name.clone(), previous))
    } else {
        promote_captured_reference_argument(ctx, arg);
        None
    }
}

/// Detaches a mutable boxed cell or retains a heap payload until its by-value call consumes it.
pub(super) fn capture_call_argument_value(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    parameter: usize,
    span: Span,
) -> LoweredValue {
    let ty = ctx.builder.value_php_type(value.value);
    let captured = if matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
        let captured = ctx.emit_owned_value(Op::MixedClone, vec![value.value], None, ty,
            Op::MixedClone.default_effects(), Some(span));
        if ctx.value_is_owning_temporary(value) {
            crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
        }
        captured
    } else {
        let captured = crate::ir_lower::ownership::acquire_lifetime_pin_if_refcounted(ctx, value, Some(span));
        if captured.value != value.value && ctx.value_is_owning_temporary(value) {
            crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
        }
        captured
    };
    ctx.guard_call_argument(captured, parameter, span);
    captured
}

/// Rechecks a declared object reference when an earlier call has changed its boxed payload.
///
/// When the call defers its class checks (`DeferredObjectParamChecks`), a `mixed` local bound
/// to a class-like reference parameter, or to a union of `null`, arrays and objects, joins
/// them instead: PHP checks every parameter in
/// declaration order once all arguments were evaluated, so `f(?T &$a, ?T $b)` must report
/// `$a` first even though `$b` is checked by the caller.
///
/// Otherwise only a non-nullable class parameter is checked here: inside the callee its slot
/// reads an object pointer. A union of `null`, arrays and objects (`?A &$v`, `?array &$v`) is a
/// boxed cell in the callee and is checked on entry instead, after every argument was evaluated
/// (`crate::ir_lower::expr::guard_reference_parameter_entry`).
fn guard_boxed_object_reference_argument(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    arg: &Expr,
) {
    if !sig.ref_params.get(index).copied().unwrap_or(false) {
        return;
    }
    if defer_reference_object_param_check(ctx, sig, index, arg) {
        return;
    }
    let declared_object = match sig.params.get(index).map(|(_, ty)| ty) {
        Some(PhpType::Object(class_name)) => Some(class_name.as_str()),
        Some(PhpType::Mixed) => match sig.param_type_exprs.get(index).and_then(Option::as_ref) {
            Some(TypeExpr::Named(name)) if !name.as_str().eq_ignore_ascii_case("mixed") => {
                Some(name.as_str())
            }
            _ => None,
        },
        _ => None,
    };
    let Some(class_name) = declared_object else {
        return;
    };
    let ExprKind::Variable(name) = &arg.kind else {
        return;
    };
    if ctx.local_type(name).codegen_repr() != PhpType::Mixed {
        return;
    }
    let mut condition = if class_name.is_empty() || class_name.eq_ignore_ascii_case("object") {
        let value = lower_expr(ctx, arg);
        let condition = ctx.emit_value(
            Op::TypePredicate,
            vec![value.value],
            Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::Object)),
            PhpType::Bool,
            Op::TypePredicate.default_effects(),
            Some(arg.span),
        );
        if ctx.value_needs_release_after_use(value) {
            crate::ir_lower::ownership::release_if_owned(ctx, value, Some(arg.span));
        }
        condition
    } else {
        lower_instanceof(
            ctx,
            arg,
            &InstanceOfTarget::Name(Name::from(class_name)),
            arg,
        )
    };
    // Native reads retain a useful object hint even when a missing continuation is null.
    // Let the shared callee entry report that null with its exact PHP callable label;
    // all other invalid reference payloads still take the existing class refusal here.
    if crate::types::builtin_classes::native_object_sink_needs_null_guard(
        &PhpType::Object(class_name.to_string()), ctx.classes, ctx.interfaces,
    ) {
        let value = lower_expr(ctx, arg);
        let null = ctx.emit_value(Op::IsNull, vec![value.value], None, PhpType::Bool,
            Op::IsNull.default_effects(), Some(arg.span));
        if ctx.value_needs_release_after_use(value) {
            crate::ir_lower::ownership::release_if_owned(ctx, value, Some(arg.span));
        }
        condition = ctx.emit_value(Op::IBitOr, vec![condition.value, null.value], None,
            PhpType::Bool, Op::IBitOr.default_effects(), Some(arg.span));
    }
    let accepted = ctx.builder.create_named_block("object.ref.accepted", Vec::new());
    let rejected = ctx.builder.create_named_block("object.ref.rejected", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: condition.value,
        then_target: accepted,
        then_args: Vec::new(),
        else_target: rejected,
        else_args: Vec::new(),
    });
    ctx.builder.position_at_end(rejected);
    let message = Expr::new(
        ExprKind::StringLiteral(format!("Argument must be of type {}", class_name)),
        arg.span,
    );
    let exception = lower_expr(ctx, &Expr::new(
        ExprKind::NewObject {
            class_name: Name::unqualified("TypeError"),
            args: vec![message],
        },
        arg.span,
    ));
    ctx.builder.terminate(Terminator::Throw { value: exception.value });
    ctx.builder.position_at_end(accepted);
}

/// Whether a by-reference parameter of type `ty` takes a boxed cell a promoted local can share:
/// `mixed` (or a declared `callable`, stored the same way), an object, a union of objects only (`A|B`), or a NULLABLE union of objects, arrays
/// and floats (`?Base`, `?array`, `?float`). The checker authorizes promotion only for a whole
/// local it gave boxed storage: one that may receive `null` (or a sibling class) through such a
/// parameter, which its typed slot could not hold.
fn boxed_reference_param_type(ty: &PhpType) -> bool {
    match ty {
        PhpType::Mixed | PhpType::DeclaredCallable | PhpType::Object(_) => true,
        PhpType::Union(members) => {
            let objects_only = members.iter().any(|member| matches!(member, PhpType::Object(_)))
                && members
                    .iter()
                    .all(|member| matches!(member, PhpType::Object(_) | PhpType::Void));
            let nullable_heap_or_float = members.iter().any(|member| matches!(member, PhpType::Void))
                && members.iter().all(|member| {
                    member.is_php_array()
                        || matches!(
                            member,
                            PhpType::Void
                                | PhpType::Object(_)
                                | PhpType::Array(_)
                                | PhpType::AssocArray { .. }
                                | PhpType::Float
                        )
                });
            objects_only || nullable_heap_or_float
        }
        _ => false,
    }
}

/// Gives an eligible whole-local reference the canonical boxed payload required by the callee.
///
/// The checker permits this only for an eligible whole local. An existing object alias already
/// shares a boxed cell, so promotion never relabels a concrete cell behind another alias.
fn promote_boxed_reference_local_argument(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    arg: &Expr,
) {
    if !sig.ref_params.get(index).copied().unwrap_or(false)
        || !sig
            .params
            .get(index)
            .is_some_and(|(_, ty)| boxed_reference_param_type(ty))
    {
        return;
    }
    let ExprKind::Variable(name) = &arg.kind else {
        return;
    };
    if !ctx.boxed_reference_promotion_is_authorized(name, arg.span)
        && ctx.local_type(name) != PhpType::Mixed
        && !constructor_argument_owns_mixed_reference(ctx, index)
    {
        return;
    }
    let owner_needs_promotion = ctx.local_type(name).codegen_repr() == PhpType::Mixed
        && ctx.ref_cell_owner_slot(name).is_some_and(|owner| !ctx.slot_is_initialized(owner));
    if (!ctx.is_ref_bound_local(name) || owner_needs_promotion) && ctx.local_can_own_reference_cell(name) {
        ctx.promote_local_mixed_ref_cell(name, Some(arg.span));
    }
}

/// Applies the selected constructor's storage contract after dynamic class dispatch.
fn constructor_argument_owns_mixed_reference(ctx: &LoweringContext<'_, '_>, index: usize) -> bool {
    let Some((class, method)) = ctx.call_description.as_deref().and_then(|name| name.rsplit_once("::")) else {
        return false;
    };
    if !method.eq_ignore_ascii_case("__construct") { return false; }
    let Some((owner, info)) = crate::types::constructor_owner(ctx.classes, class) else { return false; };
    let implementation = info.method_impl_classes.get("__construct").map(String::as_str).unwrap_or(owner);
    let Some(info) = ctx.classes.get(implementation) else { return false; };
    let Some(property) = info.constructor_param_to_prop.get(index).and_then(Option::as_ref) else { return false; };
    info.visible_property_index(property).is_some_and(|slot|
        info.property_slot_owns_reference(slot) && info.properties[slot].1 == PhpType::Mixed)
}

/// Materializes a proven callable array as the descriptor required by a Callable slot.
///
/// A tracked PHP local remains an ordinary array, and its instance receiver was already captured
/// into a hidden local at assignment. A literal crossing this boundary is consumed directly, so
/// its receiver is evaluated exactly once while constructing the descriptor.
pub(super) fn lower_tracked_callable_array_param(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    arg: &Expr,
) -> Option<LoweredValue> {
    if sig.ref_params.get(index).copied().unwrap_or(false)
        || sig.params.get(index)?.1.codegen_repr() != PhpType::Callable
    {
        return None;
    }
    let target = match static_callable_binding_for_expr(ctx, arg)? {
        StaticCallableBinding::StaticMethodDescriptor { receiver, method } => {
            CallableTarget::StaticMethod { receiver, method }
        }
        StaticCallableBinding::InstanceMethod {
            object,
            method,
            direct_call: true,
            ..
        } => {
            CallableTarget::Method { object, method }
        }
        StaticCallableBinding::UserFunction(_)
        | StaticCallableBinding::ExternFunction(_)
        | StaticCallableBinding::Builtin(_)
        | StaticCallableBinding::Closure { .. }
        | StaticCallableBinding::StaticMethod { .. }
        | StaticCallableBinding::InstanceMethod {
            direct_call: false,
            ..
        } => return None,
    };
    Some(lower_callable_array_value(ctx, &target, arg))
}

/// Gives an escaping by-reference return a managed owner for a caller local.
///
/// A reference-returning callee can hand a by-reference parameter's address back to the caller.
/// A raw caller frame address has no `__rt_reference_cell_owner`, so promote the local before the
/// call and pass a fresh `LoadRefCell` marker that resolves to the managed cell pointer. Array
/// normalization runs first so the cell stores the final parameter-compatible representation.
fn promote_reference_return_local_argument(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    arg: &Expr,
) {
    if !sig.by_ref_return || !sig.ref_params.get(index).copied().unwrap_or(false) {
        return;
    }
    let ExprKind::Variable(name) = &arg.kind else {
        return;
    };
    ctx.promote_local_ref_cell(name, Some(arg.span));
}

/// Protects a reference snapshot once while retaining its original addressable load operand.
fn root_reference_value_view(
    ctx: &mut LoweringContext<'_, '_>, value: crate::ir::ValueId, span: Span,
) {
    if ctx.builder.value_defining_op(value) != Some(Op::LoadRefCellValue) { return; }
    let depth = ctx.expression_depth;
    let Some(scope) = ctx.call_argument_evaluation_scopes.last_mut()
        .filter(|scope| scope.expression_depth == depth) else { return; };
    if !scope.rooted_reference_value_views.insert(value) { return; }
    let lowered = lowered_value_from_id(ctx, value);
    let _protected_view = root_evaluated_call_argument(ctx, lowered, span);
}

/// Protects an incidental value view of an earlier reference place without replacing the place.
///
/// A local whose final frame storage widens to `Mixed` can make its narrower `LoadLocal<Str>`
/// allocate an owned detached string during codegen. The call still needs that exact load as its
/// by-reference place marker, so retain it as the operand and let the evaluation ledger treat the
/// protected value view as an intermediate owner. An exception from a later argument then retires
/// the detached view, while successful call materialization still recovers the original local slot.
fn root_prior_argument_preserving_reference_place(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    arg: &Expr,
    value: crate::ir::ValueId,
) -> crate::ir::ValueId {
    let lowered = lowered_value_from_id(ctx, value);
    if sig.ref_params.get(index).copied().unwrap_or(false) {
        if ctx.builder.value_defining_op(value) == Some(Op::LoadRefCellValue) {
            root_reference_value_view(ctx, value, arg.span);
        } else if matches!(arg.kind, ExprKind::Variable(_))
                && sig.params.get(index).is_some_and(|(_, ty)| ty.codegen_repr() == PhpType::Str)
                && ctx.builder.value_php_type(value).codegen_repr() == PhpType::Str
        {
            let _protected_view = root_evaluated_call_argument(ctx, lowered, arg.span);
        }
        return value;
    }
    root_evaluated_call_argument(ctx, lowered, arg.span).value
}

/// Guards a boxed `Class|false` / `Class|null` argument for an object parameter.
///
/// Returns `None` when the argument already carries the declared object representation or
/// when the guard cannot be safely re-read. Shared by the positional and named-argument paths
/// so both reject the non-object alternative at the call boundary.
pub(super) fn guard_object_param_argument(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    value: LoweredValue,
    arg: &Expr,
) -> Option<LoweredValue> {
    let (_, param_ty) = sig.params.get(index)?;
    if sig.ref_params.get(index).copied().unwrap_or(false) {
        return None;
    }
    let PhpType::Object(class_name) = param_ty.codegen_repr() else {
        return None;
    };
    let source_ty = ctx.builder.value_php_type(value.value);
    let static_ty = argument_static_type(ctx, arg);
    let guard_ty = static_ty.as_ref().unwrap_or(&source_ty);
    if let Some(given) = union_object_guard_word(guard_ty, &class_name) {
        if let Some(deferred) = defer_union_object_param_check(ctx, sig, index, value, arg) {
            return Some(deferred);
        }
        return Some(guard_union_object_param(
            ctx, sig, index, value, &class_name, Some(given), arg,
        ));
    }
    let rerunnable = matches!(
        arg.kind,
        ExprKind::Variable(_)
            | ExprKind::This
            | ExprKind::BoolLiteral(_)
            | ExprKind::Null
            | ExprKind::IntLiteral(_)
            | ExprKind::FloatLiteral(_)
            | ExprKind::StringLiteral(_)
    );
    let already_object = matches!(guard_ty.codegen_repr(), PhpType::Object(_));
    let literal_null = matches!(arg.kind, ExprKind::Null);
    if rerunnable && !already_object && !literal_null {
        if let Some(deferred) = defer_union_object_param_check(ctx, sig, index, value, arg) {
            return Some(deferred);
        }
        return Some(guard_union_object_param(
            ctx, sig, index, value, &class_name, None, arg,
        ));
    }
    None
}

/// Adds the `instanceof` check of a `Class|false`, `Class|null` or re-read source to the current
/// call's deferred class checks once an earlier parameter's check was deferred, so it runs in
/// parameter order: with `f(?T $a, T $b)`, an invalid `$a` must be reported before an invalid
/// `$b`. The boxed value is pinned until `coerce_operands_to_params_with_checks` checks and
/// unboxes it. Returns `None` when nothing was deferred before it, and the check stays at the
/// argument.
fn defer_union_object_param_check(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    value: LoweredValue,
    arg: &Expr,
) -> Option<LoweredValue> {
    if !earlier_object_param_check_deferred(ctx)
        || !defer_object_param_check(ctx, sig, index, arg.span)
    {
        return None;
    }
    let boxed = if matches!(value.ir_type, IrType::Heap(IrHeapKind::Mixed | IrHeapKind::Union)) {
        value
    } else {
        ctx.emit_owned_value(Op::MixedBox, vec![value.value], None, PhpType::Mixed,
            Op::MixedBox.default_effects(), Some(arg.span))
    };
    Some(pin_evaluated_call_argument(ctx, boxed, arg.span))
}

/// Coerces a positional argument to storage owned explicitly by EIR when required.
///
/// Integer-to-float conversion selects the callee's floating-point ABI class. Mixed-to-string
/// conversion is also explicit here because it allocates caller-owned storage whose lifetime
/// depends on the call's return/argument alias contract; leaving that conversion hidden in ABI
/// materialization would give EIR no value to transfer or release after the call.
pub(crate) fn coerce_scalar_arg_to_param_storage(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    value: LoweredValue,
    arg: &Expr,
) -> LoweredValue {
    let Some((_, param_ty)) = sig.params.get(index) else {
        return value;
    };
    // A `mixed` or `object|null` source is checked from a rooted copy first; the re-reading
    // guard below keeps the `Class|false` / `Class|null` unions it already covered.
    let by_reference = sig.ref_params.get(index).copied().unwrap_or(false);
    if !by_reference {
        if let Some(guarded) = guard_runtime_object_param(ctx, sig, index, value, arg) {
            return guarded;
        }
        if let Some(deferred) = defer_callable_param_check(ctx, sig, index, value, arg.span) {
            return deferred;
        }
    }
    if let Some(guarded) = guard_object_param_argument(ctx, sig, index, value, arg) {
        return guarded;
    }
    // A boxed `mixed`/union value bound to a declared non-nullable scalar is checked before
    // the conversion below would silently turn `null`, an array or an object into a scalar.
    // Positional calls leave it boxed here and check it once every argument was evaluated.
    let by_reference = sig.ref_params.get(index).copied().unwrap_or(false);
    if !by_reference {
        if let Some(source) = super::scalar_param_guards::deferred_guard_source(ctx, arg) {
            if super::scalar_param_guards::guarded_scalar_param(ctx, sig, index, value.value, source)
                .is_some()
            {
                return value;
            }
        } else if let Some(converted) = super::scalar_param_guards::guard_scalar_param(
            ctx,
            sig,
            index,
            value,
            super::scalar_param_guards::GuardSource::Recorded(arg.span),
            arg.span,
            false,
            &[],
        ) {
            return converted;
        }
    }
    // A by-reference parameter must receive the caller's storage, not a converted temporary,
    // so declared-parameter scalar binding never applies to one. The checker keeps those on
    // the strict path for the same reason.
    let bindable = sig.declared_params.get(index).copied().unwrap_or(false)
        && !sig.ref_params.get(index).copied().unwrap_or(false);
    let param_ty = param_ty.codegen_repr();
    if value.ir_type == IrType::I64 && param_ty == PhpType::Float {
        return coerce_to_float(ctx, value, arg);
    }
    if param_ty == PhpType::Callable
        && !sig.ref_params.get(index).copied().unwrap_or(false)
        && ctx.builder.value_php_type(value.value) == PhpType::DeclaredCallable
    {
        return super::declared_callables::normalize_declared_callable(ctx, value, arg.span);
    }
    let source_ty = ctx.builder.value_php_type(value.value).codegen_repr();
    if param_ty == PhpType::Callable
        && !sig.ref_params.get(index).copied().unwrap_or(false)
        && matches!(source_ty, PhpType::Mixed | PhpType::Union(_))
    {
        return unbox_callable_param_storage(ctx, value, Some(arg.span));
    }
    if param_ty == PhpType::Iterable
        && !sig.ref_params.get(index).copied().unwrap_or(false)
        && matches!(source_ty, PhpType::Mixed | PhpType::Union(_))
    {
        return unbox_iterable_param_storage(ctx, value, Some(arg.span));
    }
    if param_ty == PhpType::Str && matches!(source_ty, PhpType::Mixed | PhpType::Union(_)) {
        return coerce_to_string(ctx, value, arg);
    }
    if bindable {
        if let Some(cast) = crate::types::param_binding::scalar_param_cast(&param_ty, &source_ty) {
            return apply_scalar_param_cast(ctx, cast, value, Some(arg.span));
        }
    }
    value
}

/// Checks a `mixed` or `object|null` argument against a class-like parameter it reaches
/// through `ParamBinding::ObjectGuard`, when the instance guard below cannot.
///
/// That guard re-reads its argument to name the runtime type, so it only covers side-effect
/// free sources bound to one class. This one names the type from a rooted copy, which covers
/// every source and the nullable and union declarations whose storage stays boxed. A raw object
/// declaration receives the unboxed payload.
///
/// When the call deferred its parameter checks (`DeferredObjectParamChecks`), the decision is
/// recorded and the boxed value returned unchanged: `coerce_operands_to_params_with_checks`
/// runs the check after every argument was evaluated, as PHP does.
fn guard_runtime_object_param(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    value: LoweredValue,
    arg: &Expr,
) -> Option<LoweredValue> {
    let (_, param_ty) = sig.params.get(index)?;
    let boundary = runtime_object_param_boundary(ctx, param_ty, value.value, arg)?;
    // The re-reading guard tests `instanceof`, which a closure passes for a bare `object`
    // parameter; this one keeps closures out of that raw object storage.
    let bare_object = matches!(param_ty, PhpType::Object(class)
        if class.is_empty() || class.eq_ignore_ascii_case("object"));
    if let (PhpType::Object(class_name), true) = (param_ty, !bare_object) {
        let source_ty = argument_static_type(ctx, arg)
            .unwrap_or_else(|| ctx.builder.value_php_type(value.value));
        let guarded_by_rereading = union_object_guard_word(&source_ty, class_name).is_some()
            || (matches!(arg.kind, ExprKind::Variable(_) | ExprKind::This)
                && source_ty.codegen_repr() == PhpType::Mixed);
        // Once an earlier parameter's check was deferred, this one is deferred too, so the
        // parameters are checked in declaration order.
        if guarded_by_rereading && !earlier_object_param_check_deferred(ctx) {
            return None;
        }
    }
    if defer_object_param_check(ctx, sig, index, arg.span) {
        return Some(pin_evaluated_call_argument(ctx, value, arg.span));
    }
    Some(emit_object_param_guard(ctx, sig, index, value, &boundary, arg.span))
}

/// Emits the runtime class check of regular parameter `index` and, for raw object storage,
/// unboxes the accepted value.
fn emit_object_param_guard(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    value: LoweredValue,
    boundary: &crate::types::object_boundary::ObjectBoundary,
    span: Span,
) -> LoweredValue {
    let param_ty = &sig.params[index].1;
    let (prefix, suffix) = object_param_error_parts(ctx, sig, index, param_ty, span);
    let release = ctx.value_needs_release_after_use(value);
    let value = crate::ir_lower::object_boundary_checks::guard_runtime_object_value(
        ctx, value, boundary, prefix, &suffix, release, span,
    );
    if !matches!(param_ty, PhpType::Object(_)) || callee_reflects_boxed_object(ctx) {
        return value;
    }
    let result = ctx.emit_owned_value(Op::MixedUnbox, vec![value.value], None, param_ty.clone(),
        Op::mixed_unbox_effects(param_ty), Some(span));
    release_coerced_source_if_owned(ctx, value, Some(span));
    result
}

/// Builds the `TypeError` text around the runtime type for argument `index` declared as
/// `declared`: `f(): Argument #N ($p) must be of type T, ` and ` given, called in F on line L`.
fn object_param_error_parts(
    ctx: &LoweringContext<'_, '_>, sig: &FunctionSig, index: usize, declared: &PhpType, span: Span,
) -> (Expr, String) {
    let (prefix, suffix) = param_error_text(ctx, sig, index, declared, span);
    (Expr::string_lit(prefix), suffix)
}

/// The text of `object_param_error_parts`.
fn param_error_text(
    ctx: &LoweringContext<'_, '_>, sig: &FunctionSig, index: usize, declared: &PhpType, span: Span,
) -> (String, String) {
    let param_name = &sig.params[index].0;
    let callable = ctx.call_description.as_deref().unwrap_or("call").to_string();
    let source = ctx.source_path().unwrap_or("unknown").to_string();
    let prefix = format!(
        "{callable}(): Argument #{} (${param_name}) must be of type {}, ",
        index + 1,
        crate::types::diagnostic_names::php_type_name(declared),
    );
    (prefix, format!(" given, called in {source} on line {}", span.line))
}

/// The check a caller runs for a `mixed` local bound to a by-reference parameter.
enum ReferenceParamCheck {
    /// A class-like declaration (`T`, `?T`, `A|B|null`, `object`).
    Object(PhpType, crate::types::object_boundary::ObjectBoundary),
    /// Another union of `null`, arrays and objects (`?array`, `array|A`), which the callee
    /// entry checks as well (`reference_parameter_needs_entry_guard`).
    Entry(PhpType),
}

/// Returns the check by-reference parameter `index` needs for a `mixed` local, if any.
///
/// A by-reference parameter whose declaration the signature widened to `mixed` storage keeps
/// its hint in `param_type_exprs`. Relative names (`self`, `static`, `parent`) name the
/// callee's class, which only its entry check can resolve, so they answer `None`.
fn reference_param_check(
    ctx: &LoweringContext<'_, '_>, sig: &FunctionSig, index: usize,
) -> Option<ReferenceParamCheck> {
    if !sig.declared_params.get(index).copied().unwrap_or(false) {
        return None;
    }
    let (_, ty) = sig.params.get(index)?;
    let declared = if *ty == PhpType::Mixed {
        ctx.type_expr_to_php_type_for_value(sig.param_type_exprs.get(index)?.as_ref()?)
    } else {
        ty.clone()
    };
    let relative = |class: &str| {
        let class = class.trim_start_matches('\\');
        ["self", "static", "parent"].iter().any(|word| class.eq_ignore_ascii_case(word))
    };
    let names_relative_class = match &declared {
        PhpType::Object(class) => relative(class),
        PhpType::Union(members) => members.iter()
            .any(|member| matches!(member, PhpType::Object(class) if relative(class))),
        _ => false,
    };
    if names_relative_class {
        return None;
    }
    if let Some(boundary) = crate::types::object_boundary::declared_object_boundary(&declared) {
        return Some(ReferenceParamCheck::Object(declared, boundary));
    }
    crate::ir_lower::expr::reference_parameter_needs_entry_guard(&declared)
        .then_some(ReferenceParamCheck::Entry(declared))
}

/// Records the check of a `mixed` local passed to a typed by-reference parameter in the
/// current call's frame, so it runs in parameter order with the deferred value checks.
///
/// The check re-reads the local when it runs: a later argument may assign it, and PHP checks
/// the value the reference holds at the boundary. Returns false when nothing was recorded.
fn defer_reference_object_param_check(
    ctx: &mut LoweringContext<'_, '_>, sig: &FunctionSig, index: usize, arg: &Expr,
) -> bool {
    let ExprKind::Variable(name) = &arg.kind else { return false; };
    // Lowering temporaries (property and receiver aliases) keep their own boundary checks.
    if name == "this" || crate::names::is_generated_local_name(name) || name.starts_with("__eir_") {
        return false;
    }
    let source = ctx.local_type(name);
    let recorded = match reference_param_check(ctx, sig, index) {
        Some(ReferenceParamCheck::Object(..)) =>
            crate::types::object_boundary::runtime_checked_object_source(&source),
        Some(ReferenceParamCheck::Entry(_)) => source.codegen_repr() == PhpType::Mixed,
        None => false,
    };
    recorded && defer_param_check(ctx, sig, DeferredObjectParamCheck {
        index,
        span: arg.span,
        reference_local: Some(name.clone()),
        callable: false,
    })
}

/// Checks the current value of `local`, bound to by-reference parameter `index`, against the
/// parameter's declaration. The callee entry keeps its own check for other callers.
fn emit_reference_object_param_guard(
    ctx: &mut LoweringContext<'_, '_>, sig: &FunctionSig, index: usize, local: &str, span: Span,
) {
    let (declared, boundary) = match reference_param_check(ctx, sig, index) {
        Some(ReferenceParamCheck::Object(declared, boundary)) => (declared, boundary),
        Some(ReferenceParamCheck::Entry(declared)) => {
            let callable = ctx.call_description.as_deref().unwrap_or("call").to_string();
            let source = ctx.source_path().unwrap_or("unknown").to_string();
            crate::ir_lower::expr::guard_reference_argument_value(
                ctx, &callable, index + 1, &sig.params[index].0, local, &declared,
                &format!(", called in {source} on line {}", span.line),
            );
            return;
        }
        None => return,
    };
    let (prefix, suffix) = object_param_error_parts(ctx, sig, index, &declared, span);
    let value = ctx.load_local(local, Some(span));
    let release = ctx.value_needs_release_after_use(value);
    let value = crate::ir_lower::object_boundary_checks::guard_runtime_object_value(
        ctx, value, &boundary, prefix, &suffix, release, span,
    );
    if release {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
}

/// Returns whether the call being lowered is the builtin `ReflectionObject`/`ReflectionClass`
/// constructor, which the backend lowers itself.
///
/// It reads a boxed operand through static metadata, as before this check existed, while an
/// unboxed bare `object` outside a method names no class to dispatch on.
fn callee_reflects_boxed_object(ctx: &LoweringContext<'_, '_>) -> bool {
    let Some(class) = ctx.call_description.as_deref()
        .and_then(|description| description.strip_suffix("::__construct"))
    else {
        return false;
    };
    let class = class.trim_start_matches('\\');
    (class.eq_ignore_ascii_case("ReflectionObject") || class.eq_ignore_ascii_case("ReflectionClass"))
        && ctx.classes.iter().any(|(name, info)| name.eq_ignore_ascii_case(class) && info.is_internal)
}

/// Opens a frame that defers the class-like parameter checks of the call being lowered at the
/// current expression depth. Nested calls lower their arguments deeper and keep their own frame.
pub(super) fn begin_deferred_object_param_checks(ctx: &mut LoweringContext<'_, '_>) {
    let expression_depth = ctx.expression_depth;
    ctx.deferred_object_param_checks.push(crate::ir_lower::context::DeferredObjectParamChecks {
        expression_depth,
        checks: Vec::new(),
    });
}

/// Closes the frame opened by `begin_deferred_object_param_checks` and returns its checks.
pub(super) fn finish_deferred_object_param_checks(
    ctx: &mut LoweringContext<'_, '_>,
) -> Vec<DeferredObjectParamCheck> {
    ctx.deferred_object_param_checks.pop().map(|frame| frame.checks).unwrap_or_default()
}

/// Records that regular parameter `index` needs its class check once the call's arguments
/// are complete. Returns false when no frame of the current call is open.
fn defer_object_param_check(
    ctx: &mut LoweringContext<'_, '_>, sig: &FunctionSig, index: usize, span: Span,
) -> bool {
    let check = DeferredObjectParamCheck { index, span, reference_local: None, callable: false };
    defer_param_check(ctx, sig, check)
}

/// Returns whether the call being lowered at the current depth already deferred a class check.
///
/// A check its argument can run at once keeps PHP's parameter order only while no earlier
/// parameter's check waits for the end of the argument list.
fn earlier_object_param_check_deferred(ctx: &LoweringContext<'_, '_>) -> bool {
    ctx.deferred_object_param_checks.last()
        .is_some_and(|frame| frame.expression_depth == ctx.expression_depth && !frame.checks.is_empty())
}

/// Records `check` in the current call's frame. Returns false when no frame of the current
/// call is open.
fn defer_param_check(
    ctx: &mut LoweringContext<'_, '_>, sig: &FunctionSig, check: DeferredObjectParamCheck,
) -> bool {
    let index = check.index;
    let depth = ctx.expression_depth;
    if index >= crate::types::call_args::regular_param_count(sig) {
        return false;
    }
    let Some(frame) = ctx.deferred_object_param_checks.last_mut()
        .filter(|frame| frame.expression_depth == depth)
    else {
        return false;
    };
    if !frame.checks.iter().any(|recorded| recorded.index == index) {
        frame.checks.push(check);
    }
    true
}

/// Records the binding check of a `mixed` value passed to a non-reference declared `callable`
/// parameter in the current call's frame, and pins the value it will read.
///
/// The callee checks that parameter on entry, after the caller ran its own parameter checks,
/// so with `f(callable $c, ?T $b)` an invalid `$b` would be reported before an invalid `$c`.
/// Recording it lets `coerce_operands_to_params_with_checks` check it in parameter order
/// whenever another parameter of the call is checked at the call site. Returns `None` when
/// nothing was recorded and the value is unchanged.
fn defer_callable_param_check(
    ctx: &mut LoweringContext<'_, '_>, sig: &FunctionSig, index: usize, value: LoweredValue, span: Span,
) -> Option<LoweredValue> {
    if !matches!(sig.params.get(index), Some((_, PhpType::DeclaredCallable)))
        || !matches!(ctx.builder.value_php_type(value.value), PhpType::Mixed | PhpType::Union(_))
        || ctx.deferred_object_param_checks.last().is_some_and(|frame|
            frame.expression_depth == ctx.expression_depth
                && frame.checks.iter().any(|check| check.index == index))
    {
        return None;
    }
    let check = DeferredObjectParamCheck { index, span, reference_local: None, callable: true };
    defer_param_check(ctx, sig, check).then(|| pin_evaluated_call_argument(ctx, value, span))
}

/// Records the class check a named or unpacked source bound to parameter `index` needs.
///
/// Called right after the source was evaluated, so a later argument that writes the same local
/// cannot change the type the decision reads. Returns the operand to use: a deferred check pins
/// a borrowed local load, so that write cannot free the value the check reads either.
pub(super) fn record_object_param_source(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    value: crate::ir::ValueId,
    source: &Expr,
) -> crate::ir::ValueId {
    if sig.ref_params.get(index).copied().unwrap_or(false) {
        return value;
    }
    let Some((_, param_ty)) = sig.params.get(index) else {
        return value;
    };
    if runtime_object_param_boundary(ctx, param_ty, value, source).is_some()
        && defer_object_param_check(ctx, sig, index, source.span)
    {
        let lowered = lowered_value_from_id(ctx, value);
        return pin_evaluated_call_argument(ctx, lowered, source.span).value;
    }
    let lowered = lowered_value_from_id(ctx, value);
    defer_callable_param_check(ctx, sig, index, lowered, source.span)
        .map_or(value, |deferred| deferred.value)
}

/// Returns the runtime class check a `mixed` or `object|null` argument needs at `param_ty`.
fn runtime_object_param_boundary(
    ctx: &LoweringContext<'_, '_>, param_ty: &PhpType, value: crate::ir::ValueId, arg: &Expr,
) -> Option<crate::types::object_boundary::ObjectBoundary> {
    let source_ty = argument_static_type(ctx, arg)
        .unwrap_or_else(|| ctx.builder.value_php_type(value));
    crate::types::object_boundary::runtime_object_boundary(param_ty, &source_ty)
}

/// Returns the checker-facing type of one argument expression when lowering still knows it.
///
/// Locals retain their flow-sensitive PHP type even when their storage widened to a boxed
/// Mixed cell; property and element places retain the declared property/element type.
fn argument_static_type(
    ctx: &LoweringContext<'_, '_>,
    arg: &Expr,
) -> Option<PhpType> {
    match &arg.kind {
        ExprKind::Variable(name) => {
            let ty = ctx.local_type(name);
            (!matches!(ty.codegen_repr(), PhpType::Mixed)).then_some(ty)
        }
        _ => super::ref_place_args::static_place_type(ctx, arg),
    }
}

/// Extracts a statically checked array whose source storage is a boxed Mixed cell.
///
/// A parameter compiled for RAW array storage — `array: Heap(Array)`, which is what the SPL and
/// prelude classes declare — receives a cell POINTER when the argument came out of a property,
/// because `prop_get` on a declared `array` yields `Heap(Mixed)`. Nothing converted between the
/// two, so the callee read the cell's header words as the array's:
///
/// ```text
/// class Holder { public array $items = [3, 4]; }
/// $ai = new ArrayIterator((new Holder())->items);
/// echo $ai->count();                        // php: 2, elephc: 4 — and foreach yielded nothing
/// ```
///
/// The checker admits the call because a declared `array` is
/// `array<mixed>|array<mixed, mixed>` and every member is assignable to `array<mixed>`; the
/// disagreement is purely one of STORAGE, which is why only the lowering can answer it. A user
/// function is unaffected and always was: its parameter slot is `Heap(Mixed)` too, so both sides
/// already agreed.
///
/// The unbox is a plain `Op::MixedUnbox`, the instruction the `Callable` case below uses, and it
/// has to stay one: it is the only conversion here that does not care which LAYOUT the payload
/// has. A declared `array` property holds a packed array or a hash depending on what was stored,
/// while the SPL parameter says `array<mixed>` either way. The shared container conversion
/// merge temps and `??` use (`coerce_container_to_mixed_payload`) looks tempting — it also
/// canonicalizes slots and carries the boxed-element-slot guard — but it trusts the STATIC layout:
/// routed through it, `new ArrayObject($h->pairs)` ran the indexed-array conversion over a hash
/// and died with SIGBUS, where this unbox prints `a1b2`. That conversion is right where the static
/// type came from a real array; at this boundary it came from a declaration.
///
/// No guard is needed, because only a `mixed`-element contract can reach this arm. The checker
/// admits a declared `array` at `array<mixed>` (each member of `array<mixed>|array<mixed, mixed>`
/// is assignable to it) but refuses it at `array<int>`: the `array<mixed, mixed>` member is not an
/// `array<int>`. Under a `mixed`-element contract the callee reads the payload through its own
/// value_type tag, so raw and boxed slots are both read correctly. If a typed contract ever becomes
/// reachable here, the guard has to come with it — and so does a layout check.
pub(super) fn unbox_array_param_storage(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    param_ty: PhpType,
    span: Option<crate::span::Span>,
) -> LoweredValue {
    let effects = Op::mixed_unbox_effects(&param_ty);
    let result = ctx.emit_owned_value(Op::MixedUnbox, vec![value.value], None, param_ty, effects, span);
    release_coerced_source_if_owned(ctx, value, span);
    result
}

/// Extracts a boxed array for a declared `array<K, V>` parameter, as the hash it is compiled for.
///
/// This is the typed contract `unbox_array_param_storage` says needs a layout check: a declared
/// PHP `array` (or any boxed array value) holds a packed list or a hash depending on what was
/// stored, and a list is a valid keyed array, but the callee reads hash storage. The payload is
/// unboxed as a list VIEW and handed to `Op::ArrayToHash`, which reads the runtime heap kind:
/// a hash is passed through as it is, a list is rebuilt. Either way the result owns one
/// reference, taken over from the unbox.
fn unbox_hash_param_storage(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    param_ty: PhpType,
    span: Option<crate::span::Span>,
) -> LoweredValue {
    let PhpType::AssocArray { value: entry_ty, .. } = param_ty.codegen_repr() else {
        return unbox_array_param_storage(ctx, value, param_ty, span);
    };
    let list_view_ty = PhpType::Array(entry_ty);
    let effects = Op::mixed_unbox_effects(&list_view_ty);
    let list_view =
        ctx.emit_owned_value(Op::MixedUnbox, vec![value.value], None, list_view_ty, effects, span);
    release_coerced_source_if_owned(ctx, value, span);
    ctx.emit_value(
        Op::ArrayToHash,
        vec![list_view.value],
        None,
        param_ty,
        Op::ArrayToHash.default_effects(),
        span,
    )
}

/// Extracts a statically checked callable whose merge storage became a Mixed cell.
/// The backend retains the descriptor, so the extracted EIR value must own that lease.
pub(super) fn unbox_callable_param_storage(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<crate::span::Span>,
) -> LoweredValue {
    let result = ctx.emit_owned_value(
        Op::MixedUnbox,
        vec![value.value],
        None,
        PhpType::Callable,
        Op::mixed_unbox_effects(&PhpType::Callable),
        span,
    );
    release_coerced_source_if_owned(ctx, value, span);
    result
}

/// Extracts a checked iterable's raw array, hash or Traversable pointer from a Mixed cell.
/// The layout-neutral unbox acquires its payload before retiring the cell's source lease.
fn unbox_iterable_param_storage(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<crate::span::Span>,
) -> LoweredValue {
    let result = ctx.emit_owned_value(
        Op::MixedUnbox, vec![value.value], None, PhpType::Iterable,
        Op::mixed_unbox_effects(&PhpType::Iterable), span,
    );
    release_coerced_source_if_owned(ctx, value, span);
    result
}

/// Returns the rejected alternative's PHP spelling when a union is exactly
/// `Object(class)|false` or `Object(class)|null`, or `None` for every other shape.
fn union_object_guard_word(actual: &PhpType, class_name: &str) -> Option<&'static str> {
    let PhpType::Union(members) = actual else {
        return None;
    };
    let expected = PhpType::Object(class_name.to_string());
    if members.len() != 2 || members.iter().filter(|member| **member == expected).count() != 1 {
        return None;
    }
    if members.iter().any(|member| matches!(member, PhpType::False)) {
        Some("false")
    } else if members.iter().any(|member| matches!(member, PhpType::Void)) {
        Some("null")
    } else {
        None
    }
}

/// Guards a `Object(class)|false` / `Object(class)|null` argument and unboxes the object.
///
/// The rejected branch throws a catchable `TypeError` naming the supplied alternative; the
/// accepted branch extracts the raw object payload the declared parameter ABI expects.
fn guard_union_object_param(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    value: LoweredValue,
    class_name: &str,
    given: Option<&str>,
    arg: &Expr,
) -> LoweredValue {
    let span = Some(arg.span);
    // `InstanceOf` and the accepted branch's unbox both expect boxed storage. A
    // statically-known object (a subclass or a literal optimized from a union) keeps its
    // raw representation, so box it first instead of feeding Object where Mixed is required.
    let value = if matches!(
        value.ir_type,
        IrType::Heap(IrHeapKind::Mixed | IrHeapKind::Union)
    ) {
        value
    } else {
        ctx.emit_owned_value(
            Op::MixedBox,
            vec![value.value],
            None,
            PhpType::Mixed,
            Op::MixedBox.default_effects(),
            span,
        )
    };
    let bare_object = class_name.is_empty() || class_name.eq_ignore_ascii_case("object");
    let condition = if bare_object {
        ctx.emit_value(Op::TypePredicate, vec![value.value],
            Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::Object)),
            PhpType::Bool, Op::TypePredicate.default_effects(), span)
    } else {
        let class_data = ctx.intern_class_name(class_name);
        ctx.emit_value(Op::InstanceOf, vec![value.value], Some(Immediate::Data(class_data)),
            PhpType::Bool, Op::InstanceOf.default_effects(), span)
    };
    let accepted = ctx.builder.create_named_block("object.arg.accepted", Vec::new());
    let rejected = ctx.builder.create_named_block("object.arg.rejected", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: condition.value,
        then_target: accepted,
        then_args: Vec::new(),
        else_target: rejected,
        else_args: Vec::new(),
    });
    ctx.builder.position_at_end(rejected);
    if ctx.value_needs_release_after_use(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, span);
    }
    let param_name = sig
        .params
        .get(index)
        .map(|(name, _)| name.as_str())
        .unwrap_or("arg");
    let callable = ctx
        .call_description
        .as_deref()
        .unwrap_or("call");
    let source = ctx.source_path().unwrap_or("unknown");
    let prefix = format!(
        "{}(): Argument #{} (${}) must be of type {}, ",
        callable,
        index + 1,
        param_name,
        if bare_object { "object" } else { class_name }
    );
    let suffix = format!(" given, called in {} on line {}", source, arg.span.line);
    let given = match given {
        Some(text) => Expr::new(ExprKind::StringLiteral(text.to_string()), arg.span),
        None => super::enum_argument_guards::runtime_debug_type(arg, arg.span),
    };
    let message = [
        Expr::new(ExprKind::StringLiteral(prefix), arg.span),
        given,
        Expr::new(ExprKind::StringLiteral(suffix), arg.span),
    ]
    .into_iter()
    .reduce(|left, right| {
        Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(left),
                op: crate::parser::ast::BinOp::Concat,
                right: Box::new(right),
            },
            arg.span,
        )
    })
    .expect("the diagnostic always has a prefix and a suffix");
    let exception = lower_expr(
        ctx,
        &Expr::new(
            ExprKind::NewObject {
                class_name: Name::unqualified("TypeError"),
                args: vec![message],
            },
            arg.span,
        ),
    );
    ctx.builder.terminate(Terminator::Throw {
        value: exception.value,
    });
    ctx.builder.position_at_end(accepted);
    let object_ty = PhpType::Object(class_name.to_string());
    let result = ctx.emit_owned_value(
        Op::MixedUnbox,
        vec![value.value],
        None,
        object_ty.clone(),
        Op::mixed_unbox_effects(&object_ty),
        span,
    );
    release_coerced_source_if_owned(ctx, value, span);
    result
}

/// Applies a declared-parameter scalar binding to an already-lowered argument value.
///
/// The conversion is the one elephc emits for the equivalent explicit cast, which is why the
/// binding is expressed as a `CastType`: `(string)` and `(bool)` are total over the scalar
/// sources `crate::types::param_binding` admits, so no runtime failure path is needed here.
fn apply_scalar_param_cast(
    ctx: &mut LoweringContext<'_, '_>,
    cast: CastType,
    value: LoweredValue,
    span: Option<crate::span::Span>,
) -> LoweredValue {
    match cast {
        CastType::String => coerce_to_string_at_span(ctx, value, span),
        CastType::Bool => lower_truthy_bool(ctx, value, span),
        // `param_binding::scalar_param_cast` only ever reports the two total scalar casts.
        CastType::Int | CastType::Float | CastType::Array | CastType::Object => value,
    }
}

/// Normalizes reordered call operands to their declared scalar parameter storage.
///
/// Named and spread arguments are evaluated in source order and then reordered, so their
/// int-to-float and Mixed-to-string conversions happen here in parameter order. By-reference
/// parameters and the variadic tail remain untouched. String conversions become owned EIR
/// values so normal alias-aware call cleanup can transfer or release them safely.
pub(super) fn coerce_operands_to_params(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    operands: Vec<crate::ir::ValueId>,
) -> Vec<crate::ir::ValueId> {
    coerce_operands_to_params_guarded(ctx, sig, operands, None, false)
}

/// Normalizes reordered operands and first checks each runtime-typed operand bound to a
/// declared scalar parameter, in parameter order and after every argument was evaluated.
///
/// `sources` says, per parameter, how to decide whether its operand needs the check; `None`
/// keeps the previous behaviour of callers that never guard.
pub(super) fn coerce_operands_to_params_guarded(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    mut operands: Vec<crate::ir::ValueId>,
    sources: Option<&[super::scalar_param_guards::GuardSource]>,
    positional: bool,
) -> Vec<crate::ir::ValueId> {
    use super::scalar_param_guards::GuardSource;
    let regular_param_count = crate::types::call_args::regular_param_count(sig);
    let limit = operands.len().min(regular_param_count);
    let guard_span = ctx.call_site_span.unwrap_or_else(Span::dummy);
    let needs_guard = (0..limit)
        .map(|index| {
            let source = sources.map_or(GuardSource::Skip, |sources| {
                sources.get(index).copied().unwrap_or(GuardSource::Skip)
            });
            super::scalar_param_guards::guarded_scalar_param(ctx, sig, index, operands[index], source)
                .map(|_| source)
        })
        .collect::<Vec<_>>();
    // A positional operand whose check was deferred here keeps the argument-time contract:
    // its conversion runs without first publishing every operand, exactly as when it was
    // converted while its argument was lowered, so a guarded call costs no extra roots.
    let deferred_positional = (0..limit)
        .map(|index| positional && needs_guard[index].is_some())
        .collect::<Vec<_>>();
    let needs_coercion = (0..limit)
        .map(|index| !deferred_positional[index]
            && (needs_guard[index].is_some() || operand_needs_param_coercion(ctx, sig, &operands, index)))
        .collect::<Vec<_>>();
    // A deferred guard can throw too: every other owned operand (a freshly built array, a call
    // result) must be published first, or the throw leaks it. Guarded operands themselves stay
    // unpublished, so an accepted check costs no extra root; a refusal releases them instead,
    // its own operand and every other guarded operand (or its converted string) still owned.
    let publish_all = needs_coercion.iter().any(|needs| *needs);
    let publish_others = !publish_all && deferred_positional.iter().any(|deferred| *deferred);
    if publish_all || publish_others {
        // Source evaluation has completed, but any parameter conversion below can throw.
        // Publish the one last source value that did not need protection during source
        // evaluation, plus synthesized owned operands, before the first conversion runs.
        for index in 0..limit {
            if sig.ref_params.get(index).copied().unwrap_or(false)
                || publish_others && deferred_positional[index]
            {
                continue;
            }
            let lowered = lowered_value_from_id(ctx, operands[index]);
            operands[index] =
                root_evaluated_call_argument(ctx, lowered, Span::dummy()).value;
        }
        // A deferred class check runs after the variadic tail array was built; publish that
        // owned array too, so the check's `TypeError` releases it while unwinding.
        if !object_checks.is_empty() && !variadic_param_is_by_ref(sig) {
            for index in limit..operands.len() {
                let lowered = lowered_value_from_id(ctx, operands[index]);
                operands[index] =
                    root_evaluated_call_argument(ctx, lowered, Span::dummy()).value;
            }
        }
    }
    // What each deferred guard's refusal must release besides its own operand: the other
    // guarded operands that are owned temporaries, plus, for an earlier string guard, the box it
    // kept and the string it converted (an earlier int/float guard already released its box).
    let deferred_sources = operands.clone();
    let mut still_owned: Vec<Vec<crate::ir::ValueId>> = (0..limit)
        .map(|index| {
            let value = lowered_value_from_id(ctx, deferred_sources[index]);
            if publish_others && deferred_positional[index] && ctx.value_is_owning_temporary(value) {
                vec![value.value]
            } else {
                Vec::new()
            }
        })
        .collect();
    for index in 0..limit {
        if sig.ref_params.get(index).copied().unwrap_or(false) {
            if let Some((local, span)) = reference_check(index) {
                emit_reference_object_param_guard(ctx, sig, index, &local, span);
            }
            continue;
        }
        let Some((_, param_ty)) = sig.params.get(index) else {
            continue;
        };
        let value = operands[index];
        if let Some(source) = needs_guard[index] {
            let lowered = lowered_value_from_id(ctx, value);
            let also_release = still_owned
                .iter()
                .enumerate()
                .filter(|(other, _)| *other != index)
                .flat_map(|(_, values)| values.iter().copied())
                .collect::<Vec<_>>();
            if let Some(converted) = super::scalar_param_guards::guard_scalar_param(
                ctx, sig, index, lowered, source, guard_span,
                publish_others && deferred_positional[index], &also_release,
            ) {
                // The guard released an owned box it converted: nothing is left to release.
                still_owned[index].clear();
                // The guard already produced the `int`/`float` PHP binds; the boxed source
                // stays owned by its published root and retires with the call's operands.
                operands[index] = converted.value;
                continue;
            }
            if deferred_positional[index] && param_ty.codegen_repr() == PhpType::Str {
                // The conversion can throw (a `Stringable`'s `__toString()`), and only a guard's
                // refusal releases the other unpublished operands: publish them first.
                if publish_others {
                    for other in 0..limit {
                        if still_owned[other].is_empty() {
                            continue;
                        }
                        let operand = lowered_value_from_id(ctx, operands[other]);
                        // A later operand, or an earlier string guard's converted string.
                        if let Some(position) =
                            still_owned[other].iter().position(|value| *value == operand.value)
                        {
                            still_owned[other].remove(position);
                            operands[other] =
                                root_evaluated_call_argument(ctx, operand, Span::dummy()).value;
                        }
                    }
                }
                // The same owned conversion argument lowering would have made. It carries the
                // call's span: a conversion that can warn publishes its span as the current
                // line, and a span-less one would publish line 0 over the caller's site (a
                // compiler-generated span is not published at all).
                // Its own box may have been published just above.
                let source = lowered_value_from_id(ctx, operands[index]);
                let string = coerce_to_string_at_span(ctx, source, Some(guard_span));
                if publish_others && ctx.value_is_owning_temporary(string) {
                    still_owned[index].push(string.value);
                }
                operands[index] = string.value;
                continue;
            }
        }
        let operand_ty = ctx.builder.value_php_type(value).codegen_repr();
        let param_ty = param_ty.codegen_repr();
        if param_ty == PhpType::Float && matches!(operand_ty, PhpType::Int | PhpType::Bool) {
            let lowered = LoweredValue {
                value,
                ir_type: IrType::I64,
            };
            let coerced = coerce_to_float_at_span(ctx, lowered, None);
            operands[index] =
                root_evaluated_call_argument(ctx, coerced, Span::dummy()).value;
        } else if param_ty == PhpType::Str
            && matches!(operand_ty, PhpType::Mixed | PhpType::Union(_))
        {
            let lowered = LoweredValue {
                value,
                ir_type: ctx.builder.value_type(value),
            };
            let coerced = coerce_to_string_at_span(ctx, lowered, None);
            operands[index] =
                root_evaluated_call_argument(ctx, coerced, Span::dummy()).value;
        } else if param_ty == PhpType::Callable
            && matches!(operand_ty, PhpType::Mixed | PhpType::Union(_))
        {
            let lowered = LoweredValue { value, ir_type: ctx.builder.value_type(value) };
            let coerced = if ctx.builder.value_php_type(value) == PhpType::DeclaredCallable {
                super::declared_callables::normalize_declared_callable(ctx, lowered, Span::dummy())
            } else {
                unbox_callable_param_storage(ctx, lowered, None)
            };
            operands[index] =
                root_evaluated_call_argument(ctx, coerced, Span::dummy()).value;
        } else if param_ty == PhpType::Iterable
            && matches!(operand_ty, PhpType::Mixed | PhpType::Union(_))
        {
            let lowered = LoweredValue { value, ir_type: ctx.builder.value_type(value) };
            let coerced = unbox_iterable_param_storage(ctx, lowered, None);
            operands[index] =
                root_evaluated_call_argument(ctx, coerced, Span::dummy()).value;
        } else if matches!(param_ty, PhpType::Array(_) | PhpType::AssocArray { .. })
            && matches!(operand_ty, PhpType::Mixed | PhpType::Union(_))
        {
            let lowered = LoweredValue { value, ir_type: ctx.builder.value_type(value) };
            let declared_hash = sig.declared_params.get(index).copied().unwrap_or(false)
                && matches!(param_ty, PhpType::AssocArray { .. });
            let coerced = if declared_hash {
                unbox_hash_param_storage(ctx, lowered, param_ty.clone(), None)
            } else {
                unbox_array_param_storage(ctx, lowered, param_ty.clone(), None)
            };
            operands[index] =
                root_evaluated_call_argument(ctx, coerced, Span::dummy()).value;
        } else if let Some(hash_ty) = packed_argument_param_storage(ctx, sig, index, value) {
            // `ArrayToHash` consumes one owner of its operand, so the argument (a borrowed
            // local, or a temporary the pass above already rooted) lends it a reference of its
            // own. The caller's list is untouched and the callee receives an independent hash.
            let lowered = lowered_value_from_id(ctx, value);
            let source =
                crate::ir_lower::ownership::acquire_if_refcounted(ctx, lowered, None);
            let converted = ctx.emit_value(
                Op::ArrayToHash,
                vec![source.value],
                None,
                hash_ty,
                Op::ArrayToHash.default_effects(),
                None,
            );
            operands[index] =
                root_evaluated_call_argument(ctx, converted, Span::dummy()).value;
        } else if sig.declared_params.get(index).copied().unwrap_or(false) {
            // Same declared-parameter scalar binding the positional path applies, run here in
            // parameter order because named and spread arguments are lowered in source order
            // and only reordered afterwards.
            if let Some(cast) =
                crate::types::param_binding::scalar_param_cast(&param_ty, &operand_ty)
            {
                let lowered = LoweredValue {
                    value,
                    ir_type: ctx.builder.value_type(value),
                };
                let coerced = apply_scalar_param_cast(ctx, cast, lowered, None);
                operands[index] =
                    root_evaluated_call_argument(ctx, coerced, Span::dummy()).value;
            }
        }
    }
    operands
}

/// Returns whether parameter-order normalization will emit a conversion for one operand.
fn operand_needs_param_coercion(
    ctx: &LoweringContext<'_, '_>,
    sig: &FunctionSig,
    operands: &[crate::ir::ValueId],
    index: usize,
) -> bool {
    if sig.ref_params.get(index).copied().unwrap_or(false) {
        return false;
    }
    let Some((_, param_ty)) = sig.params.get(index) else {
        return false;
    };
    let operand_ty = ctx.builder.value_php_type(operands[index]).codegen_repr();
    let param_ty = param_ty.codegen_repr();
    (param_ty == PhpType::Float && matches!(operand_ty, PhpType::Int | PhpType::Bool))
        || (param_ty == PhpType::Str
            && matches!(operand_ty, PhpType::Mixed | PhpType::Union(_)))
        || (param_ty == PhpType::Callable
            && matches!(operand_ty, PhpType::Mixed | PhpType::Union(_)))
        || (param_ty == PhpType::Iterable
            && matches!(operand_ty, PhpType::Mixed | PhpType::Union(_)))
        || (matches!(param_ty, PhpType::Array(_) | PhpType::AssocArray { .. })
            && matches!(operand_ty, PhpType::Mixed | PhpType::Union(_)))
        || packed_argument_param_storage(ctx, sig, index, operands[index]).is_some()
        || (sig.declared_params.get(index).copied().unwrap_or(false)
            && crate::types::param_binding::scalar_param_cast(&param_ty, &operand_ty).is_some())
}

/// Returns the hash a packed list operand is rebuilt into for a declared by-value hash
/// parameter, or `None` when the operand already has the parameter's storage.
///
/// Only user-declared parameter types take part: a builtin's signature describes what its
/// runtime helper accepts, and those helpers read either layout through the heap kind.
fn packed_argument_param_storage(
    ctx: &LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    operand: crate::ir::ValueId,
) -> Option<PhpType> {
    if !sig.declared_params.get(index).copied().unwrap_or(false)
        || sig.ref_params.get(index).copied().unwrap_or(false)
    {
        return None;
    }
    let (_, param_ty) = sig.params.get(index)?;
    crate::types::packed_argument_hash_storage(param_ty, &ctx.builder.value_php_type(operand))
}

/// Normalizes concrete local arrays to the storage required by their by-reference parameter.
pub(super) fn lower_by_ref_array_arg_with_signature(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    arg: &Expr,
) -> Option<crate::ir::ValueId> {
    if !sig.ref_params.get(index).copied().unwrap_or(false) {
        return None;
    }
    let (_, param_ty) = sig.params.get(index)?;
    let ExprKind::Variable(name) = &arg.kind else {
        return None;
    };
    if param_ty.is_php_array()
        && matches!(ctx.local_type(name).codegen_repr(), PhpType::Array(_) | PhpType::AssocArray { .. })
    {
        let local = ctx.load_local(name, Some(arg.span));
        let boxed = ctx.box_value_as_mixed(local, param_ty.clone(), Some(arg.span));
        ctx.store_call_argument_local(name, boxed, param_ty.clone(), Some(arg.span));
        promote_reference_return_local_argument(ctx, sig, index, arg);
        return Some(ctx.load_local(name, Some(arg.span)).value);
    }
    // Only a written array type is a layout contract; a plain `array` hint keeps the paths below.
    if crate::types::is_written_reference_array_param(&sig.ref_params, &sig.param_type_exprs, index) {
        if let Some(storage_ty) =
            crate::types::packed_reference_argument_storage(param_ty, &ctx.local_type(name))
        {
            return Some(convert_packed_local_for_reference(ctx, sig, index, arg, name, storage_ty));
        }
    }
    if !by_ref_array_arg_needs_mixed_storage(ctx, name, param_ty) {
        return None;
    }
    let array_ty = PhpType::Array(Box::new(PhpType::Mixed));
    let local = ctx.load_local(name, Some(arg.span));
    let converted = ctx.emit_value(
        Op::ArrayToMixed,
        vec![local.value],
        None,
        array_ty.clone(),
        Op::ArrayToMixed.default_effects(),
        Some(arg.span),
    );
    ctx.store_call_normalized_local(name, converted, array_ty, Some(arg.span));
    promote_reference_return_local_argument(ctx, sig, index, arg);
    Some(ctx.load_local(name, Some(arg.span)).value)
}

/// Converts a typed list local, in place, to the layout a declared by-reference container
/// parameter is compiled for: a hash (`array<K, V>`) or a list of boxed values (`array<mixed>`).
///
/// The callee reads and writes the caller's variable through the reference, so the variable
/// has to hold the declared layout before its address is taken. The conversion consumes the
/// local's owner and the local is re-typed and stored back, which also records the conversion
/// for the statement fixed point, so a call inside a loop or a branch converts the local at
/// the region entry instead of mid-flight.
///
/// The checker accepts only an ordinary local here (rule 4 of `require_boxed_by_ref_storage`).
/// A slot lowering cannot convert in place after all is refused rather than bound unconverted.
fn convert_packed_local_for_reference(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    arg: &Expr,
    name: &str,
    storage_ty: PhpType,
) -> crate::ir::ValueId {
    if ctx.is_ref_bound_local(name)
        || !crate::ir_lower::stmt::local_slot_is_convertible_here(ctx, name)
    {
        crate::ir_lower::diagnostics::refuse(
            arg.span,
            "A list variable that other code already reads as a typed list cannot be bound to a \
             by-reference parameter with another array layout; declare the variable with the \
             parameter's type",
        );
    }
    let local = ctx.load_local(name, Some(arg.span));
    if matches!(storage_ty.codegen_repr(), PhpType::AssocArray { .. }) {
        let hash = ctx.emit_value(
            Op::ArrayToHash,
            vec![local.value],
            None,
            storage_ty.clone(),
            Op::ArrayToHash.default_effects(),
            Some(arg.span),
        );
        // The callee reaches the hash through this slot's address, so the slot has to keep
        // holding the raw container pointer. A plain re-typing store would widen the frame
        // slot to a boxed cell instead, and the callee would then read the box as the table.
        ctx.store_retyped_container_local(name, hash, storage_ty, Some(arg.span));
    } else {
        let boxed = ctx.emit_value(
            Op::ArrayToMixed,
            vec![local.value],
            None,
            storage_ty.clone(),
            Op::ArrayToMixed.default_effects(),
            Some(arg.span),
        );
        ctx.store_mutated_local(name, boxed, storage_ty, Some(arg.span));
    }
    promote_reference_return_local_argument(ctx, sig, index, arg);
    ctx.load_local(name, Some(arg.span)).value
}

/// Lowers `$array[$index]` as a direct by-reference argument cell address.
pub(super) fn lower_by_ref_array_element_arg_with_signature(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    arg: &Expr,
) -> Option<crate::ir::ValueId> {
    if !sig.ref_params.get(index).copied().unwrap_or(false) {
        return None;
    }
    let ExprKind::ArrayAccess { array, index: element_index } = &arg.kind else {
        return None;
    };
    let ExprKind::Variable(array_name) = &array.kind else {
        return None;
    };
    let local_ty = ctx.local_type(array_name).codegen_repr();
    let (_, param_ty) = sig.params.get(index)?;
    if let PhpType::AssocArray { key, value } = &local_ty {
        // A ref-bound table is shared with another frame that keeps its declared layout; the
        // typed write-back rewrite in `ref_place_args` handles those elements instead.
        if param_ty.codegen_repr() == PhpType::Mixed
            && matches!(value.codegen_repr(), PhpType::Pointer(_) | PhpType::Buffer(_) | PhpType::Packed(_))
        {
            // Widening boxes raw pointer, buffer and packed words as plain integers, which would
            // corrupt every entry of the hash before the callee even runs.
            crate::ir_lower::diagnostics::refuse(
                arg.span,
                "A typed array element holding pointers, or whose own entries are arrays or \
                 objects, cannot be passed to a wider by-reference parameter; declare the \
                 parameter with the element's exact type",
            );
        } else if param_ty.codegen_repr() == PhpType::Mixed
            && value.codegen_repr() != PhpType::Mixed
            && !ctx.is_ref_bound_local(array_name)
        {
            // Same as the indexed branch below: the callee can store any value through this
            // entry, so widen the hash's entries to boxed Mixed cells first. Binding the concrete
            // entry left the local typed `int` while the callee stored an object in it, and every
            // later typed read decoded that object pointer as an integer.
            let parent = ctx.load_local(array_name, Some(array.span));
            let owned =
                crate::ir_lower::ownership::acquire_if_refcounted(ctx, parent, Some(arg.span));
            let boxed_ty = PhpType::AssocArray { key: key.clone(), value: Box::new(PhpType::Mixed) };
            let converted = ctx.emit_value(
                Op::HashToMixed,
                vec![owned.value],
                None,
                boxed_ty.clone(),
                Op::HashToMixed.default_effects(),
                Some(arg.span),
            );
            ctx.store_call_argument_local(array_name, converted, boxed_ty, Some(arg.span));
        }
        let hash_value = ctx.load_local(array_name, Some(array.span));
        let element_index = lower_expr(ctx, element_index);
        let cell = ctx.emit_value(
            Op::LoadArrayElemRefCell,
            vec![hash_value.value, element_index.value],
            None,
            PhpType::Pointer(None),
            Op::LoadArrayElemRefCell.default_effects(),
            Some(arg.span),
        );
        return Some(lease_managed_call_argument_ref_cell(ctx, cell, arg.span).value);
    }
    if matches!(local_ty, PhpType::Mixed | PhpType::Union(_)) {
        // A declared PHP array reads back as one boxed value, so neither concrete branch above
        // matches even though the element is a managed cell just the same. Leasing it here is
        // what puts the cell in the call's unwind ledger: without that, a later argument whose
        // evaluation throws left the cell unreleased.
        let array_value = ctx.load_local(array_name, Some(array.span));
        let element_index = lower_expr(ctx, element_index);
        let cell = ctx.emit_value(
            Op::LoadArrayElemRefCell,
            vec![array_value.value, element_index.value],
            None,
            PhpType::Pointer(None),
            Op::LoadArrayElemRefCell.default_effects(),
            Some(arg.span),
        );
        return Some(lease_managed_call_argument_ref_cell(ctx, cell, arg.span).value);
    }
    let PhpType::Array(elem_ty) = local_ty else {
        return None;
    };
    if elem_ty.codegen_repr() == PhpType::Mixed {
        let array_value = ctx.load_local(array_name, Some(array.span));
        let element_index = lower_expr(ctx, element_index);
        let cell = ctx.emit_value(
            Op::LoadArrayElemRefCell,
            vec![array_value.value, element_index.value],
            None,
            PhpType::Pointer(None),
            Op::LoadArrayElemRefCell.default_effects(),
            Some(arg.span),
        );
        return Some(lease_managed_call_argument_ref_cell(ctx, cell, arg.span).value);
    }
    if param_ty.codegen_repr() == PhpType::Mixed && elem_ty.codegen_repr() != PhpType::Mixed {
        // The callee replaces a Mixed pointer through this element's actual slot, not a
        // detached temporary. Widen the outer array's slots before exposing that address.
        // Retaining the borrowed source lets the consuming conversion separate COW aliases.
        let parent = ctx.load_local(array_name, Some(array.span));
        let owned = crate::ir_lower::ownership::acquire_if_refcounted(ctx, parent, Some(arg.span));
        let boxed_parent_ty = PhpType::Array(Box::new(PhpType::Mixed));
        let converted = ctx.emit_value(
            Op::ArrayToMixed,
            vec![owned.value],
            None,
            boxed_parent_ty.clone(),
            Op::ArrayToMixed.default_effects(),
            Some(arg.span),
        );
        ctx.store_call_argument_local(
            array_name, converted, boxed_parent_ty, Some(arg.span),
        );
        // The parent now stores boxed slots, so this element is a managed cell like any other
        // `array<mixed>` element. Handing out a bare interior address instead left the FIRST of
        // two aliases of one element pointing at a slot the second argument then replaced with a
        // reference cell, so the callee read that cell pointer as the element's value.
        let array_value = ctx.load_local(array_name, Some(array.span));
        let element_index = lower_expr(ctx, element_index);
        let cell = ctx.emit_value(
            Op::LoadArrayElemRefCell,
            vec![array_value.value, element_index.value],
            None,
            PhpType::Pointer(None),
            Op::LoadArrayElemRefCell.default_effects(),
            Some(arg.span),
        );
        return Some(lease_managed_call_argument_ref_cell(ctx, cell, arg.span).value);
    }
    let array_value = ctx.load_local(array_name, Some(array.span));
    let element_index = lower_expr(ctx, element_index);
    let element_index = coerce_to_int_at_span(ctx, element_index, Some(arg.span));
    let value = ctx
        .builder
        .emit_with_effects(
            Op::ArrayElemAddr,
            vec![array_value.value, element_index.value],
            None,
            IrType::I64,
            PhpType::Pointer(None),
            Ownership::NonHeap,
            Op::ArrayElemAddr.default_effects(),
            Some(arg.span),
        )
        .expect("array_elem_addr produces a value");
    Some(value)
}

/// Returns true when a local array must be converted before a by-reference call.
pub(super) fn by_ref_array_arg_needs_mixed_storage(
    ctx: &LoweringContext<'_, '_>,
    name: &str,
    param_ty: &PhpType,
) -> bool {
    let PhpType::Array(param_elem) = param_ty.codegen_repr() else {
        return false;
    };
    if param_elem.codegen_repr() != PhpType::Mixed {
        return false;
    }
    let PhpType::Array(local_elem) = ctx.local_type(name).codegen_repr() else {
        return false;
    };
    local_elem.codegen_repr() != PhpType::Mixed
}

/// Lowers positional call arguments with omitted optional defaults and variadic tail packing.
pub(super) fn lower_args_with_signature(
    ctx: &mut LoweringContext<'_, '_>,
    sig: Option<&FunctionSig>,
    args: &[Expr],
) -> Vec<crate::ir::ValueId> {
    lower_args_with_signature_options(ctx, sig, args, false, false)
}


/// Lowers arguments while preserving omission of trailing default-only parameter slots.
pub(super) fn lower_args_with_signature_trimming_trailing_defaults(
    ctx: &mut LoweringContext<'_, '_>,
    sig: Option<&FunctionSig>,
    args: &[Expr],
) -> Vec<crate::ir::ValueId> {
    lower_args_with_signature_options(ctx, sig, args, true, false)
}

/// Applies shared argument planning with optional elision of trailing defaults.
pub(super) fn lower_args_with_signature_options(
    ctx: &mut LoweringContext<'_, '_>,
    sig: Option<&FunctionSig>,
    args: &[Expr],
    trim_trailing_defaults: bool,
    capture_values: bool,
) -> Vec<crate::ir::ValueId> {
    lower_args_with_signature_options_for_capture(
        ctx, sig, args, trim_trailing_defaults, capture_values, None,
    )
}

/// Promotes one runtime-parser output when its argument is evaluated in source order.
pub(super) fn lower_args_with_signature_options_for_capture(
    ctx: &mut LoweringContext<'_, '_>,
    sig: Option<&FunctionSig>,
    args: &[Expr],
    trim_trailing_defaults: bool,
    capture_values: bool,
    capture_output_index: Option<(usize, PhpType)>,
) -> Vec<crate::ir::ValueId> {
    let Some(sig) = sig else {
        return lower_args(ctx, args);
    };
    let literal_bound = rewrite_literal_param_bindings(sig, args);
    let args = literal_bound.as_deref().unwrap_or(args);
    if crate::func_args::sig_collects_surplus_args(sig) {
        if let Some(operands) = dynamic_spreads::lower_boxed_spread_args(ctx, sig, args, "", false) {
            return operands;
        }
    }
    if crate::types::call_args::has_named_args(args) {
        begin_deferred_object_param_checks(ctx);
        let operands = lower_named_args_with_signature_options(
            ctx, sig, args, trim_trailing_defaults, capture_values, capture_output_index,
        );
        let sources = vec![super::scalar_param_guards::GuardSource::Unrecorded; operands.len()];
        return coerce_operands_to_params_guarded(ctx, sig, operands, Some(&sources), false);
    }
    if let Some(operands) = lower_positional_spread_args_with_signature(
        ctx, sig, args, None, capture_values, capture_output_index.as_ref(),
    ) {
        return operands;
    }
    let static_spread_args = if has_static_call_spread_args(args) {
        Some(expand_static_call_spread_args(args))
    } else {
        None
    };
    let args = static_spread_args.as_deref().unwrap_or(args);
    if let Some(operands) = lower_assoc_spread_only_args(ctx, sig, args) {
        let sources = vec![super::scalar_param_guards::GuardSource::Unrecorded; operands.len()];
        return coerce_operands_to_params_guarded(ctx, sig, operands, Some(&sources), false);
    }
    if args.iter().any(is_spread_arg) {
        if capture_values {
            return args.iter().enumerate().map(|(index, arg)| {
                if is_spread_arg(arg) {
                    let lowered = lower_expr(ctx, arg);
                    capture_call_argument_value(ctx, lowered, index, arg.span).value
                } else {
                    lower_arg_with_signature_options(ctx, sig, index, arg, true)
                }
            }).collect();
        }
        return lower_runtime_spread_args(ctx, sig, args);
    }
    let regular_param_count = crate::types::call_args::regular_param_count(sig);
    let fixed_arg_count = if sig.variadic.is_some() {
        args.len().min(regular_param_count)
    } else {
        args.len()
    };
    // Statically expanded spread elements have no checker record of their own.
    let expanded_spread = static_spread_args.is_some();
    let guard_sources = (0..regular_param_count)
        .map(|index| match args.get(index) {
            Some(_) if expanded_spread => super::scalar_param_guards::GuardSource::Unrecorded,
            Some(arg) => super::scalar_param_guards::GuardSource::Recorded(arg.span),
            None => super::scalar_param_guards::GuardSource::Skip,
        })
        .collect::<Vec<_>>();
    // Check scalar parameters after every argument was evaluated, as PHP does at the callee.
    let previous_deferral = ctx.scalar_guard_deferral;
    if !capture_values {
        ctx.scalar_guard_deferral = Some((ctx.expression_depth, expanded_spread));
    }
    let operands = lower_positional_operands(
        ctx, sig, args, trim_trailing_defaults, capture_values, capture_output_index,
        regular_param_count, fixed_arg_count,
    );
    ctx.scalar_guard_deferral = previous_deferral;
    let sources = (!capture_values).then_some(guard_sources.as_slice());
    let mut operands = coerce_operands_to_params_guarded(ctx, sig, operands, sources, true);
    // PHP binds the supplied parameters before it evaluates an omitted default, so a refused
    // argument throws `TypeError` before a throwing default (`int $n = 1 % 0`) runs.
    if !trim_trailing_defaults && fixed_arg_count < regular_param_count {
        let throwing_default = sig.defaults.iter().take(regular_param_count).skip(fixed_arg_count)
            .flatten().any(|default| !default_is_inert(default));
        if !capture_values && throwing_default {
            // The checks leave what they converted (a `mixed`-to-string cast) unrooted: claim
            // it as an owned string so the root releases it if the default throws.
            for index in 0..operands.len() {
                if sig.ref_params.get(index).copied().unwrap_or(false) {
                    continue;
                }
                let claimed = super::call_operand_owners::claim_owned_string_operand(
                    ctx, operands[index], Span::dummy(),
                );
                let lowered = lowered_value_from_id(ctx, claimed);
                operands[index] =
                    root_evaluated_call_argument(ctx, lowered, Span::dummy()).value;
            }
        }
        lower_omitted_defaults_and_tail(
            ctx, sig, args, &mut operands, regular_param_count, fixed_arg_count, capture_values,
        );
        // A default still gets the storage normalization (a packed `[]` bound to a hash).
        let defaults_end = operands.len().min(regular_param_count);
        if (fixed_arg_count..defaults_end)
            .any(|index| operand_needs_param_coercion(ctx, sig, &operands, index))
        {
            operands = coerce_operands_to_params(ctx, sig, operands);
        }
    }
    operands
}

/// Returns whether evaluating a default cannot throw: a literal, a `::class` name or a magic
/// constant, their negation, or an array of those. A constant read is not inert: a deprecated
/// one (`MT_RAND_PHP`) raises `E_DEPRECATED`, which a user error handler can turn into an
/// exception, and anything else (an operator such as `1 % 0`, a `new`) may throw, so what the
/// call already holds is rooted first.
pub(super) fn default_is_inert(default: &Expr) -> bool {
    match &default.kind {
        ExprKind::StringLiteral(_) | ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_) | ExprKind::Null | ExprKind::ClassConstant { .. }
        | ExprKind::MagicConstant(_) => true,
        ExprKind::Negate(value) | ExprKind::Not(value) => default_is_inert(value),
        ExprKind::ArrayLiteral(values) => values.iter().all(default_is_inert),
        ExprKind::ArrayLiteralAssoc(entries) => entries.iter()
            .all(|(key, value)| default_is_inert(key) && default_is_inert(value)),
        _ => false,
    }
}

/// Returns whether an omitted parameter's default can throw when it is evaluated.
fn omitted_default_can_throw(sig: &FunctionSig, fixed_arg_count: usize, regular_param_count: usize) -> bool {
    sig.defaults.iter().take(regular_param_count).skip(fixed_arg_count)
        .flatten().any(crate::types::deferred_defaults::contains_deferred_error)
}

/// Appends the omitted parameters' defaults and the (empty) variadic tail of a positional call.
fn lower_omitted_defaults_and_tail(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    args: &[Expr],
    operands: &mut Vec<crate::ir::ValueId>,
    regular_param_count: usize,
    fixed_arg_count: usize,
    capture_values: bool,
) {
    for idx in fixed_arg_count..regular_param_count {
        let Some(Some(default)) = sig.defaults.get(idx) else {
            break;
        };
        let value = lower_arg_with_signature(ctx, sig, idx, default);
        // A later default can still throw (`array $l = [], int $n = 1 % 0`): root this one.
        let later_throws = sig.defaults.iter().take(regular_param_count).skip(idx + 1)
            .flatten().any(|later| !default_is_inert(later));
        let value = if later_throws && !capture_values && !sig.ref_params.get(idx).copied().unwrap_or(false) {
            let lowered = lowered_value_from_id(ctx, value);
            root_evaluated_call_argument(ctx, lowered, default.span).value
        } else {
            value
        };
        operands.push(value);
    }
    if sig.variadic.is_some() {
        let tail = if args.len() > regular_param_count {
            &args[regular_param_count..]
        } else {
            &[]
        };
        if crate::func_args::sig_has_hidden_argc_param(sig) {
            operands.push(emit_i64_at_span(ctx, args.len() as i64, crate::span::Span::dummy()).value);
        }
        operands.push(lower_variadic_tail_array(ctx, sig, tail, args.len()).value);
    }
}

/// Lowers fixed positional arguments in source order, then the variadic tail.
///
/// When a parameter is omitted, only the supplied arguments are returned: the caller checks
/// them and then appends the defaults and the (empty) tail.
#[allow(clippy::too_many_arguments)]
fn lower_positional_operands(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    args: &[Expr],
    trim_trailing_defaults: bool,
    capture_values: bool,
    capture_output_index: Option<(usize, PhpType)>,
    regular_param_count: usize,
    fixed_arg_count: usize,
) -> Vec<crate::ir::ValueId> {
    if sig.variadic.is_none() && fixed_arg_count >= regular_param_count {
        let operands = args
            .iter()
            .enumerate()
            .map(|(index, arg)| {
                let previous = capture_output_index.as_ref().filter(|(output, _)| *output == index)
                    .and_then(|(_, ty)| if capture_values {
                        promote_captured_reference_argument(ctx, arg); None
                    } else { prepare_captured_output_argument(ctx, arg, ty) });
                let value = lower_arg_with_signature_options(ctx, sig, index, arg, capture_values);
                if let Some((name, ty)) = previous { ctx.set_local_logical_type(&name, ty); }
                if !capture_values && index + 1 < args.len() {
                    root_prior_argument_preserving_reference_place(
                        ctx, sig, index, arg, value,
                    )
                } else {
                    value
                }
            })
            .collect();
        return operands;
    }
    let mut operands: Vec<crate::ir::ValueId> = args[..fixed_arg_count]
        .iter()
        .enumerate()
        .map(|(index, arg)| {
            let previous = capture_output_index.as_ref().filter(|(output, _)| *output == index)
                .and_then(|(_, ty)| if capture_values {
                    promote_captured_reference_argument(ctx, arg); None
                } else { prepare_captured_output_argument(ctx, arg, ty) });
            let value = lower_arg_with_signature_options(ctx, sig, index, arg, capture_values);
            if let Some((name, ty)) = previous { ctx.set_local_logical_type(&name, ty); }
            // Omitted defaults execute after every caller expression, including the last one.
            let deferred_default = !trim_trailing_defaults
                && omitted_default_can_throw(sig, fixed_arg_count, regular_param_count);
            if !capture_values && (index + 1 < args.len() || deferred_default) {
                root_prior_argument_preserving_reference_place(
                    ctx, sig, index, arg, value,
                )
            } else {
                value
            }
        })
        .collect();
    // Omitted defaults (and the then empty variadic tail) are lowered by the caller once the
    // supplied parameters were checked: PHP binds those first.
    if !trim_trailing_defaults && fixed_arg_count < regular_param_count {
        return operands;
    }
    if sig.variadic.is_some() {
        let tail = if args.len() > regular_param_count {
            &args[regular_param_count..]
        } else {
            &[]
        };
        if crate::func_args::sig_has_hidden_argc_param(sig) {
            operands.push(emit_i64_at_span(ctx, args.len() as i64, crate::span::Span::dummy()).value);
        }
        operands.push(lower_variadic_tail_array(ctx, sig, tail, args.len()).value);
    }
    operands
}

/// Returns the declaring class and declared type of `property` on `class_name`.
///
/// Untyped properties accept every PHP value regardless of the storage type elephc inferred
/// from their default, so only a declared slot may be re-validated after a by-reference call.
/// The declaring class is what PHP names in the reference-held diagnostic, which can be a
/// parent of the receiver's static class.
fn declared_property_type(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    property: &str,
) -> Option<(String, PhpType)> {
    let class_name = class_name.trim_start_matches('\\');
    let info = ctx.classes.get(class_name)?;
    let index = info.visible_property_index(property)?;
    if !info.property_slot_is_declared(index, property) {
        return None;
    }
    let (_, (_, ty)) = info.visible_property(property)?;
    let declaring = info
        .property_declaring_classes
        .get(property)
        .cloned()
        .unwrap_or_else(|| class_name.to_string());
    Some((declaring, ty.clone()))
}

/// Resolves the statically known receiver class for a property argument.
///
/// Mirrors the checker's `single_object_class_name`: a nullable property or parameter narrows
/// to its object member, and a nested property read is resolved through the declaring class's
/// visible property type. Shapes the checker would not accept (calls, array elements) answer
/// `None` here as well.
fn receiver_class_name(ctx: &LoweringContext<'_, '_>, object: &Expr) -> Option<String> {
    let ty = match &object.kind {
        ExprKind::Variable(name) => ctx.local_type(name),
        ExprKind::This => ctx
            .current_class
            .as_ref()
            .map(|class| PhpType::Object(class.clone()))
            .unwrap_or(PhpType::Mixed),
        ExprKind::PropertyAccess { object: parent, property } => {
            let parent_class = receiver_class_name(ctx, parent)?;
            let info = ctx.classes.get(parent_class.trim_start_matches('\\'))?;
            info.visible_property(property)?.1.1.clone()
        }
        _ => infer_expr_type_syntactic(object),
    };
    crate::types::checker::single_object_class_name(&ty)
}

/// Records one property by-reference type check for the enclosing call's boundary pass.
///
/// Returns false when the argument is lowered outside an argument-evaluation scope; the
/// caller then emits the same check immediately.
fn defer_property_type_guard(
    ctx: &mut LoweringContext<'_, '_>,
    guard: crate::ir_lower::context::PendingPropertyTypeGuard,
) -> bool {
    let Some(scope) = ctx.call_argument_evaluation_scopes.last_mut() else {
        return false;
    };
    scope.pending_property_type_guards.push(guard);
    true
}

/// Emits one property by-reference type check after the complete argument list evaluated.
///
/// A conditional check branches to the call-site `TypeError` when the current value does not
/// satisfy the parameter; its accepted branch retires the raw temporary. An unconditional
/// check raises the statically known diagnostic and leaves the following call unreachable.
pub(super) fn emit_property_type_guard(
    ctx: &mut LoweringContext<'_, '_>,
    guard: &crate::ir_lower::context::PendingPropertyTypeGuard,
) {
    if let (Some(place), Some(temp)) = (guard.place.as_ref(), guard.temp.as_ref()) {
        let read = lower_expr(ctx, place);
        store_value_into_temp(
            ctx,
            temp,
            PhpType::Mixed,
            read,
            guard.span,
        );
        let temp_expr = Expr::new(ExprKind::Variable(temp.clone()), guard.span);
        let accepted = if guard.coerces_weakly && !crate::source::current_strict_types() {
            super::enum_argument_guards::accepts_type(&guard.expected, &temp_expr, guard.span)
        } else {
            crate::types::param_binding::runtime_reference_param_acceptance(
                &guard.expected,
                &temp_expr,
            )
        };
        let Some(accepted) = accepted else {
            let null = ctx.emit_value(
                Op::ConstNull,
                Vec::new(),
                None,
                PhpType::Void,
                Op::ConstNull.default_effects(),
                Some(guard.span),
            );
            ctx.unset_local(temp, null, Some(guard.span));
            return;
        };
        let condition = lower_expr(ctx, &accepted).value;
        let ok = ctx
            .builder
            .create_named_block("ref_property.type.ok", Vec::new());
        let bad = ctx
            .builder
            .create_named_block("ref_property.type.bad", Vec::new());
        ctx.builder.terminate(Terminator::CondBr {
            cond: condition,
            then_target: ok,
            then_args: Vec::new(),
            else_target: bad,
            else_args: Vec::new(),
        });
        ctx.builder.position_at_end(bad);
        let value_type = super::enum_argument_guards::runtime_type(
            &temp_expr,
            &guard.expected,
            guard.span,
        );
        let given_value = super::enum_argument_guards::concat(
            value_type,
            Expr::new(ExprKind::StringLiteral(" given".to_string()), guard.span),
            guard.span,
        );
        let message = super::enum_argument_guards::concat(
            Expr::new(ExprKind::StringLiteral(guard.prefix.clone()), guard.span),
            given_value,
            guard.span,
        );
        super::enum_argument_guards::throw_message(ctx, "TypeError", message, guard.span);
        ctx.builder.position_at_end(ok);
        // Convert into the callee's temporary only after the current value passed the
        // check: an array or object rejected by `string &$slot` must raise the argument
        // TypeError, never run an array-to-string warning or `__toString()` first.
        if let (Some(callee_temp), Some(callee_ty)) =
            (guard.callee_temp.as_ref(), guard.callee_temp_ty.as_ref())
        {
            let snapshot = ctx.load_local(temp, Some(guard.span));
            let snapshot = if callee_ty.codegen_repr() == PhpType::Bool {
                super::unary_logic::lower_truthy_bool(ctx, snapshot, Some(guard.span))
            } else {
                super::merge_temps::coerce_value_for_temp(
                    ctx,
                    snapshot,
                    callee_ty,
                    guard.span,
                )
            };
            ctx.store_local(callee_temp, snapshot, callee_ty.clone(), Some(guard.span));
        }
        let null = ctx.emit_value(
            Op::ConstNull,
            Vec::new(),
            None,
            PhpType::Void,
            Op::ConstNull.default_effects(),
            Some(guard.span),
        );
        ctx.unset_local(temp, null, Some(guard.span));
        return;
    }
    if let Some(message) = &guard.unconditional_message {
        let data = ctx.intern_string(message);
        ctx.emit_void(
            Op::ReferenceArgumentTypeError,
            Vec::new(),
            Some(Immediate::Data(data)),
            Op::ReferenceArgumentTypeError.default_effects(),
            Some(guard.span),
        );
    }
}

/// Returns whether a by-reference property argument uses the boxed temporary read/write-back
/// path instead of binding the property's own reference cell.
///
/// Declared properties, `mixed`/union storage, and an untyped property with a recorded
/// call-boundary type guard all need that temporary; an untyped property without a guard keeps
/// its ordinary storage path.
fn property_argument_is_declared(ctx: &LoweringContext<'_, '_>, arg: &Expr) -> bool {
    let ExprKind::PropertyAccess { object, property } = &arg.kind else {
        return false;
    };
    // An untyped property with a recorded call-boundary guard needs the same temporary
    // read/write-back path: its storage can be retyped by the coercion or by the callee.
    if matches!(
        ctx.throw_access_site(arg.span).map(|info| &info.kind),
        Some(crate::types::ThrowAccessKind::ReferenceArgumentType { .. })
    ) {
        return true;
    }
    let Some(class) = receiver_class_name(ctx, object) else {
        return false;
    };
    ctx.classes
        .get(class.trim_start_matches('\\'))
        .is_some_and(|info| {
            info.visible_property_is_declared(property)
                || info.visible_property(property).is_some_and(|(_, (_, ty))| {
                    matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
                })
        })
}

/// Lowers a call whose runtime spread the callee binds itself, then checks and converts the
/// positional arguments written before the first spread.
///
/// PHP evaluates every argument, the spread included, before the callee checks any parameter,
/// so the prefix is checked (in parameter order, like any other call) only after the spread
/// source was evaluated. Without this the prefix reached a declared scalar parameter unchecked:
/// `$o->m($null, ...side())` bound `null` as `""` instead of throwing `TypeError`.
fn lower_runtime_spread_args(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    args: &[Expr],
) -> Vec<crate::ir::ValueId> {
    let mut operands = lower_args(ctx, args);
    let prefix_len = args
        .iter()
        .take_while(|arg| !is_spread_arg(arg) && !matches!(arg.kind, ExprKind::NamedArg { .. }))
        .count()
        .min(crate::types::call_args::regular_param_count(sig))
        .min(operands.len());
    if prefix_len == 0 {
        return operands;
    }
    // The prefix check can throw: publish the spread operands first, or a freshly built
    // spread array (the last argument, which argument evaluation never roots) leaks.
    let rest = operands
        .split_off(prefix_len)
        .into_iter()
        .map(|operand| {
            let lowered = lowered_value_from_id(ctx, operand);
            root_evaluated_call_argument(ctx, lowered, Span::dummy()).value
        })
        .collect::<Vec<_>>();
    let sources = args[..prefix_len]
        .iter()
        .map(|arg| super::scalar_param_guards::GuardSource::Recorded(arg.span))
        .collect::<Vec<_>>();
    let mut operands = coerce_operands_to_params_guarded(ctx, sig, operands, Some(&sources), false);
    operands.extend(rest);
    operands
}
