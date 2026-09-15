//! Purpose:
//! Instance property writes, magic setters, and hook dispatch.
//!
//! Called from:
//! - `crate::ir_lower::stmt`.
//!
//! Key details:
//! - Preserves statement ordering, CFG shape, EIR effects, and ownership contracts.

use super::*;

/// Lowers an object property write.
pub(super) fn lower_property_assign(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
    value: &Expr,
    span: Span,
) {
    // A statically-decided readonly-property write outside the declaring
    // constructor raises a catchable `Error` in PHP rather than a compile-time
    // error, but the object and RHS expressions must still be evaluated first.
    let throw_access_message = ctx.throw_access_sites.get(&span).and_then(|info| {
        if let ThrowAccessKind::ReadonlyProperty { class_name, property } = &info.kind {
            Some(format!("Cannot modify readonly property {}::${}", class_name, property))
        } else {
            None
        }
    });
    let object = lower_expr(ctx, object);
    let value_expr = value;
    let lowered_value = lower_expr(ctx, value_expr);
    if let Some(message) = throw_access_message {
        if ctx.value_is_owning_temporary(object) {
            crate::ir_lower::ownership::release_if_owned(ctx, object, Some(span));
        }
        if ctx.value_is_owning_temporary(lowered_value) {
            crate::ir_lower::ownership::release_if_owned(ctx, lowered_value, Some(span));
        }
        lower_throw_access_error(ctx, &message, span);
        return;
    }
    // A runtime SUBCLASS can declare `__set` where the receiver's STATIC class does not, and php
    // calls the accessor on such an instance. Only the runtime class can answer that, so the guard
    // asks it. Receiver and value are already lowered, once each and in source order, so the
    // guard adds no evaluation and both branches see exactly the same two values.
    let magic_classes = magic_accessor_subclasses(ctx, object.value, property, "__set");
    if !magic_classes.is_empty() {
        return lower_property_assign_guarding_magic_subclasses(
            ctx,
            object,
            property,
            value_expr,
            lowered_value,
            &magic_classes,
            span,
        );
    }
    lower_property_assign_value(ctx, object, property, value_expr, lowered_value, false, span)
}

/// Emits the `instanceof` chain that hands a runtime subclass's `__set` its own call.
///
/// One guard per class, in declaration-id order, each falling through to the next; the last
/// fallthrough is the ordinary write. Every branch converges on one merge block, so the statement
/// leaves exactly one live path and the ownership retirement the ordinary path performs is the
/// only one either branch owes.
fn lower_property_assign_guarding_magic_subclasses(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: &str,
    value_expr: &Expr,
    lowered_value: LoweredValue,
    magic_classes: &[String],
    span: Span,
) {
    let merge = ctx
        .builder
        .create_named_block("prop_assign.magic.merge", Vec::new());
    for class_name in magic_classes {
        let magic_block = ctx
            .builder
            .create_named_block("prop_assign.magic.call", Vec::new());
        let next_block = ctx
            .builder
            .create_named_block("prop_assign.magic.next", Vec::new());
        let matched = emit_receiver_instanceof(ctx, object.value, class_name, span);
        ctx.builder.terminate(Terminator::CondBr {
            cond: matched,
            then_target: magic_block,
            then_args: Vec::new(),
            else_target: next_block,
            else_args: Vec::new(),
        });
        ctx.builder.position_at_end(magic_block);
        // The call is resolved against the class the guard proved, not against the receiver's
        // static type, which does not declare the accessor at all.
        let guarded = borrow_receiver_as_runtime_class(ctx, object, class_name, span);
        lower_magic_property_set(ctx, guarded.value, property, lowered_value, span);
        branch_to(ctx, merge);
        ctx.builder.position_at_end(next_block);
    }
    // Preserve the literal as an operand on the ordinary arm too. The backend still materializes
    // every runtime-class arm, including the accessor subclasses peeled off above, and their
    // deferred magic call needs the name even though control flow cannot reach it on this arm.
    lower_property_assign_value(ctx, object, property, value_expr, lowered_value, true, span);
    branch_to(ctx, merge);
    ctx.builder.position_at_end(merge);
}

/// Lowers the ordinary object property write, once the receiver and the value are evaluated.
fn lower_property_assign_value(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    property: &str,
    value_expr: &Expr,
    lowered_value: LoweredValue,
    preserve_property_operand: bool,
    span: Span,
) {
    let value = contextualize_property_array_assignment(
        ctx,
        object.value,
        property,
        lowered_value,
        value_expr,
        span,
    );
    // Property slots use their declared/inferred storage representation. In particular, an
    // untyped property widened to Mixed needs a boxed cell even when this assignment is scalar.
    let property_ty = object_property_type(ctx, object.value, property);
    let value = match property_ty {
        // A runtime-shaped value assigned to a class-typed property stays BOXED here. Unboxing
        // it now would promote whatever the cell holds to an object pointer sight unseen, and
        // a null or an unrelated class would be stored instead of raising PHP's `TypeError`.
        // The backend's weak-mode property guard checks the runtime class and then unboxes.
        Some(ty)
            if declared_property_type(ctx, object.value, property)
                && boxed_value_for_class_typed_property(ctx, &ty, value) =>
        {
            value
        }
        Some(ty) => coerce_typed_assign_value(ctx, value, &ty, span),
        None => value,
    };
    // A packed `int` field accepts a boxed Mixed value only through a strict runtime
    // narrowing (int tag → raw payload, anything else → TypeError). Without it the packed
    // store would write the box POINTER into fixed field storage; with a coercion it would
    // silently truncate the overflow promotion the box exists to carry.
    let value = narrow_mixed_value_for_packed_int_field(ctx, object.value, property, value, span);
    let value = box_value_for_runtime_shaped_receiver(ctx, object, value, span);
    if magic_set_receiver_has_method(ctx, object.value, property) {
        lower_magic_property_set(ctx, object.value, property, value, span);
        return;
    }
    // Route a write to a set-hooked property to its `__propset_<p>($value)` accessor, except inside
    // that property's own accessor where `$this->prop = v` must write the raw backing slot.
    if set_hook_receiver_has_accessor(ctx, object.value, property)
        && !ctx.in_own_property_accessor(property)
    {
        lower_property_hook_set(ctx, object.value, property, value, span);
        return;
    }
    let data = ctx.intern_string(property);
    // A declared property store carries MAY_THROW: the weak typed-property guard raises PHP's
    // catchable TypeError, and a Stringable receiver can throw out of its own __toString. Both
    // jump to __rt_throw_current with the assigned temporary still pure SSA, so pin it in the
    // unwind chain for the store and retire the pin without releasing once the store returns.
    //
    // Only a store that keeps an INDEPENDENT owner is pinned, which is exactly the condition
    // `release_property_assignment_source_after_retaining_store` releases under. A store that
    // instead transfers the temporary into the slot owes no release afterwards, and pinning it
    // would leave one reference nothing retires.
    let stored_property_ty = object_property_type(ctx, object.value, property).unwrap_or(PhpType::Mixed);
    let pins = if property_store_keeps_independent_ref(
        &stored_property_ty,
        &ctx.builder.value_php_type(value.value),
    ) {
        crate::ir_lower::expr::pin_in_flight_owners(ctx, &[value.value], span)
    } else {
        Vec::new()
    };
    if preserve_property_operand {
        let property_name = ctx.emit_value(
            Op::ConstStr,
            Vec::new(),
            Some(Immediate::Data(data)),
            PhpType::Str,
            Op::ConstStr.default_effects(),
            Some(span),
        );
        ctx.emit_void(
            Op::DynamicPropSet,
            vec![object.value, property_name.value, value.value],
            None,
            Op::DynamicPropSet.default_effects(),
            Some(span),
        );
    } else {
        ctx.emit_void(
            Op::PropSet,
            vec![object.value, value.value],
            Some(Immediate::Data(data)),
            Op::PropSet.default_effects(),
            Some(span),
        );
    }
    crate::ir_lower::expr::unpin_in_flight_owners(ctx, pins, span);
    // Undeclared dynamic properties store boxed Mixed values. Boxing retains a
    // concrete temporary payload just like a declared property store does.
    let property_ty = object_property_type(ctx, object.value, property).unwrap_or(PhpType::Mixed);
    release_property_assignment_source_after_retaining_store(ctx, &property_ty, value, span);
}

/// Boxes a concrete value assigned through a receiver whose CLASS is only known at run time.
///
/// A boxed `Mixed` or union receiver names no class, so the slot this store lands on is decided by
/// the runtime class and can be a TYPED one. Handing the backend a concrete value left it deciding
/// acceptance STATICALLY, and a static answer cannot tell php's weak-mode COERCION (`'5'` into an
/// `int` slot, which php stores as `5`) from php's REFUSAL (`'nope'`, which php raises a
/// `TypeError` for). The class it could not model was then dropped from the runtime-class
/// dispatch, and the assignment vanished into a miss path that understands `stdClass` alone.
///
/// Boxing hands the same value to `mixed_property_type_guard`, which is the only thing that can
/// see the runtime tag, so php's two answers are both produced by the code that already knows how.
/// A receiver whose class IS known keeps its concrete value and its compile-time check.
pub(crate) fn box_value_for_runtime_shaped_receiver(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    value: LoweredValue,
    span: Span,
) -> LoweredValue {
    if !matches!(
        ctx.builder.value_php_type(object.value).codegen_repr(),
        PhpType::Mixed | PhpType::Union(_)
    ) {
        return value;
    }
    if matches!(
        ctx.builder.value_php_type(value.value).codegen_repr(),
        PhpType::Mixed
    ) {
        return value;
    }
    ctx.box_value_as_mixed(value, PhpType::Mixed, Some(span))
}

/// Narrows a boxed Mixed value assigned to a packed `int` field into its raw `I64` payload.
///
/// Emits `Op::PackedFieldMixedToInt` (int tag passes, every other runtime tag throws a
/// catchable `TypeError` naming the runtime type) and releases the source box right after:
/// the payload is a raw copy, so the box's lifetime ends at the narrowing, not at the store.
/// Non-packed receivers, non-Mixed values, and non-int fields pass through untouched.
fn narrow_mixed_value_for_packed_int_field(
    ctx: &mut LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    property: &str,
    value: LoweredValue,
    span: Span,
) -> LoweredValue {
    let PhpType::Packed(class_name) = ctx.builder.value_php_type(object).codegen_repr() else {
        return value;
    };
    if !matches!(
        ctx.builder.value_php_type(value.value).codegen_repr(),
        PhpType::Mixed
    ) {
        return value;
    }
    let normalized = class_name.trim_start_matches('\\');
    let Some(field_ty) = ctx
        .packed_classes
        .get(normalized)
        .and_then(|info| info.fields.iter().find(|field| field.name == property))
        .map(|field| field.php_type.codegen_repr())
    else {
        return value;
    };
    if field_ty != PhpType::Int {
        return value;
    }
    let message = format!(
        "Packed field {}::${} must be of type int, ",
        normalized, property
    );
    let data = ctx.intern_string(&message);
    let narrowed = ctx.emit_value(
        Op::PackedFieldMixedToInt,
        vec![value.value],
        Some(Immediate::Data(data)),
        PhpType::Int,
        Op::PackedFieldMixedToInt.default_effects(),
        Some(span),
    );
    if ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
    narrowed
}

/// Returns the runtime subclasses whose own accessor php would call for this name.
///
/// The receiver's STATIC class is excluded: if it declares the accessor, the ordinary
/// static-class check already routed the whole access there and no guard is needed. What is left
/// is the polymorphic case, where a subclass adds `__set`, `__get` or `__unset` its parent does
/// not, and php calls it on an instance of that subclass. A class is only listed when php would
/// really consult the accessor on it, which means the name does not resolve to a slot visible
/// from this scope there.
pub(crate) fn magic_accessor_subclasses(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    property: &str,
    method: &str,
) -> Vec<String> {
    let PhpType::Object(class_name) = ctx.builder.value_php_type(object).codegen_repr() else {
        return Vec::new();
    };
    let normalized = class_name.trim_start_matches('\\');
    let method_key = php_symbol_key(method);
    let static_declares = ctx
        .classes
        .get(normalized)
        .is_some_and(|class_info| class_info.methods.contains_key(&method_key));
    if static_declares {
        return Vec::new();
    }
    let mut classes = ctx
        .classes
        .iter()
        .filter(|(candidate, candidate_info)| {
            candidate_info.methods.contains_key(&method_key)
                && crate::types::class_inherits_from(ctx.classes, candidate, normalized)
                && !property_resolves_to_visible_slot(ctx, candidate, property)
        })
        .map(|(candidate, _)| candidate.clone())
        .collect::<Vec<_>>();
    classes.sort();
    classes
}

/// Returns whether php resolves the name to a slot this scope can see on that class.
fn property_resolves_to_visible_slot(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    property: &str,
) -> bool {
    matches!(
        crate::types::resolve_property_name(
            ctx.classes,
            class_name,
            property,
            ctx.current_class.as_deref(),
        ),
        crate::types::PropertyNameResolution::Visible
            | crate::types::PropertyNameResolution::ScopePrivate { .. }
    )
}

/// Returns the already-lowered receiver RE-TYPED as the runtime subclass the guard just proved.
///
/// Method dispatch resolves `__set` and `__unset` from the RECEIVER VALUE's static class, so
/// handing the guarded branch the base-typed value looked the accessor up on a class that does
/// not declare it, which is the one class php would never call. The guard has already proved the
/// instance IS a `class_name`, so the branch may say so.
///
/// `Op::Borrow` forwards the same pointer and lowers to a plain value copy, so this adds no
/// evaluation, no allocation and no refcount work: the result is `Ownership::Borrowed` and the
/// original value keeps every lease it had. Receiver, name and value are still evaluated exactly
/// once, before the guard.
pub(crate) fn borrow_receiver_as_runtime_class(
    ctx: &mut LoweringContext<'_, '_>,
    object: LoweredValue,
    class_name: &str,
    span: Span,
) -> LoweredValue {
    let value = ctx
        .builder
        .emit_with_effects(
            Op::Borrow,
            vec![object.value],
            None,
            object.ir_type,
            PhpType::Object(class_name.trim_start_matches('\\').to_string()),
            Ownership::Borrowed,
            Op::Borrow.default_effects(),
            Some(span),
        )
        .expect("receiver retype borrow produces a value");
    LoweredValue {
        value,
        ir_type: object.ir_type,
    }
}

/// Emits `$receiver instanceof ClassName` over an already-lowered receiver.
pub(crate) fn emit_receiver_instanceof(
    ctx: &mut LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    class_name: &str,
    span: Span,
) -> crate::ir::ValueId {
    let target = ctx.intern_class_name(class_name);
    ctx.emit_value(
        Op::InstanceOf,
        vec![object],
        Some(Immediate::Data(target)),
        PhpType::Bool,
        Op::InstanceOf.default_effects(),
        Some(span),
    )
    .value
}

/// Returns true when a property write should dispatch to `__set`.
pub(super) fn magic_set_receiver_has_method(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    property: &str,
) -> bool {
    let PhpType::Object(class_name) = ctx.builder.value_php_type(object).codegen_repr() else {
        return false;
    };
    let normalized = class_name.trim_start_matches('\\');
    let Some(class_info) = ctx.classes.get(normalized) else {
        return false;
    };
    // A slot this SCOPE does not resolve the name to is not a declaration for this decision.
    // php 7.4 removed shadow properties, so a strict ancestor's private name is not in the
    // child's by-name table at all: php consults `__set` for it exactly as it does for a name
    // the class never declared, measured on php 8.5.10 from the child scope and from global
    // scope alike. Testing the PHYSICAL table alone sent that write past `__set` and into the
    // ancestor's slot.
    if class_info
        .properties
        .iter()
        .any(|(name, _)| name == property)
        && !crate::types::property_name_shadows_ancestor_private_slot(
            ctx.classes,
            normalized,
            property,
            ctx.current_class.as_deref(),
        )
    {
        return false;
    }
    class_info.methods.contains_key(&php_symbol_key("__set"))
}

/// Lowers an undeclared property write through the guarded dynamic-property operation.
pub(super) fn lower_magic_property_set(
    ctx: &mut LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    property: &str,
    value: LoweredValue,
    span: Span,
) {
    let property_data = ctx.intern_string(property);
    let property_name = ctx.emit_value(
        Op::ConstStr,
        Vec::new(),
        Some(Immediate::Data(property_data)),
        PhpType::Str,
        Op::ConstStr.default_effects(),
        Some(span),
    );
    let pins = crate::ir_lower::expr::pin_in_flight_owners(
        ctx,
        &[object, property_name.value, value.value],
        span,
    );
    ctx.emit_void(
        Op::DynamicPropSet,
        vec![object, property_name.value, value.value],
        None,
        Op::DynamicPropSet.default_effects(),
        Some(span),
    );
    crate::ir_lower::expr::unpin_in_flight_owners(ctx, pins, span);
    release_magic_set_value_after_call(ctx, value, span);
}

/// Releases an owning RHS temporary after a magic or hooked setter has retained it.
pub(super) fn release_magic_set_value_after_call(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
) {
    if ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
}

/// Returns true when the runtime class of `object` declares a `__propset_<property>` set-hook
/// accessor, meaning a write to `property` should be routed through it.
pub(super) fn set_hook_receiver_has_accessor(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    property: &str,
) -> bool {
    let PhpType::Object(class_name) = ctx.builder.value_php_type(object).codegen_repr() else {
        return false;
    };
    let normalized = class_name.trim_start_matches('\\');
    ctx.classes.get(normalized).is_some_and(|info| {
        info.methods
            .contains_key(&php_symbol_key(&property_hook_set_method(property)))
    })
}

/// Lowers a write to a set-hooked property as a call to its `__propset_<p>($value)` accessor,
/// passing the assigned value as the single argument and releasing it if it was an owning temporary.
pub(super) fn lower_property_hook_set(
    ctx: &mut LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    property: &str,
    value: LoweredValue,
    span: Span,
) {
    let method_data = ctx.intern_string(&property_hook_set_method(property));
    ctx.emit_void(
        Op::MethodCall,
        vec![object, value.value],
        Some(Immediate::Data(method_data)),
        Op::MethodCall.default_effects(),
        Some(span),
    );
    release_magic_set_value_after_call(ctx, value, span);
}

/// Converts array literals to hash storage when a declared object property requires assoc storage.
pub(super) fn contextualize_property_array_assignment(
    ctx: &mut LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    property: &str,
    lowered: LoweredValue,
    value_expr: &Expr,
    span: Span,
) -> LoweredValue {
    let Some(contextual_ty) = object_property_type(ctx, object, property) else {
        return lowered;
    };
    contextualize_property_array_value(ctx, lowered, value_expr, &contextual_ty, span)
}

/// Consumes a fresh indexed literal when the physical property requires associative storage.
pub(in crate::ir_lower) fn contextualize_property_array_value(
    ctx: &mut LoweringContext<'_, '_>,
    lowered: LoweredValue,
    value_expr: &Expr,
    contextual_ty: &PhpType,
    span: Span,
) -> LoweredValue {
    let php_type = ctx.builder.value_php_type(lowered.value);
    if !matches!(value_expr.kind, ExprKind::ArrayLiteral(_)) {
        return lowered;
    }
    if !matches!(php_type.codegen_repr(), PhpType::Array(_)) {
        return lowered;
    }
    let contextual_ty = contextual_ty.codegen_repr();
    if !matches!(contextual_ty, PhpType::AssocArray { .. }) {
        return lowered;
    }
    ctx.emit_value(
        Op::ArrayToHash,
        vec![lowered.value],
        None,
        contextual_ty,
        Op::ArrayToHash.default_effects(),
        Some(span),
    )
}

/// Returns true when a boxed value is assigned to a class-typed property slot.
fn boxed_value_for_class_typed_property(
    ctx: &LoweringContext<'_, '_>,
    property_ty: &PhpType,
    value: LoweredValue,
) -> bool {
    matches!(property_ty.codegen_repr(), PhpType::Object(_))
        && ctx.builder.value_php_type(value.value).codegen_repr() == PhpType::Mixed
}

/// Returns true when the receiver's class declares a PHP type for this property.
///
/// An untyped property has no weak-mode property typing to enforce, so its writes keep the
/// previous unboxing lowering.
fn declared_property_type(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    property: &str,
) -> bool {
    let PhpType::Object(class_name) = ctx.builder.value_php_type(object).codegen_repr() else {
        return false;
    };
    ctx.classes
        .get(class_name.trim_start_matches('\\'))
        .is_some_and(|class_info| class_info.visible_property_is_declared(property))
}
