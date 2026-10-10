//! Purpose:
//! Lowers object-property sources for by-reference `foreach` and nested writes, whatever the receiver.
//!
//! Called from:
//! - `crate::ir_lower::stmt::typed_foreach` when the loop source is a property access.
//! - `crate::ir_lower::stmt::nested_array_writes` for boxed property write roots.
//!
//! Key details:
//! - Emits a borrowed `PropGetForWrite` when the receiver is a statically known non-null object
//!   and the final property is a fixed container slot. Frontend and backend slot classification
//!   are mirrored over the same class metadata so the two sides cannot silently disagree.
//! - For a by-reference `foreach` the receiver no longer has to name stable backing storage.
//!   That question now only decides WHO releases the receiver: a stable root (a variable,
//!   `$this`, a chain of declared non-null object slots) releases its temporary here as before,
//!   while an OWNING temporary -- `$arr[0]->x`, `$o->get()->x` -- is handed to the loop, which
//!   outlives the borrow and releases it on every way out, including `break`, `return` and a
//!   caught throw (issue #690). A nested write has no loop to hand the receiver to, so every
//!   step of its receiver chain must still be stable backing storage.
//! - Hooked, magic, dynamic, nullable, `stdClass`, and non-container property slots keep the
//!   ordinary retaining property read.

use super::*;

/// Recognizes pure static-property receiver chains whose traversal follows the RHS of a write.
pub(crate) fn is_static_property_write_chain(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::StaticPropertyAccess { .. } => true,
        ExprKind::PropertyAccess { object, .. } => is_static_property_write_chain(object),
        _ => false,
    }
}

/// Guards intermediate write-context properties before reading them, once per chain step.
pub(crate) fn lower_static_property_write_chain(
    ctx: &mut LoweringContext<'_, '_>, expr: &Expr, rhs: LoweredValue, span: Span,
) -> crate::ir_lower::stmt::property_write_receiver::PropertyWriteReceiver {
    use crate::ir_lower::stmt::property_write_receiver::PropertyWriteReceiver;
    let ExprKind::PropertyAccess { object, property } = &expr.kind else {
        let value = lower_expr(ctx, expr);
        return PropertyWriteReceiver::new(ctx, value, span);
    };
    let mut parent = lower_static_property_write_chain(ctx, object, rhs, span);
    parent.narrow_for_array(ctx, property, &[rhs.value], span);
    // The parent lease owns cleanup. Pass a borrowed view to the ordinary getter so it
    // cannot retire that same owner a second time, including when a hook performs the read.
    let ty = ctx.builder.value_php_type(parent.value.value);
    let borrowed = ctx.emit_value(Op::Borrow, vec![parent.value.value], None, ty,
        Op::Borrow.default_effects(), Some(span));
    ctx.builder.set_value_ownership(borrowed.value, Ownership::Borrowed);
    let child = lower_property_get_from_value(ctx, borrowed, property, Op::PropGet, expr);
    let child = if ctx.value_is_owning_temporary(child) { child } else {
        crate::ir_lower::ownership::acquire_lifetime_pin_if_refcounted(ctx, child, Some(span))
    };
    parent.finish(ctx, span);
    PropertyWriteReceiver::new(ctx, child, span)
}

/// Lowers the source of a by-reference `foreach` whose receiver is an object property.
///
/// An ordinary property read acquires the container, so `IterStart` sees a shared source and
/// copy-on-writes. The split consumes one reference and the loop-exit release consumes another,
/// potentially freeing storage still named by the property. This path instead splits first,
/// publishes the unique container back into the property slot, and returns it borrowed.
pub(crate) fn lower_by_ref_foreach_property_source(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
    expr: &Expr,
) -> ByRefForeachPropertySource {
    lower_property_write_source(ctx, object, property, expr, false)
}

/// Returns whether a by-reference foreach may bind a synthetic origin to this property slot.
///
/// The origin must name the same fixed backing storage that an ordinary fetch-for-write would
/// mutate. Hooked, magic, inaccessible, dynamic, nullable, and temporary receiver shapes must
/// keep the value-fetch fallback so lowering does not bypass user code or retain an interior slot
/// after its receiver dies.
pub(crate) fn by_ref_foreach_property_source_is_addressable(
    ctx: &LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
) -> bool {
    let Some((class_name, nullable)) = instance_callable_object_class_and_nullability(ctx, object)
    else {
        return false;
    };
    if nullable
        || !matches!(
            crate::types::resolve_property_name(
                ctx.classes,
                class_name.trim_start_matches('\\'),
                property,
                ctx.current_class.as_deref(),
            ),
            crate::types::PropertyNameResolution::Visible
                | crate::types::PropertyNameResolution::ScopePrivate { .. }
        )
    {
        return false;
    }
    property_is_splittable_container_slot(ctx, &class_name, property, false)
        && receiver_is_stable_backing_storage(ctx, object)
}

/// Separates and republishes a fixed Mixed property before mutating its nested array payload.
///
/// A nested write has no loop frame to hand an unstable receiver to, so
/// `property_fetch_for_write_applies` only admits receiver chains that keep the slot alive
/// themselves and the read never yields a receiver to pin.
pub(crate) fn lower_nested_assignment_property_source(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
    expr: &Expr,
) -> LoweredValue {
    lower_property_write_source(ctx, object, property, expr, true).value
}

/// Selects borrowed slot access only for the stable storage shape required by its consumer.
fn lower_property_write_source(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
    expr: &Expr,
    mixed_root: bool,
) -> ByRefForeachPropertySource {
    let object_value = lower_expr(ctx, object);
    if !property_fetch_for_write_applies(ctx, &object_value, property, expr, mixed_root) {
        return ByRefForeachPropertySource {
            value: lower_property_get_from_value(ctx, object_value, property, Op::PropGet, expr),
            receiver: None,
        };
    }
    let data = ctx.intern_string(property);
    let result_type =
        property_slot_result_type(ctx, object_value.value, property, Op::PropGet, expr);
    let result = ctx.emit_value(
        Op::PropGetForWrite,
        vec![object_value.value],
        Some(Immediate::Data(data)),
        result_type,
        Op::PropGetForWrite.default_effects(),
        Some(expr.span),
    );
    // The separated value belongs to the property slot. Keep it borrowed so temporary cleanup
    // cannot release the property's owner after iteration or a nested assignment.
    ctx.builder
        .set_value_ownership(result.value, Ownership::Borrowed);
    // A receiver that names stable backing storage -- a variable, `$this`, a declared object
    // slot chain -- keeps itself and its property slot alive for the loop, so the temporary (if
    // any) is released here as before. A receiver that does NOT is a temporary the loop is
    // borrowing THROUGH: `$arr[0]->x`, `$o->get()->x`. Releasing it here frees the object whose
    // slot owns the container the iterator walks, so it is handed to the loop instead and
    // released on every way out (issue #690).
    if !ctx.value_is_owning_temporary(object_value) {
        return ByRefForeachPropertySource {
            value: result,
            receiver: None,
        };
    }
    if receiver_is_stable_backing_storage(ctx, object) {
        crate::ir_lower::ownership::release_if_owned(ctx, object_value, Some(expr.span));
        return ByRefForeachPropertySource {
            value: result,
            receiver: None,
        };
    }
    ByRefForeachPropertySource {
        value: result,
        receiver: Some(object_value),
    }
}

/// A by-reference `foreach` property source, plus the receiver the loop borrows through.
pub(crate) struct ByRefForeachPropertySource {
    /// The container the loop iterates.
    pub(crate) value: LoweredValue,
    /// A receiver temporary the loop must outlive. `Some` only for the borrowed
    /// fetch-for-write read through a receiver that names no stable storage of its own.
    pub(crate) receiver: Option<LoweredValue>,
}

/// Returns whether the consumer can mutate a stable property through a fetch-for-write read.
///
/// The receiver must be a statically known non-null object and the final property must be a
/// fixed container slot the backend can split. A nested write (`mixed_root`) additionally needs
/// every receiver-chain step to have stable backing storage; a by-reference `foreach` pins an
/// unstable receiver for the loop instead.
fn property_fetch_for_write_applies(
    ctx: &LoweringContext<'_, '_>,
    object_value: &LoweredValue,
    property: &str,
    expr: &Expr,
    mixed_root: bool,
) -> bool {
    let PhpType::Object(class_name) = ctx.builder.value_php_type(object_value.value).codegen_repr()
    else {
        return false;
    };
    if value_is_nullable(ctx, object_value.value) {
        return false;
    }
    let property_ty =
        property_slot_result_type(ctx, object_value.value, property, Op::PropGet, expr);
    let property_ty = normalize_value_php_type(property_ty);
    let supported = if mixed_root {
        property_ty.codegen_repr() == PhpType::Mixed
    } else {
        property_ty.is_php_array()
            || matches!(property_ty.codegen_repr(), PhpType::Array(_) | PhpType::AssocArray { .. })
    };
    if !supported {
        return false;
    }
    // The backend has no plain-read fallback for this borrowed op. Mirror its slot
    // classification over the same class metadata so the two sides cannot silently disagree.
    if !property_is_splittable_container_slot(ctx, &class_name, property, mixed_root) {
        return false;
    }
    if mixed_root {
        // A nested write has no loop frame to hand an unstable receiver to: the receiver chain
        // itself has to keep the slot alive across the write, or the ordinary retaining read
        // is the only safe source.
        let ExprKind::PropertyAccess { object, .. } = &expr.kind else {
            return false;
        };
        return receiver_is_stable_backing_storage(ctx, object);
    }
    // A by-reference `foreach` receiver no longer has to name stable storage: an unstable one
    // is held by the loop instead (see `lower_by_ref_foreach_property_source`). What still has
    // to hold is that the receiver is a real object value, which the checks above established.
    //
    // One receiver shape keeps the ordinary retaining read: a PROPERTY chain that is not stable
    // backing storage, i.e. one reached through a magic, hooked, or nullable accessor
    // (`$o->inner->x` with `__get`). Its object arrives narrowed out of the accessor's boxed
    // result, and handing that object to the loop leaves the box behind (regression #642);
    // the element and call receivers of issue #690 do not go through such a box.
    if let ExprKind::PropertyAccess { object, .. } = &expr.kind {
        if matches!(object.kind, ExprKind::PropertyAccess { .. })
            && !receiver_is_stable_backing_storage(ctx, object)
        {
            return false;
        }
    }
    true
}

/// Returns whether a class property is a fixed container slot the backend can split in place.
///
/// This mirrors `resolve_property_slot_for_class` and `property_container_split`, including the
/// SPL storage-type override. Hooked and undeclared properties have no directly addressable slot.
fn property_is_splittable_container_slot(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    property: &str,
    mixed_root: bool,
) -> bool {
    let normalized = class_name.trim_start_matches('\\');
    if is_builtin_stdclass_name(normalized) {
        return false;
    }
    let Some(class_info) = ctx.classes.get(normalized) else {
        return false;
    };
    if class_info
        .methods
        .contains_key(&php_symbol_key(&property_hook_get_method(property)))
    {
        return false;
    }
    let Some((_, (_, declared_ty))) = class_info.visible_property(property) else {
        return false;
    };
    let slot_ty = runtime_property_type_override(ctx, normalized, property)
        .unwrap_or_else(|| declared_ty.clone());
    if mixed_root {
        slot_ty.codegen_repr() == PhpType::Mixed
    } else {
        slot_ty.is_php_array()
            || matches!(slot_ty.codegen_repr(), PhpType::Array(_) | PhpType::AssocArray { .. })
    }
}

/// Returns whether an object expression names storage that keeps the receiver alive for mutation.
///
/// Variables and `$this` are stable roots. A property chain remains stable only through declared,
/// non-null, non-hooked object slots; calls, constructors, magic access, and other temporaries fail.
fn receiver_is_stable_backing_storage(ctx: &LoweringContext<'_, '_>, expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Variable(_) | ExprKind::This => true,
        ExprKind::PropertyAccess { object, property } => {
            let Some((class_name, nullable)) =
                instance_callable_object_class_and_nullability(ctx, object)
            else {
                return false;
            };
            if nullable {
                return false;
            }
            property_is_stable_object_backing_slot(ctx, &class_name, property)
                && receiver_is_stable_backing_storage(ctx, object)
        }
        _ => false,
    }
}

/// Returns whether an intermediate chain step reads an object from a plain declared slot.
fn property_is_stable_object_backing_slot(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    property: &str,
) -> bool {
    let normalized = class_name.trim_start_matches('\\');
    if is_builtin_stdclass_name(normalized) {
        return false;
    }
    let Some(class_info) = ctx.classes.get(normalized) else {
        return false;
    };
    if class_info
        .methods
        .contains_key(&php_symbol_key(&property_hook_get_method(property)))
    {
        return false;
    }
    class_info
        .visible_property(property)
        .is_some_and(|(_, (_, slot_ty))| matches!(slot_ty.codegen_repr(), PhpType::Object(_)))
}
