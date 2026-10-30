//! Purpose:
//! Classifies and lowers element unsets through instance and static properties.
//!
//! Called from:
//! - `super::unset_target_supported()` and `super::lower_unset_array_access()`.
//!
//! Key details:
//! - Keys precede delayed property fetches, and callback operands remain unwind-visible.
//! - Array storage is separated in place without overwriting destructor mutations.

use super::*;

/// Returns the property storage type when `unset($object->prop[$key])` lowers directly.
///
/// Two property shapes qualify, the two the element-write path already mutates in place:
///
/// - a declared PHP `array` (the boxed packed-or-hash contract), removed through `OffsetUnset`
///   after the cell is detached, which is how a declared-array local is handled;
/// - an associative `array<K, V>` (an untyped property whose values are keyed), removed through
///   `HashUnset`.
///
/// A packed indexed `array<T>` property is refused on purpose. `unset()` leaves a key hole, so
/// the storage would have to become a hash, and a property slot is not the unset site's to
/// retype: every other method of the class reads that slot as a packed list. A declared `array`
/// property is the boxed contract instead, which is why the ordinary spelling
/// (`private array $items`) is covered. The receiver must be one non-null object of a known
/// class, and the property must be declared and reachable from this scope; anything else keeps
/// the shared unsupported diagnostic.
pub(super) fn unset_array_access_property_array_type(
    ctx: &LoweringContext<'_, '_>,
    array: &Expr,
) -> Option<PhpType> {
    let ExprKind::PropertyAccess { object, property } = &array.kind else {
        return None;
    };
    let property_ty = unset_receiver_declared_property_type(ctx, object, property)?;
    if property_ty.is_php_array() {
        return Some(property_ty);
    }
    if boxed_property_may_hold_array(&property_ty) {
        return Some(PhpType::Mixed);
    }
    match property_ty.codegen_repr() {
        assoc @ PhpType::AssocArray { .. } => Some(assoc),
        _ => None,
    }
}

/// Returns true when a property is stored as a boxed `Mixed` cell that may hold a PHP array and
/// is never declared to hold an object.
///
/// That covers an untyped property type checking widened to `Mixed` (after `unset($o->items)`,
/// for example), a `mixed` property, and a union such as `?array`. Its element is removed through
/// the same `PropGetForWrite` + `OffsetUnset` pair as a declared `array` property; the backend
/// answers PHP's no-op for a null cell and PHP's `Error` for a scalar. A union that can hold an
/// object (a class, `iterable`, `callable`) keeps the `ArrayAccess` or unsupported paths. A bare
/// `Mixed` can hold an object too: `OffsetUnset` hands it to `__rt_mixed_object_offset_unset`,
/// which calls its `offsetUnset()`, as it does for a by-reference `Mixed` local.
fn boxed_property_may_hold_array(property_ty: &PhpType) -> bool {
    match property_ty {
        PhpType::Mixed => true,
        PhpType::Union(members) => {
            property_ty.codegen_repr() == PhpType::Mixed
                && members.iter().any(|member| {
                    matches!(
                        member.codegen_repr(),
                        PhpType::Array(_) | PhpType::AssocArray { .. }
                    )
                })
                && members.iter().all(|member| {
                    matches!(
                        member,
                        PhpType::Array(_)
                            | PhpType::AssocArray { .. }
                            | PhpType::Int
                            | PhpType::Float
                            | PhpType::Str
                            | PhpType::Bool
                            | PhpType::False
                            | PhpType::Void
                    )
                })
        }
        _ => false,
    }
}

/// Returns true when `unset($object->prop[$key])` targets a property holding an ArrayAccess
/// object, which dispatches to that object's `offsetUnset($key)`.
pub(super) fn unset_array_access_has_array_access_property_receiver(
    ctx: &LoweringContext<'_, '_>,
    array: &Expr,
) -> bool {
    let ExprKind::PropertyAccess { object, property } = &array.kind else {
        return false;
    };
    unset_receiver_declared_property_type(ctx, object, property)
        .is_some_and(|ty| type_satisfies_array_access_for_ir(ctx, &ty))
}

/// Returns the declared type of `$object->property` when the receiver is one non-null object of
/// a known class and the property is declared and accessible from the current scope.
fn unset_receiver_declared_property_type(
    ctx: &LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
) -> Option<PhpType> {
    let (class_name, nullable) = instance_callable_object_class_and_nullability(ctx, object)?;
    if nullable {
        return None;
    }
    let class_info = ctx.classes.get(class_name.as_str())?;
    if !property_is_accessible_for_ir(ctx, &class_name, class_info, property) {
        return None;
    }
    class_info
        .visible_property(property)
        .map(|(_, (_, property_ty))| property_ty.clone())
}

/// Returns the static receiver and property when `unset(Class::$prop[$key])` lowers directly,
/// which is the case for a declared PHP `array` static property.
pub(super) fn unset_array_access_static_php_array_receiver<'e>(
    ctx: &LoweringContext<'_, '_>,
    array: &'e Expr,
) -> Option<(&'e StaticReceiver, &'e str)> {
    let ExprKind::StaticPropertyAccess { receiver, property } = &array.kind else {
        return None;
    };
    crate::ir_lower::stmt::static_property_type(ctx, receiver, property)
        .is_some_and(|ty| ty.is_php_array())
        .then_some((receiver, property.as_str()))
}

/// Explains why `unset()` of a property ARRAY ELEMENT has no lowering, for the shapes that are
/// refused on purpose rather than simply not implemented.
///
/// Both refusals name the declared `array` type as the fix, because that is the representation
/// that can hold the key hole `unset()` leaves:
///
/// - a packed indexed `array<T>` object property (an untyped property whose default is a list)
///   would have to become a hash, which retypes a slot every method of the class reads;
/// - an untyped static array property has no separated-cell write path to remove through.
///
/// Recorded as a source diagnostic so the user sees the line, not the backend's generic
/// unsupported-shape message.
pub(super) fn unset_property_element_refusal(ctx: &LoweringContext<'_, '_>, arg: &Expr) -> Option<String> {
    let ExprKind::ArrayAccess { array, .. } = &arg.kind else {
        return None;
    };
    match &array.kind {
        ExprKind::PropertyAccess { object, property } => {
            let property_ty = unset_receiver_declared_property_type(ctx, object, property)?;
            if property_ty.is_php_array()
                || !matches!(property_ty.codegen_repr(), PhpType::Array(_))
            {
                return None;
            }
            Some(format!(
                "Unsupported unset(): removing an element of `${property}` needs hash storage, \
                 because PHP's unset() leaves a key hole without renumbering, but this untyped \
                 property holds a packed list and every method of its class reads the slot as \
                 one. Declare the property `array` (for example `public array ${property} = \
                 [...];`), whose storage can hold sparse keys"
            ))
        }
        ExprKind::StaticPropertyAccess { receiver, property } => {
            let property_ty = crate::ir_lower::stmt::static_property_type(ctx, receiver, property)?;
            if property_ty.is_php_array()
                || !matches!(
                    property_ty.codegen_repr(),
                    PhpType::Array(_) | PhpType::AssocArray { .. }
                )
            {
                return None;
            }
            Some(format!(
                "Unsupported unset(): an element of static property `${property}` can be removed \
                 only when the property is declared `array` (for example `public static array \
                 ${property} = [...];`)"
            ))
        }
        _ => None,
    }
}

/// Lowers `unset($object->prop[$key])` for a declared-array or associative property.
///
/// PHP's order is receiver, key, then the property fetch, so both operands are lowered by
/// [`lower_unset_receiver_and_key`] and the property is read by `lower_property_array_unset`
/// last. The removed value's release can run a destructor that throws, so an owning receiver
/// temporary and the key are rooted in an unwind-visible slot across the removal, the same
/// protection the declared-array local path gives its key. A typed property that was never
/// initialized (or was unset) is left alone, as PHP's quiet unset fetch does, instead of reaching
/// the write fetch's uninitialized-property guard.
pub(super) fn lower_unset_property_array_element(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
    property_ty: &PhpType,
    index: &Expr,
    expr: &Expr,
) {
    let typed = unset_receiver_property_is_typed(ctx, object, property);
    let (object_value, object_owner, index_value) = lower_unset_receiver_and_key(ctx, object, index);
    let (index_value, key_owner) = root_owned_call_operand(ctx, index_value, index.span);
    let join_block = if typed {
        let data = ctx.intern_string(property);
        let initialized = ctx.emit_value(
            Op::PropInitialized,
            vec![object_value.value],
            Some(Immediate::Data(data)),
            PhpType::Bool,
            Op::PropInitialized.default_effects(),
            Some(expr.span),
        );
        let remove_block = ctx
            .builder
            .create_named_block("unset.property_element.remove", Vec::new());
        let join_block = ctx
            .builder
            .create_named_block("unset.property_element.done", Vec::new());
        ctx.builder.terminate(Terminator::CondBr {
            cond: initialized.value,
            then_target: remove_block,
            then_args: Vec::new(),
            else_target: join_block,
            else_args: Vec::new(),
        });
        ctx.builder.position_at_end(remove_block);
        Some(join_block)
    } else {
        None
    };
    crate::ir_lower::stmt::lower_property_array_unset(
        ctx,
        object_value,
        property,
        property_ty,
        index_value,
        expr.span,
    );
    if let Some(join_block) = join_block {
        if !ctx.builder.insertion_block_is_terminated() {
            ctx.builder.terminate(Terminator::Br {
                target: join_block,
                args: Vec::new(),
            });
        }
        ctx.builder.position_at_end(join_block);
    }
    if let Some(slot) = key_owner {
        retire_owned_call_operand(ctx, slot, expr.span);
    }
    if let Some(slot) = object_owner {
        retire_owned_call_operand(ctx, slot, expr.span);
    }
}

/// Returns true when `$object->prop` names a property with a declared type, which PHP leaves
/// uninitialized (rather than null) until it is assigned.
fn unset_receiver_property_is_typed(
    ctx: &LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
) -> bool {
    instance_callable_object_class_and_nullability(ctx, object)
        .and_then(|(class_name, _)| ctx.classes.get(class_name.as_str()))
        .is_some_and(|class_info| class_info.declared_properties.contains(property))
}

/// Lowers the receiver and the key of `unset($object->prop[$key])` in PHP's order.
///
/// PHP delays the property fetch of an unset target until after its dimension, and that delayed
/// fetch reads a compiled variable (or `$this`) where it stands. So a variable receiver is read
/// AFTER the key: a key that reassigns the variable reaches the new object, and nothing is left
/// pointing at an object the key may have released. Any other receiver is an expression PHP
/// evaluates before the key; its value is lowered first and, when it is an owning temporary,
/// rooted in the returned owner slot, which the caller retires.
fn lower_unset_receiver_and_key(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    index: &Expr,
) -> (LoweredValue, Option<crate::ir::LocalSlotId>, LoweredValue) {
    if matches!(object.kind, ExprKind::Variable(_) | ExprKind::This) {
        let index_value = lower_expr(ctx, index);
        let object_value = lower_expr(ctx, object);
        return (object_value, None, index_value);
    }
    let object_value = lower_expr(ctx, object);
    let (object_value, object_owner) = if ctx.value_is_owning_temporary(object_value) {
        root_owned_call_operand(ctx, object_value, object.span)
    } else {
        (object_value, None)
    };
    let index_value = lower_expr(ctx, index);
    (object_value, object_owner, index_value)
}

/// Lowers `unset(Class::$prop[$key])` for a declared PHP `array` static property, rooting an
/// owning key temporary across the removal like the other element paths do.
pub(super) fn lower_unset_static_property_array_element(
    ctx: &mut LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    property: &str,
    index: &Expr,
    expr: &Expr,
) {
    let index_value = lower_expr(ctx, index);
    let (index_value, key_owner) = root_owned_call_operand(ctx, index_value, index.span);
    if crate::ir_lower::property_default_errors::for_receiver(ctx, receiver, expr.span).is_some() {
        return;
    }
    let data = ctx.intern_string(&format!("{}::{}", receiver_name(receiver), property));
    let initialized = ctx.emit_value(
        Op::StaticPropInitialized,
        Vec::new(),
        Some(Immediate::Data(data)),
        PhpType::Bool,
        Op::StaticPropInitialized.default_effects(),
        Some(expr.span),
    );
    let remove = ctx.builder.create_named_block("unset.static_property.remove", Vec::new());
    let done = ctx.builder.create_named_block("unset.static_property.done", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: initialized.value,
        then_target: remove,
        then_args: Vec::new(),
        else_target: done,
        else_args: Vec::new(),
    });
    ctx.builder.position_at_end(remove);
    crate::ir_lower::stmt::lower_static_property_array_unset(
        ctx,
        receiver,
        property,
        index_value,
        expr.span,
    );
    if !ctx.builder.insertion_block_is_terminated() {
        ctx.builder.terminate(Terminator::Br { target: done, args: Vec::new() });
    }
    ctx.builder.position_at_end(done);
    if let Some(slot) = key_owner {
        retire_owned_call_operand(ctx, slot, expr.span);
    }
}

/// Lowers `unset($object->prop[$key])` when the property holds an `ArrayAccess` object.
///
/// PHP delays the property fetch of an unset target until its dimension has been evaluated: the
/// receiver runs first, then the key, then `$object->prop` is read and its `offsetUnset($key)`
/// called. A key expression that stores another object into the property therefore reaches the
/// NEW object. A plain `$object->prop->offsetUnset($key)` call would read the property before
/// the argument, so the receiver and the key are evaluated here and parked in hidden owner slots,
/// and the call is built over those slots. A variable, `$this` or a literal is read where it
/// stands, which is what PHP does with a compiled variable too. A typed property that was never
/// initialized (or was unset) is left alone, as PHP's quiet unset fetch does, instead of reaching
/// the read's uninitialized-property `Error`.
pub(super) fn lower_unset_array_access_property_offset(
    ctx: &mut LoweringContext<'_, '_>,
    array: &Expr,
    object: &Expr,
    property: &str,
    index: &Expr,
    expr: &Expr,
) {
    let typed = unset_receiver_property_is_typed(ctx, object, property);
    let mut owners = Vec::new();
    let receiver = park_unset_operand(ctx, object, &mut owners);
    let key = park_unset_operand(ctx, index, &mut owners);
    let join_block = if typed {
        let object_value = lower_expr(ctx, &receiver);
        let data = ctx.intern_string(property);
        let initialized = ctx.emit_value(
            Op::PropInitialized,
            vec![object_value.value],
            Some(Immediate::Data(data)),
            PhpType::Bool,
            Op::PropInitialized.default_effects(),
            Some(expr.span),
        );
        let call_block = ctx
            .builder
            .create_named_block("unset.array_access_property.call", Vec::new());
        let join_block = ctx
            .builder
            .create_named_block("unset.array_access_property.done", Vec::new());
        ctx.builder.terminate(Terminator::CondBr {
            cond: initialized.value,
            then_target: call_block,
            then_args: Vec::new(),
            else_target: join_block,
            else_args: Vec::new(),
        });
        ctx.builder.position_at_end(call_block);
        Some(join_block)
    } else {
        None
    };
    let fetch = Expr::new(
        ExprKind::PropertyAccess {
            object: Box::new(receiver),
            property: property.to_string(),
        },
        array.span,
    );
    // The fetched object is parked too. An `offsetUnset()` that throws would otherwise strand
    // the reference the property read took, because the unwinder only sees owner slots.
    let fetched = lower_expr(ctx, &fetch);
    let guarded_from = owners.len();
    let container = park_lowered_unset_operand(ctx, fetched, array.span, &mut owners);
    let call = Expr::new(
        ExprKind::MethodCall {
            object: Box::new(container),
            method: "offsetUnset".to_string(),
            args: vec![key],
        },
        expr.span,
    );
    lower_expr(ctx, &call);
    if let Some(join_block) = join_block {
        // The container slot is written only on the initialized path, so it is retired there.
        for slot in owners.split_off(guarded_from).into_iter().rev() {
            retire_owned_call_operand(ctx, slot, expr.span);
        }
        if !ctx.builder.insertion_block_is_terminated() {
            ctx.builder.terminate(Terminator::Br {
                target: join_block,
                args: Vec::new(),
            });
        }
        ctx.builder.position_at_end(join_block);
    }
    for slot in owners.into_iter().rev() {
        retire_owned_call_operand(ctx, slot, expr.span);
    }
}

/// Evaluates one unset operand now and returns an expression that reads the same value later.
///
/// A variable, `$this` and a scalar literal are returned unchanged: reading them later has no
/// side effect and sees what PHP's delayed fetch sees. Anything else is lowered once and parked
/// by [`park_lowered_unset_operand`]. The caller retires every slot pushed onto `owners` after
/// the operation.
fn park_unset_operand(
    ctx: &mut LoweringContext<'_, '_>,
    operand: &Expr,
    owners: &mut Vec<crate::ir::LocalSlotId>,
) -> Expr {
    if matches!(
        operand.kind,
        ExprKind::Variable(_)
            | ExprKind::This
            | ExprKind::StringLiteral(_)
            | ExprKind::IntLiteral(_)
            | ExprKind::FloatLiteral(_)
            | ExprKind::BoolLiteral(_)
            | ExprKind::Null
    ) {
        return operand.clone();
    }
    let value = lower_expr(ctx, operand);
    park_lowered_unset_operand(ctx, value, operand.span, owners)
}

/// Parks an already lowered unset operand in a hidden owner slot and returns a read of it.
///
/// The value is retained into a hidden temporary registered as a call-operand owner, so a
/// throwing `offsetUnset()` releases it on unwind, and an owning source temporary is released
/// right away. The slot is pushed onto `owners` for the caller to retire.
fn park_lowered_unset_operand(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    span: Span,
    owners: &mut Vec<crate::ir::LocalSlotId>,
) -> Expr {
    let ty = ctx.builder.value_php_type(value.value);
    let name = ctx.declare_hidden_temp(ty.clone());
    let retained = crate::ir_lower::ownership::acquire_if_refcounted(ctx, value, Some(span));
    ctx.store_local(&name, retained, ty, Some(span));
    if ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
    let slot = ctx.local_slots[&name];
    register_owned_call_operand(ctx, slot, span);
    owners.push(slot);
    Expr::new(ExprKind::Variable(name), span)
}

/// Returns PHP's `Error` message when `unset($object->prop[$key])` targets a readonly property.
///
/// PHP refuses every indirect modification of an initialized readonly property, from any scope
/// and for a missing key too, naming the DECLARING class. The receiver must resolve to a known
/// class; an unknown receiver keeps the ordinary lowering.
pub(super) fn readonly_property_element_unset_message(
    ctx: &LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
) -> Option<String> {
    let (class_name, _) = instance_callable_object_class_and_nullability(ctx, object)?;
    let class_info = ctx.classes.get(class_name.as_str())?;
    if !class_info.readonly_properties.contains(property) {
        return None;
    }
    let declaring = class_info
        .property_declaring_classes
        .get(property)
        .cloned()
        .unwrap_or(class_name);
    Some(format!(
        "Cannot indirectly modify readonly property {}::${}",
        declaring.trim_start_matches('\\'),
        property
    ))
}

/// Lowers `unset($object->prop[$key])` for a readonly array property.
///
/// PHP evaluates the receiver and the key, then refuses the write fetch: an initialized readonly
/// property raises `Error("Cannot indirectly modify readonly property C::$p")`, and an
/// uninitialized one is left alone, so `PropInitialized` picks the branch. Both operands are
/// rooted across the throw, which releases them through the owner records, and retired on the
/// quiet path.
pub(super) fn lower_unset_readonly_property_element(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
    message: &str,
    index: &Expr,
    expr: &Expr,
) {
    let (object_value, object_owner, index_value) = lower_unset_receiver_and_key(ctx, object, index);
    let (index_value, key_owner) = root_owned_call_operand(ctx, index_value, index.span);
    let data = ctx.intern_string(property);
    let initialized = ctx.emit_value(
        Op::PropInitialized,
        vec![object_value.value],
        Some(Immediate::Data(data)),
        PhpType::Bool,
        Op::PropInitialized.default_effects(),
        Some(expr.span),
    );
    let refuse_block = ctx
        .builder
        .create_named_block("unset.readonly.refuse", Vec::new());
    let quiet_block = ctx
        .builder
        .create_named_block("unset.readonly.uninitialized", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: initialized.value,
        then_target: refuse_block,
        then_args: Vec::new(),
        else_target: quiet_block,
        else_args: Vec::new(),
    });
    ctx.builder.position_at_end(refuse_block);
    let property_ty = unset_receiver_declared_property_type(ctx, object, property);
    if property_ty.is_some_and(|ty| !ty.is_php_array() && ty.codegen_repr() == PhpType::Mixed) {
        // Readonly permits mutation of an object payload. Inspect the initialized value and
        // keep its snapshot alive across offsetUnset without fetching the property for write.
        let value = ctx.emit_value(
            Op::PropGet,
            vec![object_value.value],
            Some(Immediate::Data(data)),
            PhpType::Mixed,
            Op::PropGet.default_effects(),
            Some(expr.span),
        );
        let (value, owner) = root_owned_call_operand(ctx, value, expr.span);
        let is_object = ctx.emit_value(
            Op::TypePredicate,
            vec![value.value],
            Some(Immediate::TypePredicate(crate::ir::PhpTypePredicate::Object)),
            PhpType::Bool,
            Op::TypePredicate.default_effects(),
            Some(expr.span),
        );
        let remove = ctx.builder.create_named_block("unset.readonly.object", Vec::new());
        let error = ctx.builder.create_named_block("unset.readonly.value", Vec::new());
        ctx.builder.terminate(Terminator::CondBr {
            cond: is_object.value,
            then_target: remove,
            then_args: Vec::new(),
            else_target: error,
            else_args: Vec::new(),
        });
        ctx.builder.position_at_end(remove);
        ctx.emit_void(
            Op::OffsetUnset,
            vec![value.value, index_value.value],
            None,
            Op::OffsetUnset.default_effects(),
            Some(expr.span),
        );
        if let Some(owner) = owner {
            retire_owned_call_operand(ctx, owner, expr.span);
        }
        ctx.builder.terminate(Terminator::Br { target: quiet_block, args: Vec::new() });
        ctx.builder.position_at_end(error);
    }
    crate::ir_lower::stmt::lower_throw_access_error(ctx, message, expr.span);
    ctx.builder.position_at_end(quiet_block);
    if let Some(slot) = key_owner {
        retire_owned_call_operand(ctx, slot, expr.span);
    }
    if let Some(slot) = object_owner {
        retire_owned_call_operand(ctx, slot, expr.span);
    }
}
