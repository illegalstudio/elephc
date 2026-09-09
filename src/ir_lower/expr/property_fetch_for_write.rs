//! Purpose:
//! Lowers stable object-property sources for by-reference iteration and nested writes.
//!
//! Called from:
//! - `crate::ir_lower::stmt::typed_foreach` when the loop source is a property access.
//! - `crate::ir_lower::stmt::nested_array_writes` for boxed property write roots.
//!
//! Key details:
//! - Emits a borrowed `PropGetForWrite` only when frontend and backend slot
//!   classification agree and every receiver-chain step has stable backing storage.
//! - Hooked, magic, dynamic, nullable, packed, or temporary receiver paths keep
//!   the ordinary retaining property read.

use super::*;

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
) -> LoweredValue {
    lower_property_write_source(ctx, object, property, expr, false)
}

/// Separates and republishes a fixed Mixed property before mutating its nested array payload.
pub(crate) fn lower_nested_assignment_property_source(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
    expr: &Expr,
) -> LoweredValue {
    lower_property_write_source(ctx, object, property, expr, true)
}

/// Selects borrowed slot access only for the stable storage shape required by its consumer.
fn lower_property_write_source(
    ctx: &mut LoweringContext<'_, '_>,
    object: &Expr,
    property: &str,
    expr: &Expr,
    mixed_root: bool,
) -> LoweredValue {
    let object_value = lower_expr(ctx, object);
    if !property_fetch_for_write_applies(ctx, &object_value, property, expr, mixed_root) {
        return lower_property_get_from_value(ctx, object_value, property, Op::PropGet, expr);
    }
    let data = ctx.intern_string(property);
    let result_type = property_get_result_type(ctx, object_value.value, property, Op::PropGet, expr);
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
    // The stable base local keeps the receiver and its slot alive through the consumer.
    // Unstable chains were rejected above and used the ordinary retaining read instead.
    if ctx.value_is_owning_temporary(object_value) {
        crate::ir_lower::ownership::release_if_owned(ctx, object_value, Some(expr.span));
    }
    result
}

/// Returns whether the consumer can mutate a stable property through a fetch-for-write read.
///
/// The receiver must be a statically known non-null object, the final property must be a fixed
/// container slot the backend can split, and every receiver-chain step must have stable backing
/// storage.
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
        property_get_result_type(ctx, object_value.value, property, Op::PropGet, expr);
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
    let ExprKind::PropertyAccess { object, .. } = &expr.kind else {
        return false;
    };
    receiver_is_stable_backing_storage(ctx, object)
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
