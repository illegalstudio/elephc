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
    let lowered = match sig.params.get(index) {
        Some((_, param_ty)) => lower_expr_for_parameter(ctx, arg, param_ty),
        None => lower_expr(ctx, arg),
    };
    record_pointer_reference_argument(ctx, sig, index, arg);
    coerce_scalar_arg_to_param_storage(ctx, sig, index, lowered, arg).value
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
    let condition = if class_name.is_empty() || class_name.eq_ignore_ascii_case("object") {
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
            .is_some_and(|(_, ty)| matches!(ty, PhpType::Mixed | PhpType::Object(_)))
    {
        return;
    }
    let ExprKind::Variable(name) = &arg.kind else {
        return;
    };
    if !ctx.boxed_reference_promotion_is_authorized(name, arg.span) {
        return;
    }
    if !ctx.is_ref_bound_local(name) && ctx.local_is_promotable_to_ref_cell(name) {
        ctx.promote_local_mixed_ref_cell(name, Some(arg.span));
    }
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
        if matches!(arg.kind, ExprKind::Variable(_))
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
    let prefix = match ctx.throw_access_sites.get(&(scope, arg.span)).map(|info| &info.kind) {
        Some(crate::types::ThrowAccessKind::CallArgumentIntCoercion { prefix }) => prefix.clone(),
        _ => return None,
    };
    let file = definite_argument_errors::call_site_file(ctx, arg.span);
    let given = match ctx.builder.value_php_type(value.value).codegen_repr() {
        PhpType::Float => "float",
        _ => "string",
    };
    let data = ctx.intern_string(&format!(
        "{}{} given, called in {} on line {}",
        prefix, given, file, arg.span.line
    ));
    Some(ctx.emit_value(
        Op::ArgumentStringToInt,
        vec![value.value],
        Some(Immediate::Data(data)),
        PhpType::Int,
        Op::ArgumentStringToInt.default_effects(),
        Some(arg.span),
    ))
}

pub(super) fn coerce_scalar_arg_to_param_storage(
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
    // The bare `object` type is spelled `Object("")`: it names no class to test an instance
    // against, so the declared-class guard below does not apply to it.
    if !by_reference {
        if let PhpType::Object(class_name) = param_ty {
        if !class_name.is_empty() {
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
        return unbox_callable_param_storage(ctx, value, Some(arg.span));
    }
    if param_ty == PhpType::Iterable
        && !sig.ref_params.get(index).copied().unwrap_or(false)
        && matches!(source_ty, PhpType::Mixed | PhpType::Union(_))
    {
        return unbox_iterable_param_storage(ctx, value, Some(arg.span));
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

/// Extracts a statically checked callable whose merge storage became a Mixed cell.
/// The backend retains the descriptor, so the extracted EIR value must own that lease.
pub(super) fn unbox_callable_param_storage(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Option<crate::span::Span>,
) -> LoweredValue {
    // NORMALIZED, not merely unboxed: a boxed value may hold any callable spelling (`"strlen"`,
    // `[$o, 'm']`, a closure) or no callable at all. A bare unbox read `null` or `42` as a
    // descriptor pointer and the callee's first call through it segfaulted;
    // `NormalizeCallable` resolves the spellings and refuses the rest.
    let result = ctx.emit_owned_value(
        Op::NormalizeCallable,
        vec![value.value],
        None,
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
    let class_data = ctx.intern_class_name(class_name);
    let condition = ctx.emit_value(
        Op::InstanceOf,
        vec![value.value],
        Some(Immediate::Data(class_data)),
        PhpType::Bool,
        Op::InstanceOf.default_effects(),
        span,
    );
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
        class_name
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
            let coerced = unbox_array_param_storage(ctx, lowered, param_ty.clone(), None);
            operands[index] =
                root_evaluated_call_argument(ctx, coerced, Span::dummy()).value;
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
        || (sig.declared_params.get(index).copied().unwrap_or(false)
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
    if crate::superglobals::is_superglobal(name) || !ctx.local_uses_global_storage(name) {
        return None;
    }
    let temp = crate::names::generated_local_name(&format!("global_ref_arg_{name}"));
    ctx.bind_superglobal_ref_alias(&temp, name, Some(arg.span));
    Some(ctx.load_local(&temp, Some(arg.span)).value)
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
        if param_ty.codegen_repr() == PhpType::Mixed && value.codegen_repr() != PhpType::Mixed {
            // The callee stores a boxed value through this entry's cell, so the hash's value type
            // becomes `mixed` first, as the list branch below widens its slots. Left typed, a later
            // `$map["k"]` read the box `append(array &$v)` published as a raw `array<int>`, and
            // `implode()` printed its slot pointers.
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
