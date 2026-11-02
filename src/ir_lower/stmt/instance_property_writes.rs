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
    // Readonly errors depend on runtime initialization, after receiver and RHS effects.
    let readonly_write = ctx.throw_access_sites.get(&span).and_then(|info| {
        if let ThrowAccessKind::ReadonlyProperty { class_name, property, initialization_error } = &info.kind {
            Some((class_name.clone(), format!("Cannot modify readonly property {}::${}", class_name, property),
                initialization_error.clone()))
        } else {
            None
        }
    });
    let reflection_write = ctx.throw_access_sites.get(&span).and_then(|info| {
        if let ThrowAccessKind::ReflectionReadOnlyProperty { property: blocked } = &info.kind {
            (blocked == property).then(|| blocked.clone())
        } else {
            None
        }
    });
    if readonly_write.is_some()
        || (reflection_write.is_some() && value.span.line == span.line && value.span.col == span.col)
    {
        if let ExprKind::NullCoalesce { value: read, default } = &value.kind {
            if let ExprKind::PropertyAccess { object: read_object, property: read_property } = &read.kind {
                if read_object.as_ref() == object && read_property == property {
                    // The readonly Error belongs only to the fallback write, never to the
                    // read-only keep branch of an already initialized property.
                    lower_readonly_coalesce_update(ctx, object, property, default, span);
                    return;
                }
            }
        }
    }
    let value_expr = value;
    let (mut receiver, lowered_value) = if crate::ir_lower::expr::is_static_property_write_chain(object) {
        let rhs = lower_expr(ctx, value_expr);
        let receiver = crate::ir_lower::expr::lower_static_property_write_chain(ctx, object, rhs, span);
        (receiver, rhs)
    } else {
        let object = crate::ir_lower::expr::lower_member_receiver_at_span(ctx, object, span);
        let receiver = property_write_receiver::PropertyWriteReceiver::new(ctx, object, span);
        let rhs = match object_property_type(ctx, object.value, property) {
            Some(property_ty) => {
                crate::ir_lower::expr::lower_expr_for_destination(ctx, value_expr, &property_ty)
            }
            None => lower_expr(ctx, value_expr),
        };
        (receiver, rhs)
    };
    let lowered_value = ctx.borrow_write_operand_if_needed(lowered_value, span);
    // Preserve the original PHP tag until the initialization probe has chosen its branch.
    // Only an authorized first write reaches the backend's weak typed-property guard.
    // Native buffers, pointers, and packed record addresses keep their exact storage type;
    // they have no PHP weak coercion or runtime tag for that guard to preserve.
    let lowered_value = if readonly_write.is_some()
        && !matches!(ctx.builder.value_php_type(lowered_value.value).codegen_repr(),
            PhpType::Mixed | PhpType::Buffer(_) | PhpType::Pointer(_) | PhpType::Packed(_))
    {
        ctx.box_value_as_mixed(lowered_value, PhpType::Mixed, Some(span))
    } else { lowered_value };
    // Trait copies retain their source spans. A recorded handler belongs only to a
    // Reflection receiver, even when an ordinary class uses the same trait method.
    let reflection_write = reflection_write.filter(|property| {
        reflection_receiver_has_public_mirror_handler(ctx, receiver.value, property)
    });
    if let Some(property) = reflection_write {
        super::readonly_receiver_guard::guard_readonly_write_receiver(
            ctx, &receiver, lowered_value, &property, span,
        );
        lower_reflection_property_write_exception(ctx, receiver, lowered_value, &property, span);
        return;
    }
    if let Some((class_name, overwrite_message, initialization_error)) = readonly_write {
        super::readonly_receiver_guard::guard_readonly_write_receiver(
            ctx, &receiver, lowered_value, property, span,
        );
        // The static receiver's runtime view is boxed. Preserve that representation while
        // lending the initialization probe the class established by this readonly site.
        let probe = if ctx.builder.value_php_type(receiver.value.value).codegen_repr() == PhpType::Mixed {
            let ty = PhpType::Union(vec![PhpType::Object(class_name), PhpType::Void]);
            let probe = ctx.emit_value(Op::Borrow, vec![receiver.value.value], None, ty,
                Op::Borrow.default_effects(), Some(span));
            ctx.builder.set_value_ownership(probe.value, Ownership::Borrowed);
            probe
        } else { receiver.value };
        let data = ctx.intern_string(property);
        let initialized = ctx.emit_value(Op::PropInitialized, vec![probe.value],
            Some(Immediate::Data(data)), PhpType::Bool, Op::PropInitialized.default_effects(), Some(span));
        let overwrite = ctx.builder.create_named_block("readonly.write.initialized", Vec::new());
        let initialize = ctx.builder.create_named_block("readonly.write.uninitialized", Vec::new());
        ctx.builder.terminate(Terminator::CondBr {
            cond: initialized.value, then_target: overwrite, then_args: Vec::new(),
            else_target: initialize, else_args: Vec::new(),
        });
        ctx.builder.position_at_end(overwrite);
        lower_readonly_write_error(ctx, receiver.clone(), lowered_value, &overwrite_message, span);
        ctx.builder.position_at_end(initialize);
        if let Some(message) = initialization_error {
            if ctx.expression_depth > 0 {
                lower_readonly_write_error_with_result(ctx, receiver.clone(), lowered_value, &message, span);
            } else {
                lower_readonly_write_error(ctx, receiver.clone(), lowered_value, &message, span);
            }
            return;
        }
    }
    receiver.narrow_for_assignment(ctx, property, lowered_value, span);
    let object = receiver.value;
    // A runtime SUBCLASS can declare `__set` where the receiver's STATIC class does not, and php
    // calls the accessor on such an instance. Only the runtime class can answer that, so the guard
    // asks it. Receiver and value are already lowered, once each and in source order, so the
    // guard adds no evaluation and both branches see exactly the same two values.
    let magic_classes = magic_accessor_subclasses(ctx, object.value, property, "__set");
    if !magic_classes.is_empty() {
        lower_property_assign_guarding_magic_subclasses(
            ctx,
            object,
            property,
            value_expr,
            lowered_value,
            &magic_classes,
            span,
        );
    } else {
        lower_property_assign_value(ctx, object, property, value_expr, lowered_value, false, span);
    }
    receiver.finish(ctx, span);
}

/// Confirms the lowered receiver's single object class owns the recorded Reflection handler.
fn reflection_receiver_has_public_mirror_handler(
    ctx: &LoweringContext<'_, '_>,
    receiver: LoweredValue,
    property: &str,
) -> bool {
    let ty = ctx.builder.value_php_type(receiver.value);
    let class = match &ty {
        PhpType::Object(class) => Some(class.as_str()),
        PhpType::Union(members) => {
            let mut classes = members.iter().filter_map(|member| match member {
                PhpType::Object(class) => Some(class.as_str()),
                _ => None,
            });
            let first = classes.next();
            first.filter(|first| classes.all(|class| class == *first))
        }
        _ => None,
    };
    class.is_some_and(|class| {
        crate::types::reflection_properties::class_has_public_mirror_write_handler(
            ctx.classes, class, property,
        )
    })
}

/// Releases independent evaluated operands before a readonly write throws its catchable Error.
pub(super) fn lower_readonly_write_error(
    ctx: &mut LoweringContext<'_, '_>, receiver: property_write_receiver::PropertyWriteReceiver,
    value: LoweredValue,
    message: &str, span: Span,
) {
    lower_property_write_exception(ctx, receiver, value, "Error", message, span);
}

/// Emits a rejected write whose assignment expression still lowers an unreachable RHS result.
fn lower_readonly_write_error_with_result(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: property_write_receiver::PropertyWriteReceiver,
    value: LoweredValue,
    message: &str,
    span: Span,
) {
    receiver.finish(ctx, span);
    if ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
    lower_throw_access_error_expr(ctx, message, span);
}

/// Releases independently evaluated operands before a property write raises an exception.
fn lower_property_write_exception(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: property_write_receiver::PropertyWriteReceiver,
    value: LoweredValue,
    exception_class: &str,
    message: &str,
    span: Span,
) {
    receiver.finish(ctx, span);
    if ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
    lower_throw_access_exception(ctx, exception_class, message, span);
}

/// Raises Reflection's direct-write exception with the receiver's runtime class name.
///
/// A function parameter typed as `ReflectionClass` can hold a user subclass, and PHP names that
/// concrete subclass in the message. The class-name bytes are copied into a published temporary
/// before the receiver is released, so temporary subclasses and eval-owned metadata cannot leave
/// the diagnostic pointing into dead storage.
fn lower_reflection_property_write_exception(
    ctx: &mut LoweringContext<'_, '_>,
    mut receiver: property_write_receiver::PropertyWriteReceiver,
    value: LoweredValue,
    property: &str,
    span: Span,
) {
    if ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
    let target = crate::ir::RuntimeFnId::GetClass;
    let runtime_class = ctx.emit_value(
        Op::RuntimeCall,
        vec![receiver.value.value],
        Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(target))),
        PhpType::Str,
        target.effects(),
        Some(span),
    );
    let persisted_class = ctx.emit_owned_value(
        Op::StrPersist,
        vec![runtime_class.value],
        None,
        PhpType::Str,
        Op::StrPersist.default_effects(),
        Some(span),
    );
    if ctx.value_is_owning_temporary(runtime_class) {
        crate::ir_lower::ownership::release_if_owned(ctx, runtime_class, Some(span));
    }
    let class_temp = ctx.declare_hidden_temp(PhpType::Str);
    ctx.store_local(&class_temp, persisted_class, PhpType::Str, Some(span));
    let class_slot = ctx.local_slots[&class_temp];
    crate::ir_lower::expr::register_owned_call_operand(ctx, class_slot, span);
    receiver.place_above_operand_roots(ctx, &[class_slot], span);
    receiver.finish(ctx, span);

    let concat = |left, right| {
        Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(left),
                op: crate::parser::ast::BinOp::Concat,
                right: Box::new(right),
            },
            span,
        )
    };
    let message = concat(
        concat(
            Expr::new(
                ExprKind::StringLiteral("Cannot set read-only property ".to_string()),
                span,
            ),
            Expr::new(ExprKind::Variable(class_temp.clone()), span),
        ),
        Expr::new(
            ExprKind::StringLiteral(format!("::${property}")),
            span,
        ),
    );
    let exception = lower_expr(
        ctx,
        &Expr::new(
            ExprKind::NewObject {
                class_name: crate::names::Name::unqualified("ReflectionException"),
                args: vec![message],
            },
            span,
        ),
    );
    crate::ir_lower::expr::retire_owned_call_operand(ctx, class_slot, span);
    // Assignment expressions still lower their unreachable result reads after this operation.
    ctx.emit_void(
        Op::ThrowException,
        vec![exception.value],
        None,
        Op::ThrowException.default_effects(),
        Some(span),
    );
}

/// Fuses the parser's readonly coalesce receiver temporary into a scoped EIR owner.
pub(super) fn lower_synthetic_readonly_coalesce(
    ctx: &mut LoweringContext<'_, '_>,
    body: &[Stmt],
) -> bool {
    let [capture, write] = body else { return false; };
    let StmtKind::Assign { name, value: receiver } = &capture.kind else { return false; };
    if !crate::names::is_generated_local_name(name) { return false; }
    let StmtKind::PropertyAssign { object, property, value } = &write.kind else { return false; };
    let ExprKind::Variable(object_name) = &object.kind else { return false; };
    let ExprKind::NullCoalesce { value: read, default } = &value.kind else { return false; };
    let ExprKind::PropertyAccess { object: read_object, property: read_property } = &read.kind else {
        return false;
    };
    if object_name != name || read_object != object || read_property != property
        || !ctx.throw_access_sites.get(&write.span).is_some_and(|info| {
            matches!(info.kind, ThrowAccessKind::ReadonlyProperty { .. })
        })
    {
        return false;
    }
    lower_readonly_coalesce_update(ctx, receiver, property, default, write.span);
    true
}

/// Evaluates and pins the receiver once across the probe and the lazy fallback write.
fn lower_readonly_coalesce_update(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
    default: &Expr,
    span: Span,
) {
    let receiver = lower_expr(ctx, object);
    let ty = ctx.builder.value_php_type(receiver.value);
    let name = ctx.declare_hidden_temp(ty.clone());
    let pinned = crate::ir_lower::ownership::acquire_lifetime_pin_if_refcounted(
        ctx, receiver, Some(span),
    );
    ctx.store_local(&name, pinned, ty, Some(span));
    crate::ir_lower::ownership::release_if_owned(ctx, receiver, Some(span));
    let slot = ctx.local_slots[&name];
    ctx.emit_void(
        Op::PushCallOperandOwner, Vec::new(), Some(Immediate::LocalSlot(slot)),
        Op::PushCallOperandOwner.default_effects(), Some(span),
    );
    let target = Expr::new(ExprKind::PropertyAccess {
        object: Box::new(Expr::new(ExprKind::Variable(name), object.span)),
        property: property.to_string(),
    }, span);
    crate::ir_lower::expr::lower_null_coalesce_update_stmt(ctx, &target, default, span);
    crate::ir_lower::expr::retire_owned_call_operand(ctx, slot, span);
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
    let property_ty = object_property_type(ctx, object.value, property).or_else(|| {
        let object_ty = ctx.builder.value_php_type(object.value);
        if let Some(class_name) = crate::ir_lower::expr::packed_receiver_class(ctx, &object_ty) {
            return ctx
                .packed_classes
                .get(class_name.trim_start_matches('\\'))
                .and_then(|info| info.fields.iter().find(|field| field.name == property))
                .map(|field| field.php_type.clone());
        }
        let class_name = crate::ir_lower::expr::extern_receiver_class(ctx, &object_ty)?;
        ctx.extern_classes
            .get(class_name.trim_start_matches('\\'))
            .and_then(|info| info.fields.iter().find(|field| field.name == property))
            .map(|field| field.php_type.clone())
    });
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
        Some(ty) => {
            let value =
                ctx.retype_packed_address_for_pointer_sink(value, &ty, Some(span));
            coerce_typed_assign_value(ctx, value, &ty, span)
        }
        None => value,
    };
    // A fixed-layout scalar field (a packed field, or an extern struct field of any C width)
    // accepts a boxed Mixed value only through a strict runtime narrowing to its own payload.
    // Without it the store would write the box POINTER into the field; with a coercion it would
    // silently truncate the overflow promotion an integer box exists to carry.
    let value = narrow_mixed_value_for_fixed_layout_field(ctx, object.value, property, value, span);
    let retained_abstract_setter = receiver_retains_abstract_setter(ctx, object.value, property);
    let value = box_value_for_runtime_shaped_receiver(ctx, object, value, span);
    // The backing slot may be wider than its inherited setter. Preserve the runtime value's
    // tag through storage coercion so the backend can validate the setter contract itself.
    let value = if retained_abstract_setter
        && !matches!(ctx.builder.value_php_type(value.value).codegen_repr(), PhpType::Mixed)
    {
        ctx.box_value_as_mixed(value, PhpType::Mixed, Some(span))
    } else {
        value
    };
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
    // `PropertyWriteReceiver` already keeps every independent receiver lease visible to unwind.
    // Do not publish a second pin for the same SSA owner here: pins acquire no reference, so two
    // records would both release that one lease if the store throws. An unknown runtime slot can
    // also reject a value before adopting its box, so protect that source independently. Keep the
    // outer receiver lease until source cleanup completes because retiring a fresh Stringable
    // source can invoke a destructor that throws after publication.
    let stored_property_ty = object_property_type(ctx, object.value, property);
    let source_pins = if value.value != object.value
        && (retained_abstract_setter || stored_property_ty.as_ref().is_none_or(|property_ty| {
            property_store_keeps_independent_ref(property_ty, &ctx.builder.value_php_type(value.value))
        }))
    {
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
    // The source is about to be released, so its unwind record must no longer own it.
    crate::ir_lower::expr::unpin_in_flight_owners(ctx, source_pins, span);
    // Undeclared dynamic properties store boxed Mixed values. Boxing retains a
    // concrete temporary payload just like a declared property store does.
    let property_ty = object_property_type(ctx, object.value, property).unwrap_or(PhpType::Mixed);
    if retained_abstract_setter {
        release_magic_set_value_after_call(ctx, value, span);
    } else if stored_property_ty.is_none() && ctx.value_is_owning_temporary(value) {
        // Runtime slots may copy a payload or retain its box; EIR owns the source in either arm.
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    } else {
        release_property_assignment_source_after_retaining_store(ctx, &property_ty, value, span);
    }
    // `PropertyWriteReceiver` owns every independent receiver lease, including a narrowed
    // object extracted from a boxed nullable static property. Its caller retires those leases
    // after this store returns. Releasing `object` here as well would free that narrowed object
    // twice and leave any freshly published property payload dangling.
}

/// Returns whether this receiver can dispatch the write through an inherited abstract setter.
fn receiver_retains_abstract_setter(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    property: &str,
) -> bool {
    match ctx.builder.value_php_type(object).codegen_repr() {
        PhpType::Object(class) => ctx.classes.get(class.trim_start_matches('\\'))
            .is_some_and(|info| info.retained_abstract_setter(property).is_some()),
        PhpType::Mixed | PhpType::Union(_) => ctx.classes.values()
            .any(|info| info.retained_abstract_setter(property).is_some()),
        _ => false,
    }
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

/// Narrows a boxed Mixed value assigned to a fixed-layout scalar field into the field's raw
/// payload.
///
/// A packed field, or an extern struct field reached through `ptr<T>`, stores raw bits, so the
/// box POINTER must never reach the store. Lowering carries a value boxed whenever its runtime
/// type is open: integer arithmetic is boxed for its overflow-to-float promotion
/// (`$rect->w = $w * 2`), float arithmetic over such a result stays boxed although the checker
/// types it `float` (`$p->x = 1.5 * ($i + 1)` in a counted loop), and a `mixed` value is boxed
/// by definition. Each scalar field kind takes its own strict op, which throws a catchable
/// `TypeError` naming the runtime type for a value the field cannot hold:
/// - `int` (every integer width): `Op::PackedFieldMixedToInt`, only an int passes;
/// - `float` and `f32`: `Op::PackedFieldMixedToFloat`, a float passes and an int widens;
/// - `bool`: `Op::PackedFieldMixedToBool`, only a bool passes.
///
/// The source box is released right after: the payload is a raw copy, so the box's lifetime
/// ends at the narrowing, not at the store. Other receivers, unboxed values, nested records,
/// pointer fields (the checker admits only pointer-typed values there) and extern `string`
/// fields pass through untouched.
fn narrow_mixed_value_for_fixed_layout_field(
    ctx: &mut LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    property: &str,
    value: LoweredValue,
    span: Span,
) -> LoweredValue {
    if !matches!(
        ctx.builder.value_php_type(value.value).codegen_repr(),
        PhpType::Mixed
    ) {
        return value;
    }
    let Some((field_kind, class_name, field_ty)) = fixed_layout_field(ctx, object, property) else {
        return value;
    };
    let (op, narrowed_ty) = match field_ty.codegen_repr() {
        PhpType::Int => (Op::PackedFieldMixedToInt, PhpType::Int),
        PhpType::Float => (Op::PackedFieldMixedToFloat, PhpType::Float),
        PhpType::Bool => (Op::PackedFieldMixedToBool, PhpType::Bool),
        _ => return value,
    };
    let message = format!(
        "{} {}::${} must be of type {}, ",
        field_kind, class_name, property, narrowed_ty
    );
    let data = ctx.intern_string(&message);
    // The narrowing throws for a value the field cannot hold. An owning source exists only as
    // SSA at that point, so park it in the unwind chain for the narrowing: a caught TypeError
    // then still releases the box and its payload, and runs a destructor it owes.
    let source_pins = crate::ir_lower::expr::pin_in_flight_owners(ctx, &[value.value], span);
    let narrowed = ctx.emit_value(
        op,
        vec![value.value],
        Some(Immediate::Data(data)),
        narrowed_ty,
        op.default_effects(),
        Some(span),
    );
    // The source is released right below, so its unwind record must no longer own it.
    crate::ir_lower::expr::unpin_in_flight_owners(ctx, source_pins, span);
    if ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
    narrowed
}

/// Returns the fixed-layout field a write through `object` lands on: the diagnostic's owner
/// word (`Packed field` or `Extern field`), the normalized class name, and the field's declared
/// PHP type. `None` for any other receiver or an unknown field.
fn fixed_layout_field(
    ctx: &LoweringContext<'_, '_>,
    object: crate::ir::ValueId,
    property: &str,
) -> Option<(&'static str, String, PhpType)> {
    let object_ty = ctx.builder.value_php_type(object);
    if let Some(class_name) = crate::ir_lower::expr::packed_receiver_class(ctx, &object_ty) {
        let normalized = class_name.trim_start_matches('\\').to_string();
        let field_ty = ctx
            .packed_classes
            .get(&normalized)?
            .fields
            .iter()
            .find(|field| field.name == property)?
            .php_type
            .clone();
        return Some(("Packed field", normalized, field_ty));
    }
    let normalized = crate::ir_lower::expr::extern_receiver_class(ctx, &object_ty)?;
    let field_ty = ctx
        .extern_classes
        .get(&normalized)?
        .fields
        .iter()
        .find(|field| field.name == property)?
        .php_type
        .clone();
    Some(("Extern field", normalized, field_ty))
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
    // The caller's `PropertyWriteReceiver` already publishes any independent receiver lease.
    // Only the additional call operands need records here; a duplicate object record would
    // release the same lease twice when `__set` throws.
    let pins = crate::ir_lower::expr::pin_in_flight_owners(
        ctx,
        &[property_name.value, value.value],
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
