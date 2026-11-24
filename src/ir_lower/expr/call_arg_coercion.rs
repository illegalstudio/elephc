//! Purpose:
//! General call argument lowering and parameter storage coercion.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

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
    if sig.ref_params.get(index).copied().unwrap_or(false)
        && matches!(&arg.kind, ExprKind::ArrayAccess { array, .. }
            if instance_object_class_for_ir(ctx, array)
                .is_some_and(|class| class.eq_ignore_ascii_case("WeakMap")))
    {
        let alias = ctx.declare_synthetic_php_local(PhpType::Mixed);
        lower_ref_assign_array_elem(ctx, &alias, arg, arg.span);
        publish_scoped_ref_receiver_alias(ctx, &alias, arg.span);
        let variable = Expr::new(ExprKind::Variable(alias), arg.span);
        return lower_arg_with_signature(ctx, sig, index, &variable);
    }
    if sig.ref_params.get(index).copied().unwrap_or(false)
        && matches!(
            &arg.kind,
            ExprKind::ArrayAccess { array, .. }
                if !matches!(&array.kind, ExprKind::Variable(_))
        )
    {
        if let Some((alias, aliases)) = prepare_scoped_addressable_ref_array_receiver(ctx, arg) {
            let value = lower_arg_with_signature(ctx, sig, index, &alias);
            retire_scoped_ref_receiver_aliases(ctx, &aliases);
            return value;
        }
    }
    if let Some(value) = lower_by_ref_array_element_arg_with_signature(ctx, sig, index, arg) {
        return value;
    }
    if let Some(value) = lower_by_ref_property_arg_with_signature(ctx, sig, index, arg) {
        return value;
    }
    if let Some(value) = lower_global_storage_ref_arg(ctx, sig, index, arg) {
        return value;
    }
    if let Some(value) = lower_by_ref_array_arg_with_signature(ctx, sig, index, arg) {
        return value;
    }
    guard_boxed_object_reference_argument(ctx, sig, index, arg);
    promote_boxed_reference_local_argument(ctx, sig, index, arg);
    promote_reference_return_local_argument(ctx, sig, index, arg);
    // A property by-reference rewrite stages `$temp = <place>` as the argument so the read
    // happens at this argument's source position. Execute that assignment now, then pass the
    // hidden local's storage exactly like an ordinary variable argument.
    if sig.ref_params.get(index).copied().unwrap_or(false) {
        if let ExprKind::Assignment { target, .. } = &arg.kind {
            if let ExprKind::Variable(name) = &target.kind {
                if name.starts_with(crate::names::SYNTHETIC_PLACE_LOCAL_STEM) {
                    let assigned = lower_expr(ctx, arg);
                    if ctx.value_is_owning_temporary(assigned) {
                        crate::ir_lower::ownership::release_if_owned(ctx, assigned, Some(arg.span));
                    }
                    let variable = Expr::new(ExprKind::Variable(name.clone()), arg.span);
                    return lower_arg_with_signature(ctx, sig, index, &variable);
                }
            }
        }
    }
    note_by_ref_argument_param_slot(ctx, sig, index, arg);
    if let Some(lowered) = lower_tracked_callable_array_param(ctx, sig, index, arg) {
        return lowered.value;
    }
    let reference_local = sig.ref_params.get(index).copied().unwrap_or(false)
        .then(|| local_values::lower_reference_local(ctx, arg)).flatten();
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
    // A receiver that is statically a PHP array (a bare `array` parameter included) is never a
    // SimpleXML view: its elements are plain values, so no passing-mode query is needed.
    if type_is_statically_array(&ty) { return false; }
    matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
        || singular_object_class(&ty).is_some_and(|(class, _)| class.is_empty() || ctx.interfaces.contains_key(class))
        || may_be_native_selection_reference_argument(ctx, receiver)
}

/// Whether `ty` can only hold PHP arrays (list or hash), allowing absent values.
///
/// A bare `array` declaration reaches lowering as a union of the two array storages, which the
/// erased-receiver test would otherwise mistake for a possible SimpleXML family.
fn type_is_statically_array(ty: &PhpType) -> bool {
    match ty {
        PhpType::Array(_) | PhpType::AssocArray { .. } => true,
        PhpType::Union(members) => members.iter().any(type_is_statically_array)
            && members.iter().all(|member| matches!(member, PhpType::Void | PhpType::Never)
                || type_is_statically_array(member)),
        _ => false,
    }
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

/// Records a by-value parameter passed by reference, so the frame owns what the callee replaces.
fn note_by_ref_argument_param_slot(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    arg: &Expr,
) {
    if !sig.ref_params.get(index).copied().unwrap_or(false) {
        return;
    }
    let ExprKind::Variable(name) = &arg.kind else {
        return;
    };
    if let Some(slot) = ctx.local_slots.get(name).copied() {
        ctx.builder.note_by_ref_argument_param_slot(slot);
    }
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
/// Only a non-nullable class parameter is checked here: inside the callee its slot reads an
/// object pointer. A union of `null`, arrays and objects (`?A &$v`, `?array &$v`) is a boxed
/// cell in the callee and is checked on entry instead, after every argument was evaluated
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
    let declared_object = match sig.params.get(index).map(|(_, ty)| ty) {
        Some(PhpType::Object(class_name)) => Some(class_name.as_str()),
        Some(PhpType::Mixed) => match sig.param_type_exprs.get(index).and_then(Option::as_ref) {
            // Only a CLASS name: `array &$x` widened to a boxed cell
            // (`types::checker::ref_param_widening`) is still spelled `array`, and checking it
            // with `instanceof array` refused every call ("Argument must be of type array").
            Some(TypeExpr::Named(name))
                if !matches!(
                    name.as_str().to_ascii_lowercase().as_str(),
                    "mixed"
                        | "array"
                        | "iterable"
                        | "callable"
                        | "string"
                        | "int"
                        | "float"
                        | "bool"
                        | "false"
                        | "true"
                        | "null"
                        | "void"
                        | "never"
                ) =>
            {
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
/// `mixed`, an object, a union of objects only (`A|B`), or a NULLABLE union of objects, arrays
/// and floats (`?Base`, `?array`, `?float`). The checker authorizes promotion only for a whole
/// local it gave boxed storage: one that may receive `null` (or a sibling class) through such a
/// parameter, which its typed slot could not hold.
fn boxed_reference_param_type(ty: &PhpType) -> bool {
    match ty {
        PhpType::Mixed | PhpType::Object(_) => true,
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
/// The descriptor is normalized FROM THE ARRAY, so it keeps the array kind. A `callable`
/// parameter holding `[$object, 'method']` is still that array in PHP: `is_array()` is true, it
/// destructures and spreads into its two elements. This used to build a first-class callable
/// for the target instead, which is a Closure: Symfony's `ControllerEvent::setController(callable
/// $controller)` then took the `\ReflectionFunction` arm of its `match`, never reflected the
/// controller's class, and every request died in argument resolution. The array is lowered once,
/// so its receiver is still evaluated exactly once.
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
    match static_callable_binding_for_expr(ctx, arg)? {
        StaticCallableBinding::StaticMethodDescriptor { .. }
        | StaticCallableBinding::InstanceMethod {
            direct_call: true,
            ..
        } => {}
        StaticCallableBinding::UserFunction(_)
        | StaticCallableBinding::ExternFunction(_)
        | StaticCallableBinding::Builtin(_)
        | StaticCallableBinding::Closure { .. }
        | StaticCallableBinding::StaticMethod { .. }
        | StaticCallableBinding::InstanceMethod {
            direct_call: false,
            ..
        } => return None,
    }
    let array = lower_expr(ctx, arg);
    // The descriptor built from a callable array is fresh and retains its receiver, so this
    // argument OWNS it and the call boundary must release it. Left `maybe_owned`, nobody did:
    // `apply([$obj, 'm'], 7)` leaked the 96-byte bound descriptor and one receiver reference
    // per call, where the pre-merge lowering (`FirstClassCallableNew`) released it.
    let normalized = ctx.emit_owned_value(
        Op::NormalizeCallable,
        vec![array.value],
        None,
        PhpType::Callable,
        Op::NormalizeCallable.default_effects(),
        Some(arg.span),
    );
    if ctx.value_is_owning_temporary(array) {
        crate::ir_lower::ownership::release_if_owned(ctx, array, Some(arg.span));
    }
    Some(normalized)
}

/// Gives an escaping by-reference return a managed owner for a caller local.
///
/// A reference-returning callee can hand a by-reference parameter's address back to the caller.
/// A raw caller frame address has no `__rt_reference_cell_owner`, so promote the local before the
/// call and pass a fresh `LoadRefCell` marker that resolves to the managed cell pointer. Array
/// normalization runs first so the cell stores the final parameter-compatible representation.
///
/// A parameter some body binds a property to (`$this->p = &$param`) keeps the cell for the
/// object's life, so it needs the same managed cell (`bind_property_to_param_ref_cell`).
fn promote_reference_return_local_argument(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    arg: &Expr,
) {
    if !sig.ref_params.get(index).copied().unwrap_or(false) {
        return;
    }
    if !sig.by_ref_return && !ctx.property_bound_promotion_is_authorized(arg.span) {
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

/// Coerces a positional argument to the parameter storage owned explicitly by EIR.
///
/// Integer-to-float conversion selects the callee's floating-point ABI class. Mixed-to-string
/// conversion is also explicit here because it allocates caller-owned storage whose lifetime
/// depends on the call's return/argument alias contract; leaving that conversion hidden in ABI
/// materialization would give EIR no value to transfer or release after the call.
/// Converts a weak-mode string argument to an `int` parameter the way php does when the call
/// runs, for a binding the checker recorded as `ThrowAccessKind::CallArgumentIntCoercion`: a
/// numeric string binds, anything else throws php's `TypeError` naming the call site. The probe
/// reads the string in place, so the throw path has nothing of its own to release.
fn coerce_weak_int_argument(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    arg: &Expr,
) -> Option<LoweredValue> {
    let scope = ctx.loop_storage_scope.clone();
    let (prefix, internal_callee, callee_decl) =
        match ctx.throw_access_sites.get(&(scope, arg.span)).map(|info| &info.kind) {
            Some(crate::types::ThrowAccessKind::CallArgumentIntCoercion {
                prefix,
                internal_callee,
                callee_decl,
            }) => (prefix.clone(), *internal_callee, *callee_decl),
            _ => return None,
        };
    let given = match ctx.builder.value_php_type(value.value).codegen_repr() {
        PhpType::Float => "float",
        _ => "string",
    };
    // php names no call site for a function it implements internally.
    let message = if internal_callee {
        format!("{}{} given", prefix, given)
    } else {
        let file = definite_argument_errors::call_site_file(ctx, arg.span);
        format!("{}{} given, called in {} on line {}", prefix, given, file, arg.span.line)
    };
    // A user function's binding error is raised inside the callee: probe the string first, and
    // on refusal call the callee-framed thunk. The accepting branch runs the ordinary conversion.
    if let (Some(decl), "string", Some((callee_label, _))) =
        (callee_decl, given, prefix.split_once("(): "))
    {
        let probe_data = ctx.intern_string("");
        let accepted = ctx.emit_value(
            Op::ArgumentStringToInt,
            vec![value.value],
            Some(Immediate::Data(probe_data)),
            PhpType::Bool,
            Op::ArgumentStringToInt.default_effects(),
            Some(arg.span),
        );
        let refused_block = ctx.builder.create_named_block("bind.refused", Vec::new());
        let accepted_block = ctx.builder.create_named_block("bind.accepted", Vec::new());
        ctx.builder.terminate(Terminator::CondBr {
            cond: accepted.value,
            then_target: accepted_block,
            then_args: Vec::new(),
            else_target: refused_block,
            else_args: Vec::new(),
        });
        ctx.builder.position_at_end(refused_block);
        let boxed = ctx.box_value_as_mixed(value, PhpType::Mixed, Some(arg.span));
        definite_argument_errors::call_binding_error_thunk(
            ctx, arg.span, callee_label, &message, decl, &[boxed.value],
        );
        ctx.builder.terminate(Terminator::Unreachable);
        ctx.builder.position_at_end(accepted_block);
    }
    let data = ctx.intern_string(&message);
    Some(ctx.emit_value(
        Op::ArgumentStringToInt,
        vec![value.value],
        Some(Immediate::Data(data)),
        PhpType::Int,
        Op::ArgumentStringToInt.default_effects(),
        Some(arg.span),
    ))
}

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
    // PHP accepts a union that names the declared class plus one of `false`/`null` at compile
    // time and throws `TypeError` at runtime for the other alternative. Extract the object
    // payload only after the instance check, so the callee never receives a boxed cell where
    // its ABI expects a raw object pointer.
    let by_reference = sig.ref_params.get(index).copied().unwrap_or(false);
    // The bare `object` type is spelled `Object("")`: `guard_union_object_param` tests object-ness
    // for it instead of an instance of a class, and names the parameter in its TypeError.
    if !by_reference {
        if let PhpType::Object(class_name) = param_ty {
        {
            let source_ty = ctx.builder.value_php_type(value.value);
            // A union-typed local is stored in a boxed Mixed slot, so its IR type no longer
            // names the `Class|false` alternative. Recover the checker's static type from the
            // local/place facts before deciding not to emit the guard.
            let static_ty = argument_static_type(ctx, arg);
            let guard_ty = static_ty.as_ref().unwrap_or(&source_ty);
            let static_given = union_object_guard_word(guard_ty, class_name);
            if let Some(given) = static_given {
                return guard_union_object_param(
                    ctx, sig, index, value, class_name, Some(given), arg,
                );
            }
            // A union-typed local collapsed to a boxed Mixed cell, or an optimized-away
            // literal, still reaches an object parameter. Guard every source the predicate
            // above did not describe unless it is already statically the declared class, and
            // only re-read expressions whose evaluation is side-effect free.
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
            // A statically-known object already carries the representation the callee's ABI
            // expects. Subclasses and generic instantiations are accepted by the checker under
            // their own class names, so a runtime InstanceOf here would reject `Holder<Dog>` for
            // `Holder<Animal>` or a legitimate subclass. The guard is only for runtime-typed
            // storage and values optimized from a union.
            let already_object = matches!(guard_ty.codegen_repr(), PhpType::Object(_));
            // A literal null is not the boxed union payload this guard exists for. Direct
            // calls reject it in the checker; a dynamic callable keeps the null-receiver path.
            let literal_null = matches!(arg.kind, ExprKind::Null);
            if rerunnable && !already_object && !literal_null {
                return guard_union_object_param(
                    ctx, sig, index, value, class_name, None, arg,
                );
            }
        }
        }
    }
    // A by-reference parameter must receive the caller's storage, not a converted temporary,
    // so declared-parameter scalar binding never applies to one. The checker keeps those on
    // the strict path for the same reason.
    let bindable = sig.declared_params.get(index).copied().unwrap_or(false)
        && !sig.ref_params.get(index).copied().unwrap_or(false);
    let declared_param_ty = param_ty;
    let param_ty = declared_param_ty.codegen_repr();
    if value.ir_type == IrType::I64 && param_ty == PhpType::Float {
        return coerce_to_float(ctx, value, arg);
    }
    let source_php_ty = ctx.builder.value_php_type(value.value).clone();
    let source_ty = source_php_ty.codegen_repr();
    // The union's boxed storage (`Mixed`) is what the later boxing returns early with, so the
    // class check has to come first.
    let value = if bindable {
        guard_object_class_union_param(ctx, value, declared_param_ty, sig, index, arg)
    } else {
        value
    };
    if bindable
        && !ctx.php_strict_types
        && is_nullable_string_param(declared_param_ty)
        && matches!(source_ty, PhpType::Mixed | PhpType::Union(_))
    {
        return coerce_gradual_nullable_string_param(
            ctx,
            value,
            declared_param_ty,
            Some(arg.span),
        );
    }
    // A union of `Stringable` objects (and null) the checker bound through the declaration's
    // string arm: each object converts as php's weak mode does, and null stays null.
    if bindable && !ctx.php_strict_types {
        let static_ty = argument_static_type(ctx, arg).unwrap_or_else(|| source_php_ty.clone());
        if let Some(classes) =
            crate::types::param_binding::object_union_string_binding_classes(declared_param_ty, &static_ty)
        {
            let binds_through_string = classes.iter().all(|class_name| {
                !param_accepts_object_without_string_coercion(
                    ctx,
                    declared_param_ty,
                    &PhpType::Object((*class_name).to_string()),
                )
            });
            if binds_through_string {
                return apply_scalar_param_cast(
                    ctx,
                    CastType::String,
                    value,
                    declared_param_ty,
                    Some(arg.span),
                );
            }
        }
    }
    if param_ty == PhpType::Callable
        && !sig.ref_params.get(index).copied().unwrap_or(false)
        && matches!(source_ty, PhpType::Mixed | PhpType::Union(_))
    {
        // Only a parameter written `callable` refuses null: an untyped one the checker merely
        // specialized to `callable` from another call site keeps php's untouched null.
        let diagnostic = matches!(
            sig.param_type_exprs.get(index).and_then(Option::as_ref),
            Some(TypeExpr::Named(name)) if name.as_str().eq_ignore_ascii_case("callable")
        )
        .then(|| callable_param_type_error(ctx, sig, index, arg));
        return unbox_callable_param_storage_with(ctx, value, Some(arg.span), diagnostic);
    }
    if param_ty == PhpType::Iterable
        && !sig.ref_params.get(index).copied().unwrap_or(false)
        && matches!(source_ty, PhpType::Mixed | PhpType::Union(_))
    {
        let diagnostic = iterable_param_type_error(ctx, sig, index, arg);
        return unbox_iterable_param_storage_with(ctx, value, Some(arg.span), Some(diagnostic));
    }
    // A `callable` parameter's storage is a DESCRIPTOR, and an `[$object, 'method']` pair is a
    // hash pointer: handing one straight over type-checks and then faults on the first call
    // through it. `Op::NormalizeCallable` is the conversion the callable-array path already uses,
    // and it owns its result, which the ordinary argument-ownership rules then release.
    if param_ty == PhpType::Callable
        && matches!(
            &source_ty,
            PhpType::Array(element)
                if matches!(element.codegen_repr(), PhpType::Mixed | PhpType::Str)
        )
    {
        return ctx.emit_value(
            Op::NormalizeCallable,
            vec![value.value],
            None,
            PhpType::Callable,
            Op::NormalizeCallable.default_effects(),
            Some(arg.span),
        );
    }
    // An INVOKABLE OBJECT is no descriptor either: handing it over as the callable's storage made
    // the first call through the parameter jump into the object's first word.
    if param_ty == PhpType::Callable
        && !sig.ref_params.get(index).copied().unwrap_or(false)
        && matches!(&source_ty, PhpType::Object(_))
        && !source_ty.is_closure_object()
    {
        return ctx.emit_owned_value(
            Op::NormalizeCallable,
            vec![value.value],
            None,
            PhpType::Callable,
            Op::NormalizeCallable.default_effects(),
            Some(arg.span),
        );
    }
    // A null reaching a non-nullable `int` parameter is php's `TypeError` in weak mode too.
    if bindable
        && *declared_param_ty == PhpType::Int
        && !sig.ref_params.get(index).copied().unwrap_or(false)
        && matches!(
            source_ty,
            PhpType::TaggedScalar | PhpType::Mixed | PhpType::Union(_) | PhpType::Void
        )
    {
        guard_null_for_int_param(ctx, value, sig, index, arg);
    }
    // An object that only the weak `string` arm of the declaration accepts (`Stringable` passed
    // to `string|iterable`) must convert before boxing; boxing it first hands the callee the
    // object under a union whose string member PHP would have selected.
    let weak_string_object = matches!(source_ty, PhpType::Object(_))
        && !param_accepts_object_without_string_coercion(ctx, declared_param_ty, &source_ty)
        && crate::types::param_binding::scalar_param_cast(declared_param_ty, &source_php_ty)
            .is_some();
    // A weak-mode scalar the union only accepts through a scalar arm (`42` or `false` passed to
    // `string|array|null`) converts the same way: php selects the union's sole string arm, so the
    // callee must see `"42"` / `""`, not a boxed int or bool.
    let weak_scalar_cast = !ctx.php_strict_types
        && matches!(
            source_ty,
            PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::False
        )
        && crate::types::param_binding::scalar_param_cast(declared_param_ty, &source_php_ty)
            .is_some();
    // A `?int`-shaped value is only known at run time to be an int or a null. When the union
    // takes the int only through its string arm (`string|array|null`), the null/scalar split
    // converts the int arm to `"7"`, as php's weak mode does.
    let weak_tagged_scalar = !ctx.php_strict_types
        && source_ty == PhpType::TaggedScalar
        && crate::types::param_binding::scalar_param_cast(declared_param_ty, &PhpType::Int)
            == Some(crate::parser::ast::CastType::String);
    if bindable && weak_tagged_scalar && !declared_type_admits_null(declared_param_ty) {
        let param_name = sig.params.get(index).map(|(name, _)| name.as_str()).unwrap_or("");
        return coerce_nullable_int_to_string_param(
            ctx,
            value,
            &format!(
                "Argument #{} (${}) must be of type {}, null given",
                index + 1,
                param_name,
                declared_param_ty
            ),
            arg.span,
        );
    }
    if bindable && weak_tagged_scalar {
        return coerce_gradual_nullable_string_param(
            ctx,
            value,
            declared_param_ty,
            Some(arg.span),
        );
    }
    if bindable
        && param_ty == PhpType::Mixed
        && source_ty != PhpType::Mixed
        && !weak_string_object
        && !weak_scalar_cast
        && !weak_tagged_scalar
    {
        return ctx.box_value_as_mixed(value, PhpType::Mixed, Some(arg.span));
    }
    if param_ty == PhpType::Str
        && matches!(source_ty, PhpType::Mixed | PhpType::TaggedScalar | PhpType::Union(_))
    {
        return coerce_to_string(ctx, value, arg);
    }
    if bindable && param_ty == PhpType::Int && source_ty == PhpType::Str {
        if let Some(coerced) = coerce_weak_int_argument(ctx, value, arg) {
            return coerced;
        }
    }
    // A float bound to `int` or `?int` (`new TooManyLoginAttemptsAuthenticationException(
    // ceil(...))` in Symfony's `LoginThrottlingListener`): converted like a string, then bound
    // as the int it became.
    if bindable
        && source_ty == PhpType::Float
        && (param_ty == PhpType::Int
            || crate::types::param_binding::is_nullable_int(declared_param_ty))
    {
        if let Some(coerced) = coerce_weak_int_argument(ctx, value, arg) {
            if param_ty == PhpType::Int {
                return coerced;
            }
            return coerce_scalar_arg_to_param_storage(ctx, sig, index, coerced, arg);
        }
    }
    if (bindable || matches!(source_ty, PhpType::Object(_)))
        && !param_accepts_object_without_string_coercion(ctx, declared_param_ty, &source_ty)
    {
        if let Some(cast) =
            crate::types::param_binding::scalar_param_cast(declared_param_ty, &source_php_ty)
        {
            return apply_scalar_param_cast(
                ctx,
                cast,
                value,
                declared_param_ty,
                Some(arg.span),
            );
        }
    }
    if sig.ref_params.get(index).copied().unwrap_or(false) {
        return value;
    }
    let value = if bindable {
        guard_nominal_object_param(ctx, value, declared_param_ty, Some(arg.span))
    } else {
        value
    };
    let value = if bindable {
        guard_nullable_int_param(ctx, value, declared_param_ty, Some(arg.span))
    } else {
        value
    };
    let value = if bindable {
        guard_gradual_union_param(ctx, value, declared_param_ty, Some(arg.span))
    } else {
        value
    };
    let source_ty = ctx.builder.value_php_type(value.value).codegen_repr();
    // An UNDECLARED parameter whose inferred type is a container is still a boxed `mixed` cell in
    // the callee's EIR contract (`eir_signature_with_php_param_contracts`), so a boxed argument is
    // already in the shape the callee reads. Narrowing it to the inferred container type would
    // demand an array of something php lets the function receive as anything: the recursive
    // `dumpv($item)` of a walker over `preg_match_all()`'s matches threw "Value must be of type
    // array, string given" for the first string element.
    if !sig.declared_params.get(index).copied().unwrap_or(false)
        && matches!(param_ty, PhpType::Array(_) | PhpType::AssocArray { .. })
        && matches!(source_ty, PhpType::Mixed | PhpType::Union(_))
    {
        return value;
    }
    // A `false` parameter accepts only the exact false bool: the boxed argument crosses as is so
    // the backend's false-parameter guard raises php's `TypeError`. The bool cast below would
    // fold `0`, `""` and `[]` into `false` first.
    if *declared_param_ty == PhpType::False && matches!(source_ty, PhpType::Mixed) {
        return value;
    }
    // A builtin's `float` parameter reads a checked int|float arithmetic result natively and
    // nothing in that result can raise a TypeError, so the boundary cast would only hide the
    // operand shape the builtin-specific passes match (the boxed `sqrt($i + c)` range reducer).
    let builtin_argument = ctx.builtin_arg_sig == Some(sig as *const FunctionSig as usize)
        && !sig.declared_params.get(index).copied().unwrap_or(false);
    if builtin_argument
        && param_ty == PhpType::Float
        && super::integer_destinations::checked_integer_tree(ctx, value.value)
    {
        return value;
    }
    // A builtin reads a typed scalar array natively: widening it to `array<mixed>` here would
    // allocate a boxed copy per call and keep the array operand from being hoisted out of loops.
    if builtin_argument
        && matches!(&param_ty, PhpType::Array(element) if element.codegen_repr() == PhpType::Mixed)
        && matches!(&source_ty, PhpType::Array(element)
            if matches!(element.codegen_repr(), PhpType::Int | PhpType::Float))
    {
        return value;
    }
    let value = crate::ir_lower::expr::coerce_container_to_mixed_payload(
        ctx,
        value,
        &source_ty,
        &param_ty,
        Some(arg.span),
    );
    crate::ir_lower::gradual_coercions::coerce_gradual_value_to_boundary(
        ctx,
        value,
        &param_ty,
        Some(arg.span),
    )
}

/// Throws php's `TypeError` for an object that is none of the classes a declared union of
/// several classes (`ConfigurationInterface|ConfigurationExtensionInterface`) names.
///
/// The checker admits an object whose static class could still be one of them
/// (`object_argument_may_narrow_to`), and the single-class guard below cannot test two targets, so
/// the call would otherwise run into the callee with an object it cannot use. Members the
/// argument's own class already satisfies return before any probe is emitted, so the
/// `instanceof` chain below can only be answered by the object itself (or proven false).
///
/// The message names the argument's STATIC class; php names the runtime one, which differs only
/// when the static type is an interface or a base class.
fn guard_object_class_union_param(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    expected: &PhpType,
    sig: &FunctionSig,
    index: usize,
    arg: &Expr,
) -> LoweredValue {
    let PhpType::Object(source_class) = ctx.builder.value_php_type(value.value).codegen_repr()
    else {
        return value;
    };
    let source_class = source_class.trim_start_matches('\\').to_string();
    let PhpType::Union(members) = expected else {
        return value;
    };
    if source_class.is_empty()
        || !members
            .iter()
            .all(|member| matches!(member, PhpType::Object(name) if !name.is_empty()) || *member == PhpType::Void)
    {
        return value;
    }
    let classes: Vec<String> = members
        .iter()
        .filter_map(|member| match member {
            PhpType::Object(name) => Some(name.trim_start_matches('\\').to_string()),
            _ => None,
        })
        .collect();
    // A member the source's own class satisfies needs no probe; a single class is the nominal
    // guard's.
    if classes.len() < 2
        || classes.iter().any(|class| {
            class.eq_ignore_ascii_case(&source_class)
                || nominal_class_implements_or_extends(ctx, &source_class, class)
        })
    {
        return value;
    }
    let span = Some(arg.span);
    let answers: Vec<LoweredValue> = classes
        .iter()
        .map(|class| {
            let data = ctx.intern_class_name(class);
            ctx.emit_value(
                Op::InstanceOf,
                vec![value.value],
                Some(Immediate::Data(data)),
                PhpType::Bool,
                Op::InstanceOf.default_effects(),
                span,
            )
        })
        .collect();
    let initialized = ctx.initialized_slots_snapshot();
    let accepted = ctx.builder.create_named_block("object.union.accepted", Vec::new());
    let rejected = ctx.builder.create_named_block("object.union.rejected", Vec::new());
    for (position, answer) in answers.iter().enumerate() {
        let next = if position + 1 == answers.len() {
            rejected
        } else {
            ctx.builder.create_named_block("object.union.next", Vec::new())
        };
        ctx.builder.terminate(Terminator::CondBr {
            cond: answer.value,
            then_target: accepted,
            then_args: Vec::new(),
            else_target: next,
            else_args: Vec::new(),
        });
        if next != rejected {
            ctx.builder.position_at_end(next);
        }
    }
    ctx.builder.position_at_end(rejected);
    ctx.restore_initialized_slots(initialized.clone());
    if ctx.value_needs_release_after_use(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, span);
    }
    let param_name = sig.params.get(index).map(|(name, _)| name.as_str()).unwrap_or("arg");
    let callable = ctx.call_description.as_deref().unwrap_or("call");
    let source = ctx.source_path().unwrap_or("unknown");
    let message = format!(
        "{}(): Argument #{} (${}) must be of type {}, {} given, called in {} on line {}",
        callable,
        index + 1,
        param_name,
        crate::types::diagnostic_names::php_type_name(expected),
        source_class,
        source,
        arg.span.line
    );
    let exception = lower_expr(
        ctx,
        &Expr::new(
            ExprKind::NewObject {
                class_name: Name::unqualified("TypeError"),
                args: vec![Expr::new(ExprKind::StringLiteral(message), arg.span)],
            },
            arg.span,
        ),
    );
    ctx.builder.terminate(Terminator::Throw { value: exception.value });
    ctx.builder.position_at_end(accepted);
    ctx.restore_initialized_slots(initialized);
    value
}

/// Whether the call being lowered is a method of a class php implements internally, whose
/// argument errors carry no `, called in FILE on line N` clause.
fn call_description_is_internal_method(ctx: &LoweringContext<'_, '_>) -> bool {
    ctx.call_description
        .as_deref()
        .and_then(|callable| callable.split_once("::"))
        .is_some_and(|(class, _)| {
            ctx.classes
                .get(class.trim_start_matches('\\'))
                .is_some_and(|info| info.is_internal)
        })
}

/// Whether class or interface `source` is `target`, extends it or implements it.
pub(super) fn nominal_class_implements_or_extends(
    ctx: &LoweringContext<'_, '_>,
    source: &str,
    target: &str,
) -> bool {
    fn interface_extends(
        ctx: &LoweringContext<'_, '_>,
        interface: &str,
        target: &str,
        depth: usize,
    ) -> bool {
        if interface.eq_ignore_ascii_case(target) {
            return true;
        }
        depth < 32
            && ctx.interfaces.get(interface).is_some_and(|info| {
                info.parents.iter().any(|parent| interface_extends(ctx, parent, target, depth + 1))
            })
    }
    let mut current = Some(source.to_string());
    while let Some(name) = current {
        if name.eq_ignore_ascii_case(target) {
            return true;
        }
        let Some(info) = ctx.classes.get(name.as_str()) else {
            return interface_extends(ctx, &name, target, 0);
        };
        if info
            .interfaces
            .iter()
            .any(|interface| interface_extends(ctx, interface, target, 0))
        {
            return true;
        }
        current = info.parent.clone();
    }
    false
}

/// Checks the active tag of a boxed gradual value against a declared scalar/array union.
fn guard_gradual_union_param(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    expected: &PhpType,
    span: Option<crate::span::Span>,
) -> LoweredValue {
    let source_type = ctx.builder.value_php_type(value.value).clone();
    if !crate::types::param_binding::gradual_union_requires_runtime_param_guard(
        expected,
        &source_type,
    ) {
        return value;
    }
    let type_name = ctx.intern_string(&expected.to_string());
    ctx.emit_value(
        Op::RuntimeCall,
        vec![value.value],
        Some(Immediate::ParameterType {
            name: type_name,
            strict_types: ctx.php_strict_types,
        }),
        expected.clone(),
        effects_lookup::runtime_effects(),
        span,
    )
}

/// Returns whether a declared union is exactly PHP's nullable-string parameter type.
fn is_nullable_string_param(ty: &PhpType) -> bool {
    let PhpType::Union(members) = ty else {
        return false;
    };
    members.len() == 2
        && members.iter().any(|member| matches!(member, PhpType::Str))
        && members.iter().any(|member| matches!(member, PhpType::Void))
}

/// Applies weak scalar string conversion to a boxed nullable-string argument after a runtime
/// guard has rejected values PHP cannot bind to this declaration.
fn coerce_gradual_nullable_string_param(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    declared_param_ty: &PhpType,
    span: Option<crate::span::Span>,
) -> LoweredValue {
    let guarded = guard_gradual_union_param(ctx, value, declared_param_ty, span);
    apply_nullable_string_param_cast(ctx, guarded, declared_param_ty, span)
}

/// Returns whether a declared parameter type takes PHP null.
fn declared_type_admits_null(ty: &PhpType) -> bool {
    match ty {
        PhpType::Void | PhpType::Mixed => true,
        PhpType::Union(members) => members.iter().any(declared_type_admits_null),
        _ => false,
    }
}

/// Binds a compact `?int` to a parameter that takes it only through a non-nullable `string` arm.
///
/// php's weak mode converts the int and throws a `TypeError` for the null, so the split has one
/// continuing edge: the null edge releases an owning source and throws, the other converts.
fn coerce_nullable_int_to_string_param(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    null_message: &str,
    span: crate::span::Span,
) -> LoweredValue {
    let is_null = ctx.emit_value(
        Op::IsNull,
        vec![value.value],
        None,
        PhpType::Bool,
        Op::IsNull.default_effects(),
        Some(span),
    );
    let split_initialized = ctx.initialized_slots_snapshot();
    let null_block = ctx.builder.create_named_block("param_cast.null", Vec::new());
    let scalar_block = ctx.builder.create_named_block("param_cast.scalar", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_null.value,
        then_target: null_block,
        then_args: Vec::new(),
        else_target: scalar_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(null_block);
    ctx.restore_initialized_slots(split_initialized.clone());
    if ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
    let error = lower_expr(
        ctx,
        &Expr::new(
            ExprKind::NewObject {
                class_name: crate::names::Name::unqualified("TypeError"),
                args: vec![Expr::new(ExprKind::StringLiteral(null_message.to_string()), span)],
            },
            span,
        ),
    );
    ctx.builder.terminate(Terminator::Throw { value: error.value });

    ctx.builder.position_at_end(scalar_block);
    ctx.restore_initialized_slots(split_initialized);
    coerce_to_string_at_span(ctx, value, Some(span))
}

/// Checks a compact nullable integer before passing it to a declared `int` parameter.
fn guard_nullable_int_param(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    expected: &PhpType,
    span: Option<crate::span::Span>,
) -> LoweredValue {
    let source_type = ctx.builder.value_php_type(value.value).codegen_repr();
    if !crate::types::param_binding::nullable_int_requires_runtime_param_guard(
        expected,
        &source_type,
    ) {
        return value;
    }
    ctx.emit_value(
        Op::RuntimeCall,
        vec![value.value],
        None,
        PhpType::Int,
        effects_lookup::runtime_effects(),
        span,
    )
}

/// Throws php's `TypeError` for a null reaching a non-nullable `int` parameter, naming the
/// function, the parameter and the call site (`f(): Argument #1 ($x) must be of type int, null
/// given, called in /app.php on line 4`), and continues in the block where the value is not null.
fn guard_null_for_int_param(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    sig: &FunctionSig,
    index: usize,
    arg: &Expr,
) {
    let span = Some(arg.span);
    let is_null = ctx.emit_value(
        Op::IsNull,
        vec![value.value],
        None,
        PhpType::Bool,
        Op::IsNull.default_effects(),
        span,
    );
    let rejected = ctx.builder.create_named_block("int.arg.null", Vec::new());
    let accepted = ctx.builder.create_named_block("int.arg.accepted", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_null.value,
        then_target: rejected,
        then_args: Vec::new(),
        else_target: accepted,
        else_args: Vec::new(),
    });
    ctx.builder.position_at_end(rejected);
    let param_name = sig.params.get(index).map(|(name, _)| name.as_str()).unwrap_or("arg");
    let callable = ctx.call_description.as_deref().unwrap_or("call");
    let source = ctx.source_path().unwrap_or("unknown");
    let message = format!(
        "{}(): Argument #{} (${}) must be of type int, null given, called in {} on line {}",
        callable,
        index + 1,
        param_name,
        source,
        arg.span.line
    );
    let exception = lower_expr(
        ctx,
        &Expr::new(
            ExprKind::NewObject {
                class_name: Name::unqualified("TypeError"),
                args: vec![Expr::new(ExprKind::StringLiteral(message), arg.span)],
            },
            arg.span,
        ),
    );
    ctx.builder.terminate(Terminator::Throw { value: exception.value });
    ctx.builder.position_at_end(accepted);
}

/// Whether `value` is, through ownership-only forwarding, a `Mixed` cell this lowering boxed
/// around a statically typed value.
///
/// The positional argument step already boxes a typed source for a union parameter (the checker
/// proved its type, and php's weak mode converts a float or bool bound to `string|int`), so the
/// runtime tag guard of the reordered pass must only see values whose tag is not known statically.
fn value_is_box_of_static_value(
    ctx: &LoweringContext<'_, '_>,
    mut value: crate::ir::ValueId,
) -> bool {
    while let Some(producer) = ctx.builder.value_defining_instruction(value) {
        match producer.op {
            Op::MixedBox => return true,
            Op::Acquire | Op::Borrow | Op::Move => match producer.operands.first() {
                Some(&source) => value = source,
                None => return false,
            },
            _ => return false,
        }
    }
    false
}

/// Whether `value` is a Mixed cell that boxes a literal `null` (a defaulted `= null` argument).
fn value_is_boxed_literal_null(ctx: &LoweringContext<'_, '_>, value: crate::ir::ValueId) -> bool {
    let Some(boxed) = ctx.builder.value_defining_instruction(value) else {
        return false;
    };
    boxed.op == Op::MixedBox
        && boxed.operands.first().is_some_and(|&inner| {
            ctx.builder
                .value_defining_instruction(inner)
                .is_some_and(|inner| inner.op == Op::ConstNull)
        })
}

/// Inserts the runtime class guard required by a declared object parameter.
///
/// The checker only admits this path for by-value source declarations. Nullable target storage
/// is boxed again after the object check so the callee still receives its declared union ABI.
fn guard_nominal_object_param(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    expected: &PhpType,
    span: Option<crate::span::Span>,
) -> LoweredValue {
    let source_php_type = ctx.builder.value_php_type(value.value).clone();
    let source_type = source_php_type.codegen_repr();
    // A literal `null` (a defaulted `?Throwable $previous = null`) is admitted by a nullable
    // object parameter by construction: no class check, and no box to carry one.
    if matches!(expected, PhpType::Union(members) if members.contains(&PhpType::Void))
        && (source_php_type == PhpType::Void || value_is_boxed_literal_null(ctx, value.value))
    {
        return value;
    }
    // A union naming several classes still has ONE that a value of this argument's own class
    // could be; see `nominal_object_boundary_target_for_source`. That target is resolved BEFORE
    // the `needs_nominal_guard` test, because the two `*_requires_runtime_nominal_guard`
    // predicates both ask `nominal_object_boundary_target`, which turns such a union away — the
    // guard would have been skipped entirely and a class PHP rejects would have crossed silently.
    let narrowed_target = crate::types::param_binding::nominal_object_boundary_target_for_source(
        expected,
        &source_php_type,
        &|candidate, source| nominal_boundary_classes_are_related(ctx, candidate, source),
    );
    let gradual_source =
        crate::types::param_binding::gradual_object_requires_runtime_nominal_guard(
            expected,
            &source_type,
        )
        || (narrowed_target.is_some()
            && matches!(source_type, PhpType::Mixed | PhpType::Union(_)));
    let needs_nominal_guard = gradual_source
        || narrowed_target.is_some()
        || crate::types::param_binding::object_requires_runtime_nominal_guard(
            expected,
            &source_type,
        );
    if !needs_nominal_guard
        || param_accepts_object_without_string_coercion(ctx, expected, &source_type)
        || (!gradual_source && instantiation_widening_already_admitted(ctx, expected, &source_type))
    {
        return value;
    }
    let Some(target_name) = crate::types::param_binding::nominal_object_boundary_target(expected)
        .or(narrowed_target)
    else {
        return value;
    };
    let target_data = ctx.intern_class_name(&target_name);
    let guarded_type = if gradual_source {
        expected.clone()
    } else {
        PhpType::Object(target_name.clone())
    };
    let guarded = ctx.emit_value(
        Op::RuntimeCall,
        vec![value.value],
        Some(Immediate::NominalObject {
            target: target_data,
            boundary: crate::ir::NominalObjectBoundary::Parameter,
        }),
        guarded_type,
        effects_lookup::runtime_effects(),
        span,
    );
    // A guard that narrows an owned Mixed argument (a property read, a call result) to a concrete
    // object hands back an independently retained payload, so the argument's own box has no
    // consumer left and is retired here; a guard that FORWARDS the cell keeps it live.
    if gradual_source
        && !matches!(
            ctx.builder.value_php_type(guarded.value).codegen_repr(),
            PhpType::Mixed | PhpType::Union(_)
        )
        && ctx.value_is_owning_temporary(value)
    {
        crate::ir_lower::ownership::release_if_owned(ctx, value, span);
    }
    if !gradual_source && expected.codegen_repr() == PhpType::Mixed {
        ctx.box_value_as_mixed(guarded, expected.clone(), span)
    } else {
        guarded
    }
}

/// Returns whether a statically typed instantiation (`Box<Dog>`) reaches a parameter naming
/// another instantiation of the same template (`Box<Animal>`) because the checker admitted the
/// widening under a `+T` / `-T` variance marker.
///
/// The checker already refused every pairing its variance rule rejects, and monomorphised
/// instantiations are distinct, unrelated classes at runtime, so the nominal guard would throw
/// on exactly the programs the checker accepted. The source's ancestors are walked too, so a
/// `class DogBox extends Box<Dog>` argument is covered.
fn instantiation_widening_already_admitted(
    ctx: &LoweringContext<'_, '_>,
    expected: &PhpType,
    actual: &PhpType,
) -> bool {
    let Some(target) = crate::types::param_binding::nominal_object_boundary_target(expected) else {
        return false;
    };
    let target = target.trim_start_matches('\\');
    let Some((target_base, _)) = target.split_once('<') else {
        return false;
    };
    let PhpType::Object(actual_name) = actual else {
        return false;
    };
    let mut current = Some(actual_name.trim_start_matches('\\').to_string());
    while let Some(name) = current {
        if let Some((base, _)) = name.split_once('<') {
            if base.eq_ignore_ascii_case(target_base) {
                return true;
            }
        }
        current = ctx
            .classes
            .get(name.as_str())
            .and_then(|class_info| class_info.parent.clone());
    }
    false
}

/// Returns whether an object argument already satisfies a declared parameter without selecting
/// a weakly coercive `string` arm.
///
/// PHP resolves a union by identity before coercion: an object implementing `IteratorAggregate`
/// remains an object for `string|iterable`, even when it also implements `Stringable`. This
/// mirrors the checker's object/iterable/callable acceptance so lowering only consumes the
/// checker's weak-string proof when no non-string member already accepts the object.
pub(in crate::ir_lower) fn param_accepts_object_without_string_coercion(
    ctx: &LoweringContext<'_, '_>,
    expected: &PhpType,
    actual: &PhpType,
) -> bool {
    let PhpType::Object(actual_name) = actual else {
        return false;
    };
    match expected {
        PhpType::Mixed => true,
        // An object that is not `Stringable` can never take the union's `string` arm, so php binds
        // it to a class-like member -- one its static type need not name: a value typed by one
        // interface is routinely an instance of another. Symfony narrows `$user` to
        // `UserInterface` and passes it to `getPasswordHasher(string|PasswordAuthenticatedUser
        // Interface|...)`; the `string` arm cast the entity, and every demo login died on "Object
        // of class App\Entity\User could not be converted to string".
        PhpType::Union(members)
            if members.iter().any(|member| matches!(member, PhpType::Object(_)))
                && !object_name_satisfies_interface_for_ir(ctx, actual_name, "Stringable") =>
        {
            true
        }
        PhpType::Union(members) => members
            .iter()
            .any(|member| param_accepts_object_without_string_coercion(ctx, member, actual)),
        PhpType::Object(expected_name) => {
            expected_name.is_empty()
                || class_extends_class(ctx, actual_name, expected_name)
                || object_name_satisfies_interface_for_ir(ctx, actual_name, expected_name)
        }
        PhpType::Iterable => {
            object_name_satisfies_interface_for_ir(ctx, actual_name, "Traversable")
                || object_name_satisfies_interface_for_ir(ctx, actual_name, "Iterator")
                || object_name_satisfies_interface_for_ir(ctx, actual_name, "IteratorAggregate")
        }
        PhpType::Callable => actual.is_closure_object(),
        _ => false,
    }
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
    unbox_callable_param_storage_with(ctx, value, span, None)
}

/// [`unbox_callable_param_storage`] for a parameter DECLARED `callable`: `diagnostic` is php's
/// whole `TypeError` text around the runtime type word (`prefix\0suffix`), so null, a scalar or
/// an unresolvable name or pair throws it instead of passing through or faulting.
pub(super) fn unbox_callable_param_storage_with(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<crate::span::Span>,
    diagnostic: Option<String>,
) -> LoweredValue {
    // NORMALIZED, not merely unboxed: a boxed value may hold any callable spelling (`"strlen"`,
    // `[$o, 'm']`, a closure) or no callable at all. A bare unbox read `null` or `42` as a
    // descriptor pointer and the callee's first call through it segfaulted;
    // `NormalizeCallable` resolves the spellings and refuses the rest.
    let immediate = diagnostic.map(|text| Immediate::Data(ctx.intern_string(&text)));
    let result = ctx.emit_owned_value(
        Op::NormalizeCallable,
        vec![value.value],
        immediate,
        PhpType::Callable,
        Op::NormalizeCallable.default_effects(),
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
    unbox_iterable_param_storage_with(ctx, value, span, None)
}

/// [`unbox_iterable_param_storage`] carrying php's `TypeError` text for a wrong runtime type.
///
/// `diagnostic` is the `prefix\0suffix` the codegen guard wraps around the runtime type word;
/// without it the guard raises its generic `Value must be of type iterable` text.
fn unbox_iterable_param_storage_with(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<crate::span::Span>,
    diagnostic: Option<String>,
) -> LoweredValue {
    let immediate = diagnostic.map(|text| Immediate::Data(ctx.intern_string(&text)));
    let result = ctx.emit_owned_value(
        Op::MixedUnbox, vec![value.value], immediate, PhpType::Iterable,
        Op::mixed_unbox_effects(&PhpType::Iterable), span,
    );
    release_coerced_source_if_owned(ctx, value, span);
    result
}

/// Builds php's `TypeError` text for a non-iterable value reaching a declared `iterable`
/// parameter, as `prefix\0suffix` around the runtime type word: php 8.2+ spells the type
/// `Traversable|array` and names the function, the parameter and the call site
/// (`total(): Argument #1 ($values) must be of type Traversable|array, int given, called in
/// /app.php on line 9`).
fn iterable_param_type_error(
    ctx: &LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    arg: &Expr,
) -> String {
    let param_name = sig
        .params
        .get(index)
        .map(|(name, _)| name.as_str())
        .unwrap_or("arg");
    let callable = ctx.call_description.as_deref().unwrap_or("call");
    let source = ctx.source_path().unwrap_or("unknown");
    format!(
        "{}(): Argument #{} (${}) must be of type Traversable|array, \0 given, called in {} on line {}",
        callable,
        index + 1,
        param_name,
        source,
        arg.span.line
    )
}

/// Builds php's `TypeError` text for a value no `callable` parameter accepts, as
/// `prefix\0suffix` around the runtime type word (`apply(): Argument #1 ($f) must be of type
/// callable, null given, called in /app.php on line 9`).
fn callable_param_type_error(
    ctx: &LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    arg: &Expr,
) -> String {
    let param_name = sig
        .params
        .get(index)
        .map(|(name, _)| name.as_str())
        .unwrap_or("arg");
    let callable = ctx.call_description.as_deref().unwrap_or("call");
    let source = ctx.source_path().unwrap_or("unknown");
    format!(
        "{}(): Argument #{} (${}) must be of type callable, \0 given, called in {} on line {}",
        callable,
        index + 1,
        param_name,
        source,
        arg.span.line
    )
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
    // php names no call site for a method it implements internally
    // (`ReflectionClass::isInstance(5)`).
    let suffix = if call_description_is_internal_method(ctx) {
        " given".to_string()
    } else {
        format!(" given, called in {} on line {}", source, arg.span.line)
    };
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
/// binding is expressed as a `CastType`: scalar inputs use the same total `(string)` / `(bool)`
/// conversions, while objects reach `(string)` only after checker proof of `Stringable`.
/// When PHP selects a scalar member of a union, the converted value is boxed back into the
/// union's `Mixed` ABI storage before the callee receives it.
pub(crate) fn apply_scalar_param_cast(
    ctx: &mut LoweringContext<'_, '_>,
    cast: CastType,
    value: LoweredValue,
    declared_param_ty: &PhpType,
    span: Option<crate::span::Span>,
) -> LoweredValue {
    if cast == CastType::String
        && value_is_nullable(ctx, value.value)
        && php_type_accepts_null(declared_param_ty)
    {
        return apply_nullable_string_param_cast(ctx, value, declared_param_ty, span);
    }
    let converted = match cast {
        CastType::String => coerce_to_string_at_span(ctx, value, span),
        CastType::Bool => lower_truthy_bool(ctx, value, span),
        // `param_binding::scalar_param_cast` only ever reports the two total scalar casts.
        CastType::Int | CastType::Float | CastType::Array | CastType::Object => value,
    };
    if declared_param_ty.codegen_repr() == PhpType::Mixed
        && ctx
            .builder
            .value_php_type(converted.value)
            .codegen_repr()
            != PhpType::Mixed
    {
        ctx.box_value_as_mixed(converted, declared_param_ty.clone(), span)
    } else {
        converted
    }
}

/// Preserves PHP null while coercing every non-null scalar member to a string union arm.
///
/// The source has already been evaluated once. Each control-flow edge consumes that same value:
/// the null edge stores a boxed null and releases an owning source cell, while the non-null edge
/// uses the ordinary string coercer and its existing ownership cleanup.
fn apply_nullable_string_param_cast(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    declared_param_ty: &PhpType,
    span: Option<crate::span::Span>,
) -> LoweredValue {
    let source_span = span.unwrap_or_else(crate::span::Span::dummy);
    let is_null = ctx.emit_value(
        Op::IsNull,
        vec![value.value],
        None,
        PhpType::Bool,
        Op::IsNull.default_effects(),
        span,
    );
    let temp_name = ctx.declare_owned_hidden_temp(declared_param_ty.clone());
    let split_initialized = ctx.initialized_slots_snapshot();
    let null_block = ctx
        .builder
        .create_named_block("param_cast.null", Vec::new());
    let scalar_block = ctx
        .builder
        .create_named_block("param_cast.scalar", Vec::new());
    let merge_block = ctx
        .builder
        .create_named_block("param_cast.merge", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: is_null.value,
        then_target: null_block,
        then_args: Vec::new(),
        else_target: scalar_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(null_block);
    ctx.restore_initialized_slots(split_initialized.clone());
    let null_value = ctx.emit_value(
        Op::ConstNull,
        Vec::new(),
        None,
        PhpType::Void,
        Op::ConstNull.default_effects(),
        span,
    );
    store_value_into_temp(
        ctx,
        &temp_name,
        declared_param_ty.clone(),
        null_value,
        source_span,
    );
    if ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, span);
    }
    let null_initialized = ctx.initialized_slots_snapshot();
    branch_to(ctx, merge_block);

    ctx.builder.position_at_end(scalar_block);
    ctx.restore_initialized_slots(split_initialized.clone());
    let converted = coerce_to_string_at_span(ctx, value, span);
    store_value_into_temp(
        ctx,
        &temp_name,
        declared_param_ty.clone(),
        converted,
        source_span,
    );
    let scalar_initialized = ctx.initialized_slots_snapshot();
    branch_to(ctx, merge_block);

    ctx.builder.position_at_end(merge_block);
    ctx.restore_initialized_slots(merge_initialized_slots_for_expr(
        &split_initialized,
        null_initialized,
        true,
        scalar_initialized,
        true,
    ));
    take_owned_temp(ctx, &temp_name, source_span)
}

/// Returns whether a declared parameter union admits PHP null without coercion.
fn php_type_accepts_null(ty: &PhpType) -> bool {
    match ty {
        PhpType::Mixed | PhpType::Void => true,
        PhpType::Union(members) => members.iter().any(php_type_accepts_null),
        _ => false,
    }
}

/// Normalizes reordered call operands to their declared parameter storage.
///
/// Named and spread arguments are evaluated in source order and then reordered, so their
/// int-to-float, boxed-scalar, and checked heap narrowing happen here in parameter order.
/// By-reference parameters and the variadic tail remain untouched. Allocating conversions become
/// owned EIR values so normal alias-aware call cleanup can transfer or release them safely.
pub(super) fn coerce_operands_to_params(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    mut operands: Vec<crate::ir::ValueId>,
) -> Vec<crate::ir::ValueId> {
    let regular_param_count = crate::types::call_args::regular_param_count(sig);
    let limit = operands.len().min(regular_param_count);
    let needs_coercion = (0..limit)
        .map(|index| operand_needs_param_coercion(ctx, sig, &operands, index))
        .collect::<Vec<_>>();
    if needs_coercion.iter().any(|needs_coercion| *needs_coercion) {
        // Source evaluation has completed, but any parameter conversion below can throw.
        // Publish the one last source value that did not need protection during source
        // evaluation, plus synthesized owned operands, before the first conversion runs.
        for index in 0..limit {
            if sig.ref_params.get(index).copied().unwrap_or(false) {
                continue;
            }
            let lowered = lowered_value_from_id(ctx, operands[index]);
            operands[index] =
                root_evaluated_call_argument(ctx, lowered, Span::dummy()).value;
        }
    }
    for index in 0..limit {
        if sig.ref_params.get(index).copied().unwrap_or(false) {
            continue;
        }
        let Some((_, declared_param_ty)) = sig.params.get(index) else {
            continue;
        };
        let value = operands[index];
        let value = if sig.declared_params.get(index).copied().unwrap_or(false) {
            guard_nominal_object_param(ctx, LoweredValue {
                value,
                ir_type: ctx.builder.value_type(value),
            }, declared_param_ty, None)
        } else {
            LoweredValue {
                value,
                ir_type: ctx.builder.value_type(value),
            }
        };
        let value = if sig.declared_params.get(index).copied().unwrap_or(false) {
            guard_nullable_int_param(ctx, value, declared_param_ty, None)
        } else {
            value
        };
        let value = if sig.declared_params.get(index).copied().unwrap_or(false)
            && !value_is_box_of_static_value(ctx, value.value)
        {
            guard_gradual_union_param(ctx, value, declared_param_ty, None)
        } else {
            value
        };
        if sig.declared_params.get(index).copied().unwrap_or(false)
            && !ctx.php_strict_types
            && is_nullable_string_param(declared_param_ty)
            && ctx.builder.value_php_type(value.value) != *declared_param_ty
            && matches!(
                ctx.builder.value_php_type(value.value).codegen_repr(),
                PhpType::Mixed | PhpType::Union(_)
            )
        {
            operands[index] = coerce_gradual_nullable_string_param(
                ctx,
                value,
                declared_param_ty,
                None,
            )
            .value;
            continue;
        }
        operands[index] = value.value;
        let value = value.value;
        let operand_php_ty = ctx.builder.value_php_type(value).clone();
        let operand_ty = operand_php_ty.codegen_repr();
        let param_ty = declared_param_ty.codegen_repr();
        // Same ordering rule as the positional path: a `Stringable` object bound to the weak
        // `string` arm converts first (the final else-if arm), instead of being boxed as itself.
        let weak_string_object = matches!(operand_ty, PhpType::Object(_))
            && !param_accepts_object_without_string_coercion(ctx, declared_param_ty, &operand_ty)
            && crate::types::param_binding::scalar_param_cast(declared_param_ty, &operand_php_ty)
                .is_some();
        if sig.declared_params.get(index).copied().unwrap_or(false)
            && param_ty == PhpType::Mixed
            && operand_ty != PhpType::Mixed
            && !weak_string_object
        {
            let lowered = LoweredValue {
                value,
                ir_type: ctx.builder.value_type(value),
            };
            operands[index] = ctx
                .box_value_as_mixed(lowered, PhpType::Mixed, None)
                .value;
        } else if matches!(param_ty, PhpType::Array(_) | PhpType::AssocArray { .. })
            && operand_ty == PhpType::Iterable
        {
            // `iterable` is `array|Traversable` and carries a representation the array helpers
            // cannot walk. Materialising it here is what makes the binding SOUND: re-typing the
            // value instead -- which is what a narrowing rule would do -- hands the callee
            // something it reads as fixed-size slots, and it segfaults. Keys are preserved, which
            // is both `iterator_to_array()`'s default and what php does when the value is already
            // an array.
            operands[index] = ctx
                .emit_value(
                    Op::RuntimeCall,
                    vec![value],
                    Some(Immediate::RuntimeCall(
                        crate::ir::RuntimeCallTarget::Function(
                            crate::ir::RuntimeFnId::IteratorToArray,
                        ),
                    )),
                    PhpType::AssocArray {
                        key: Box::new(PhpType::Mixed),
                        value: Box::new(PhpType::Mixed),
                    },
                    crate::ir::RuntimeFnId::IteratorToArray.effects(),
                    None,
                )
                .value;
        } else if param_ty == PhpType::Float
            && matches!(operand_ty, PhpType::Int | PhpType::Bool)
        {
            let lowered = LoweredValue {
                value,
                ir_type: IrType::I64,
            };
            let coerced = coerce_to_float_at_span(ctx, lowered, None);
            operands[index] =
                root_evaluated_call_argument(ctx, coerced, Span::dummy()).value;
        } else if param_ty == PhpType::Str
            && matches!(
                operand_ty,
                PhpType::Mixed | PhpType::TaggedScalar | PhpType::Union(_)
            )
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
            let coerced = unbox_callable_param_storage(ctx, lowered, None);
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
        } else if (sig.declared_params.get(index).copied().unwrap_or(false)
            || matches!(operand_ty, PhpType::Object(_)))
            && !param_accepts_object_without_string_coercion(
                ctx,
                declared_param_ty,
                &operand_ty,
            )
        {
            // Same declared-parameter scalar binding the positional path applies, run here in
            // parameter order because named and spread arguments are lowered in source order
            // and only reordered afterwards.
            if let Some(cast) =
                crate::types::param_binding::scalar_param_cast(
                    declared_param_ty,
                    &operand_php_ty,
                )
            {
                let lowered = LoweredValue {
                    value,
                    ir_type: ctx.builder.value_type(value),
                };
                let coerced =
                    apply_scalar_param_cast(ctx, cast, lowered, declared_param_ty, None);
                operands[index] =
                    root_evaluated_call_argument(ctx, coerced, Span::dummy()).value;
            }
        }
        let lowered = LoweredValue {
            value: operands[index],
            ir_type: ctx.builder.value_type(operands[index]),
        };
        let source_ty = ctx.builder.value_php_type(lowered.value).codegen_repr();
        // An UNDECLARED parameter whose inferred type is a container is still a boxed `mixed`
        // cell in the callee's EIR contract (`eir_signature_with_php_param_contracts`), so a
        // boxed argument is already in the shape the callee reads. Narrowing it to the inferred
        // container type would demand an array of something php lets the function receive as
        // anything: the recursive `dumpv($item)` of a walker over `preg_match_all()`'s matches
        // threw "Value must be of type array, string given" for the first string element.
        if !sig.declared_params.get(index).copied().unwrap_or(false)
            && matches!(param_ty, PhpType::Array(_) | PhpType::AssocArray { .. })
            && matches!(source_ty, PhpType::Mixed | PhpType::Union(_))
        {
            continue;
        }
        // A `false` parameter keeps a boxed argument boxed so the backend's false-parameter
        // guard throws php's `TypeError`; the bool cast would fold `0`, `""` and `[]` to false.
        if *declared_param_ty == PhpType::False && matches!(source_ty, PhpType::Mixed) {
            continue;
        }
        let lowered = crate::ir_lower::expr::coerce_container_to_mixed_payload(
            ctx,
            lowered,
            &source_ty,
            &param_ty,
            None,
        );
        operands[index] = crate::ir_lower::gradual_coercions::coerce_gradual_value_to_boundary(
            ctx,
            lowered,
            &param_ty,
            None,
        )
        .value;
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
    let Some((_, declared_param_ty)) = sig.params.get(index) else {
        return false;
    };
    let declared = sig.declared_params.get(index).copied().unwrap_or(false);
    let operand_ty = ctx.builder.value_php_type(operands[index]).codegen_repr();
    let param_ty = declared_param_ty.codegen_repr();
    // The declared-parameter guards of the loop above can throw too: the nominal class check of
    // an object parameter fed a gradual or other-class operand, the nullable-int check, and the
    // nullable-string coercion of a gradual operand.
    if declared
        && ((crate::types::param_binding::nominal_object_boundary_target(declared_param_ty)
            .is_some()
            && operand_ty != param_ty)
            || crate::types::param_binding::nullable_int_requires_runtime_param_guard(
                declared_param_ty,
                &operand_ty,
            )
            || (is_nullable_string_param(declared_param_ty)
                && matches!(operand_ty, PhpType::Mixed | PhpType::Union(_))))
    {
        return true;
    }
    (param_ty == PhpType::Float && matches!(operand_ty, PhpType::Int | PhpType::Bool))
        || (matches!(param_ty, PhpType::Array(_) | PhpType::AssocArray { .. })
            && operand_ty == PhpType::Iterable)
        || (param_ty == PhpType::Str
            && matches!(operand_ty, PhpType::Mixed | PhpType::Union(_)))
        || (param_ty == PhpType::Callable
            && matches!(operand_ty, PhpType::Mixed | PhpType::Union(_)))
        || (param_ty == PhpType::Iterable
            && matches!(operand_ty, PhpType::Mixed | PhpType::Union(_)))
        || (matches!(param_ty, PhpType::Array(_) | PhpType::AssocArray { .. })
            && matches!(operand_ty, PhpType::Mixed | PhpType::Union(_)))
        || packed_argument_param_storage(ctx, sig, index, operands[index]).is_some()
        || (declared
            && crate::types::param_binding::scalar_param_cast(&param_ty, &operand_ty).is_some())
}

/// Passes a program-global variable to a by-reference `mixed` parameter as its shared cell.
///
/// A top-level variable that some function declares `global` lives in program-global storage,
/// not in a frame slot, so the ordinary by-reference path had no address to hand over: the
/// callee received a copy and its writes were lost. `sort($a)` lowered through the
/// `__elephc_sort_mixed(&$array)` prelude left `$a` unsorted with no diagnostic. A hidden local
/// bound to the global's shared reference cell (as `global $a` binds one in a function) gives
/// the call that address. Superglobals keep their own path.
fn lower_global_storage_ref_arg(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    arg: &Expr,
) -> Option<crate::ir::ValueId> {
    if !sig.ref_params.get(index).copied().unwrap_or(false) {
        return None;
    }
    let (_, param_ty) = sig.params.get(index)?;
    if param_ty.codegen_repr() != PhpType::Mixed {
        return None;
    }
    let ExprKind::Variable(name) = &arg.kind else {
        return None;
    };
    if crate::superglobals::is_superglobal(name) {
        if ctx.web {
            // A `--web` superglobal's cell holds the bare hash, not the box a `mixed &$x`
            // parameter reads: `lower_by_ref_call_through_temps` passes it through a temporary.
            return None;
        }
        // A superglobal has no frame slot either: the argument was a copy of its storage, so
        // `f($_GET)` and `rsort($_GET)` (through `__elephc_rsort_mixed(mixed &$array)`) wrote to
        // the copy and `$_GET` kept its old value. Bind a hidden local to the superglobal's
        // shared cell, as `$x = &$_GET` does.
        let temp = crate::names::generated_local_name(&format!("superglobal_ref_arg_{name}"));
        ctx.bind_superglobal_ref_alias(&temp, name, Some(arg.span));
        return Some(ctx.load_local(&temp, Some(arg.span)).value);
    }
    if !ctx.local_uses_global_storage(name) {
        return None;
    }
    let temp = crate::names::generated_local_name(&format!("global_ref_arg_{name}"));
    ctx.bind_superglobal_ref_alias(&temp, name, Some(arg.span));
    Some(ctx.load_local(&temp, Some(arg.span)).value)
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
    if !sig.declared_params.get(index).copied().unwrap_or(false)
        || !by_ref_array_arg_needs_mixed_storage(ctx, name, param_ty)
    {
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

/// Returns the class whose reference-promoted slot `$object->property` names, deciding from the
/// static receiver alone (no IR is emitted before the decision).
pub(super) fn reference_property_place_class(
    ctx: &LoweringContext<'_, '_>,
    place: &Expr,
) -> Option<String> {
    let ExprKind::PropertyAccess { object, property } = &place.kind else {
        return None;
    };
    let class = match &object.kind {
        ExprKind::This => ctx.current_class.clone()?,
        ExprKind::Variable(name) if ctx.local_slots.contains_key(name) => {
            match ctx.local_type(name).codegen_repr() {
                PhpType::Object(class) => class,
                _ => return None,
            }
        }
        _ => return None,
    };
    let class = class.trim_start_matches('\\').to_string();
    ctx.classes
        .get(class.as_str())
        .is_some_and(|info| info.reference_properties.contains(property))
        .then_some(class)
}

/// Whether a reference-promoted property can hand a by-reference parameter its own cell.
///
/// A cell holding a boxed Mixed cannot serve a parameter that writes a raw value: `bool &$x` stored
/// a bare 1 where `Symfony\\Component\\VarDumper\\Caster\\LinkStub::$inVendor` kept a box, and the next
/// read jumped through it. That parameter takes the read and write-back path, which converts.
pub(super) fn property_cell_serves_param(
    ctx: &LoweringContext<'_, '_>,
    place: &Expr,
    param_ty: &PhpType,
) -> bool {
    if reference_property_place_class(ctx, place).is_none() {
        return false;
    }
    let boxed = |ty: PhpType| matches!(ty, PhpType::Mixed | PhpType::Union(_));
    boxed(param_ty.codegen_repr())
        || !super::ref_place_args::static_place_type(ctx, place)
            .is_some_and(|ty| boxed(ty.codegen_repr()))
}

/// Lowers `$object->property` as a by-reference argument bound to the property's own cell.
///
/// php binds the parameter to the property's slot, so `new Svc($this->p)` with
/// `__construct(array &$p) { $this->p = &$p; }` leaves both objects sharing one array, and a
/// write through the parameter is a write to the property. The checker promotes such a property
/// to a reference property (`promote_by_ref_property_args`); the argument is then lowered as
/// `$alias = &$object->property; f($alias)`, the alias form whose cell handling the reference
/// suite already pins. Copying the value into a hidden local instead gave the callee a
/// reference to that temporary -- Symfony's `ServicesConfigurator` read `null` back through it.
fn lower_by_ref_property_arg_with_signature(
    ctx: &mut LoweringContext<'_, '_>,
    sig: &FunctionSig,
    index: usize,
    arg: &Expr,
) -> Option<crate::ir::ValueId> {
    if !sig.ref_params.get(index).copied().unwrap_or(false) {
        return None;
    }
    if !property_cell_serves_param(ctx, arg, &sig.params.get(index)?.1) {
        return None;
    }
    // One alias per source position: two property arguments of one call must not share a slot.
    let alias = crate::names::generated_local_name(&format!(
        "by_ref_property_arg_{}_{}_{index}",
        arg.span.line, arg.span.col
    ));
    crate::ir_lower::expr::property_access::lower_ref_assign_property(ctx, &alias, arg, arg.span);
    let variable = Expr::new(ExprKind::Variable(alias), arg.span);
    Some(lower_arg_with_signature(ctx, sig, index, &variable))
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
        // The conversion MOVES the slot's reference: the retyping store below releases nothing,
        // so a plain `ArrayToHash` over a slot load left the caller's list leaked.
        let hash = ctx.emit_local_array_to_hash_move(name, local, storage_ty.clone(), arg.span);
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
            // The callee stores a boxed value through this entry's cell, so the hash's value type
            // becomes `mixed` first, as the list branch below widens its slots. Left typed, a later
            // `$map["k"]` read the box `append(array &$v)` published as a raw `array<int>`, and
            // `implode()` printed its slot pointers. Binding the concrete entry also left the
            // local typed `int` while the callee stored an object in it.
            let parent = ctx.load_local(array_name, Some(array.span));
            let owned =
                crate::ir_lower::ownership::acquire_if_refcounted(ctx, parent, Some(arg.span));
            let widened_ty = PhpType::AssocArray {
                key: key.clone(),
                value: Box::new(PhpType::Mixed),
            };
            let converted = ctx.emit_value(
                Op::HashToMixed,
                vec![owned.value],
                None,
                widened_ty.clone(),
                Op::HashToMixed.default_effects(),
                Some(arg.span),
            );
            ctx.store_call_argument_local(array_name, converted, widened_ty, Some(arg.span));
        }
        return Some(managed_element_ref_cell_arg(
            ctx, array_name, array.span, element_index, param_ty, arg.span,
        ));
    }
    if matches!(local_ty, PhpType::Mixed | PhpType::Union(_)) {
        // A declared PHP array reads back as one boxed value, so neither concrete branch above
        // matches even though the element is a managed cell just the same. Leasing it here is
        // what puts the cell in the call's unwind ledger: without that, a later argument whose
        // evaluation throws left the cell unreleased.
        return Some(managed_element_ref_cell_arg(
            ctx, array_name, array.span, element_index, param_ty, arg.span,
        ));
    }
    let PhpType::Array(elem_ty) = local_ty else {
        return None;
    };
    if elem_ty.codegen_repr() == PhpType::Mixed {
        return Some(managed_element_ref_cell_arg(
            ctx, array_name, array.span, element_index, param_ty, arg.span,
        ));
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
        return Some(managed_element_ref_cell_arg(
            ctx, array_name, array.span, element_index, param_ty, arg.span,
        ));
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

/// Binds a local container's element as a leased managed reference cell for a by-reference call.
///
/// The key is evaluated before the container is fetched, as PHP orders a write: a key expression
/// that grows the container would otherwise leave a stale pointer to bind into. The cell op
/// copies a string key into the container, so an owned key temporary (`$m[next_key()]`) is
/// released here -- it leaked one string per call.
fn managed_element_ref_cell_arg(
    ctx: &mut LoweringContext<'_, '_>,
    array_name: &str,
    array_span: Span,
    element_index: &Expr,
    param_ty: &PhpType,
    arg_span: Span,
) -> crate::ir::ValueId {
    let element_index = lower_expr(ctx, element_index);
    let container = ctx.load_local(array_name, Some(array_span));
    let cell = ctx.emit_value(
        Op::LoadArrayElemRefCell,
        vec![container.value, element_index.value],
        None,
        PhpType::Pointer(None),
        Op::LoadArrayElemRefCell.default_effects(),
        Some(arg_span),
    );
    crate::ir_lower::stmt::release_persisted_string_operand(ctx, element_index, arg_span);
    let leased = lease_managed_call_argument_ref_cell(ctx, cell, arg_span);
    if matches!(param_ty.codegen_repr(), PhpType::Mixed | PhpType::TaggedScalar) {
        return leased.value;
    }
    // The cell's payload is a boxed Mixed, and a typed parameter writes a raw value: `int &$p`
    // stored 7 over the box pointer, and the next read of the element jumped through it.
    ctx.emit_value(
        Op::RefCellMixedPayload,
        vec![leased.value],
        None,
        PhpType::Pointer(None),
        Op::RefCellMixedPayload.default_effects(),
        Some(arg_span),
    )
    .value
}

/// Returns true when a local array's concrete slots differ from a generic array parameter.
///
/// Callers must additionally establish that the parameter is source-declared. Internal
/// operations have typed mutation strategies and must retain the concrete representation their
/// backend selected rather than inheriting this declared-parameter normalization.
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

/// Lowers a USER callable's arguments, packing spreads that land in its variadic tail into the
/// tail array (`f($fixed, $extra, ...$rest)`).
///
/// Builtins keep [`lower_args_with_signature`]: their registry runtime signatures take the source
/// arguments one operand each and expand a spread themselves (`array_merge_recursive([], ...$r)`).
pub(super) fn lower_user_args_with_signature(
    ctx: &mut LoweringContext<'_, '_>,
    sig: Option<&FunctionSig>,
    args: &[Expr],
) -> Vec<crate::ir::ValueId> {
    lower_args_with_signature_options_for_capture(ctx, sig, args, false, false, None, true)
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
        ctx, sig, args, trim_trailing_defaults, capture_values, None, false,
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
    pack_tail_spreads: bool,
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
        let operands = lower_named_args_with_signature_options(
            ctx, sig, args, trim_trailing_defaults, capture_values, capture_output_index,
        );
        return coerce_operands_to_params(ctx, sig, operands);
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
        return coerce_operands_to_params(ctx, sig, operands);
    }
    let regular_param_count = crate::types::call_args::regular_param_count(sig);
    // Spreads that all land in the variadic tail, after every regular parameter has a plain
    // argument, are packed into the tail array below; any other spread keeps the generic path.
    let spreads_only_in_variadic_tail = pack_tail_spreads
        && sig.variadic.is_some()
        && args.len() > regular_param_count
        && args
            .iter()
            .enumerate()
            .all(|(index, arg)| !is_spread_arg(arg) || index >= regular_param_count);
    if args.iter().any(is_spread_arg) && !spreads_only_in_variadic_tail {
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
        return lower_args(ctx, args);
    }
    let fixed_arg_count = if sig.variadic.is_some() {
        args.len().min(regular_param_count)
    } else {
        args.len()
    };
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
        return coerce_operands_to_params(ctx, sig, operands);
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
                && sig.defaults.iter().take(regular_param_count).skip(fixed_arg_count)
                    .flatten().any(crate::types::deferred_defaults::contains_deferred_error);
            if !capture_values && (index + 1 < args.len() || deferred_default) {
                root_prior_argument_preserving_reference_place(
                    ctx, sig, index, arg, value,
                )
            } else {
                value
            }
        })
        .collect();
    if !trim_trailing_defaults {
        for idx in fixed_arg_count..regular_param_count {
            let Some(Some(default)) = sig.defaults.get(idx) else {
                break;
            };
            operands.push(lower_arg_with_signature(ctx, sig, idx, default));
        }
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
    coerce_operands_to_params(ctx, sig, operands)
}

/// Returns whether two class names name the same class or one descends from the other.
///
/// PHP has single inheritance, so two unrelated classes can never describe one value. An interface
/// on either side answers true: a class may implement any number of them, so relatedness cannot be
/// ruled out and the union stays ambiguous (which turns the caller's guard selection away).
fn nominal_boundary_classes_are_related(
    ctx: &LoweringContext<'_, '_>,
    candidate: &str,
    source: &str,
) -> bool {
    if candidate.eq_ignore_ascii_case(source) {
        return true;
    }
    if !ctx.classes.contains_key(candidate) || !ctx.classes.contains_key(source) {
        return true;
    }
    class_descends_from(ctx, candidate, source) || class_descends_from(ctx, source, candidate)
}

/// Walks the parent chain of `class_name` looking for `ancestor`.
fn class_descends_from(ctx: &LoweringContext<'_, '_>, class_name: &str, ancestor: &str) -> bool {
    let mut current = Some(class_name.to_string());
    while let Some(name) = current {
        if name.eq_ignore_ascii_case(ancestor) {
            return true;
        }
        current = ctx
            .classes
            .get(name.as_str())
            .and_then(|class_info| class_info.parent.clone());
    }
    false
}
