//! Purpose:
//! ReflectionProperty value access and reflected-target extraction.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Lowers `ReflectionProperty::getValue($object)` when the reflected property is known.
pub(super) fn lower_reflection_property_value_call(
    ctx: &mut LoweringContext<'_, '_>,
    object_expr: Option<&Expr>,
    method: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let object_expr = object_expr?;
    match php_symbol_key(method).as_str() {
        "getvalue" => {
            if let Some((declaring_class, property, property_ty)) =
                reflection_property_static_target(ctx, object_expr)
            {
                return lower_reflection_property_get_static_value(
                    ctx,
                    &declaring_class,
                    &property,
                    property_ty,
                    args,
                    expr,
                );
            }
            let (declaring_class, property, property_ty) =
                reflection_property_any_instance_target(ctx, object_expr)?;
            lower_reflection_property_get_value(
                ctx,
                &declaring_class,
                &property,
                property_ty,
                args,
                expr,
            )
        }
        "setvalue" => {
            if let Some((declaring_class, property, _)) =
                reflection_property_static_target(ctx, object_expr)
            {
                return lower_reflection_property_set_static_value(
                    ctx,
                    &declaring_class,
                    &property,
                    args,
                    expr,
                );
            }
            let (declaring_class, property, property_ty) =
                reflection_property_any_instance_target(ctx, object_expr)?;
            lower_reflection_property_set_value(
                ctx,
                &declaring_class,
                &property,
                property_ty,
                args,
                expr,
            )
        }
        "isinitialized" => {
            if let Some((declaring_class, property, _)) =
                reflection_property_static_target(ctx, object_expr)
            {
                return lower_reflection_property_static_is_initialized(
                    ctx,
                    &declaring_class,
                    &property,
                    args,
                    expr,
                );
            }
            let (_, property, _) = reflection_property_any_instance_target(ctx, object_expr)?;
            lower_reflection_property_is_initialized(ctx, &property, args, expr)
        }
        _ => None,
    }
}

/// Lowers `ReflectionProperty::getValue($object)` to a direct property read.
pub(super) fn lower_reflection_property_get_value(
    ctx: &mut LoweringContext<'_, '_>,
    declaring_class: &str,
    property: &str,
    property_ty: PhpType,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let object_arg = reflection_property_get_value_arg(args)?;
    if !reflection_property_physical_receiver_is_compatible(ctx, &object_arg, declaring_class) {
        return None;
    }
    let object = lower_expr(ctx, &object_arg);
    let slot = reflection_property_physical_ref(ctx, declaring_class, property)?;
    let result = ctx.emit_value(
        Op::PropGet,
        vec![object.value],
        Some(slot),
        property_ty,
        Op::PropGet.default_effects(),
        Some(expr.span),
    );
    Some(stabilize_borrowed_result_and_release_receiver(
        ctx,
        object,
        result,
        expr.span,
    ))
}

/// Lowers `ReflectionProperty::setValue($object, $value)` to a direct property write.
pub(super) fn lower_reflection_property_set_value(
    ctx: &mut LoweringContext<'_, '_>,
    declaring_class: &str,
    property: &str,
    property_ty: PhpType,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let (object_arg, value_arg) = reflection_property_set_value_args(args)?;
    if !reflection_property_physical_receiver_is_compatible(ctx, &object_arg, declaring_class) {
        return None;
    }
    let object = lower_expr(ctx, &object_arg);
    let value = lower_expr(ctx, &value_arg);
    let value = crate::ir_lower::stmt::contextualize_property_array_value(
        ctx,
        value,
        &value_arg,
        &property_ty,
        expr.span,
    );
    let value = crate::ir_lower::stmt::coerce_typed_assign_value(
        ctx,
        value,
        &property_ty,
        expr.span,
    );
    let slot = reflection_property_physical_ref(ctx, declaring_class, property)?;
    let pins = pin_in_flight_owners(ctx, &[object.value, value.value], expr.span);
    ctx.emit_void(
        Op::PropSet,
        vec![object.value, value.value],
        Some(slot),
        Op::PropSet.default_effects(),
        Some(expr.span),
    );
    unpin_in_flight_owners(ctx, pins, expr.span);
    crate::ir_lower::stmt::release_property_assignment_source_after_retaining_store(
        ctx,
        &property_ty,
        value,
        expr.span,
    );
    Some(lower_null(ctx, expr))
}

/// Identifies a reflected instance property by its physical layout slot.
///
/// Only statically resolved ReflectionProperty calls emit this immediate. Ordinary source
/// property access continues to carry a name and therefore remains subject to scope checks.
fn reflection_property_physical_ref(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    property: &str,
) -> Option<Immediate> {
    let info = ctx.classes.get(class_name.trim_start_matches('\\'))?;
    let index = info.visible_property_index(property)?;
    Some(Immediate::ReflectionPropertyRef {
        class: u32::try_from(info.class_id).ok()?,
        property: u32::try_from(index).ok()?,
    })
}

/// Returns whether an instance-property reflection call can safely name one physical slot.
///
/// Runtime-shaped and incompatible receivers must stay on ReflectionProperty's ordinary method
/// path, which performs the existing runtime object checks and name dispatch. The physical
/// immediate is reserved for a concrete receiver whose layout is proven to contain the reflected
/// class's prefix.
fn reflection_property_physical_receiver_is_compatible(
    ctx: &LoweringContext<'_, '_>,
    object: &Expr,
    declaring_class: &str,
) -> bool {
    let object_ty = match &object.kind {
        ExprKind::Variable(name) => ctx.local_type(name),
        _ => infer_expr_type_syntactic(object),
    };
    let PhpType::Object(receiver_class) = object_ty.codegen_repr() else {
        return false;
    };
    class_extends_class(ctx, &receiver_class, declaring_class)
}

/// Lowers `ReflectionProperty::isInitialized($object)` to a direct slot probe.
pub(super) fn lower_reflection_property_is_initialized(
    ctx: &mut LoweringContext<'_, '_>,
    property: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let object_arg = reflection_property_get_value_arg(args)?;
    let object = lower_expr(ctx, &object_arg);
    let data = ctx.intern_string(property);
    Some(ctx.emit_value(
        Op::PropInitialized,
        vec![object.value],
        Some(Immediate::Data(data)),
        PhpType::Bool,
        Op::PropInitialized.default_effects(),
        Some(expr.span),
    ))
}

/// Lowers static `ReflectionProperty::getValue()` to a reflection static-property read.
pub(super) fn lower_reflection_property_get_static_value(
    ctx: &mut LoweringContext<'_, '_>,
    declaring_class: &str,
    property: &str,
    property_ty: PhpType,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if let Some(ignored_object) = reflection_property_static_get_value_ignored_arg(args)? {
        lower_ignored_reflection_argument(ctx, &ignored_object);
    }
    Some(lower_reflection_static_property_get_by_class_name(
        ctx,
        declaring_class,
        property,
        property_ty,
        expr,
    ))
}

/// Lowers static `ReflectionProperty::isInitialized()` to a direct static-slot probe.
pub(super) fn lower_reflection_property_static_is_initialized(
    ctx: &mut LoweringContext<'_, '_>,
    declaring_class: &str,
    property: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if let Some(ignored_object) = reflection_property_static_get_value_ignored_arg(args)? {
        lower_ignored_reflection_argument(ctx, &ignored_object);
    }
    Some(lower_reflection_static_property_initialized_by_class_name(
        ctx,
        declaring_class,
        property,
        expr,
    ))
}

/// Lowers static `ReflectionProperty::setValue(null, $value)` to a reflection static-property write.
pub(super) fn lower_reflection_property_set_static_value(
    ctx: &mut LoweringContext<'_, '_>,
    declaring_class: &str,
    property: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let (ignored_object, value_arg) = reflection_property_static_set_value_args(args)?;
    lower_ignored_reflection_argument(ctx, &ignored_object);
    let value = lower_expr(ctx, &value_arg);
    store_reflection_static_property_by_class_name(
        ctx,
        declaring_class,
        property,
        value.value,
        expr.span,
    );
    Some(lower_null(ctx, expr))
}

/// Evaluates an ignored Reflection argument and releases temporary objects.
pub(super) fn lower_ignored_reflection_argument(ctx: &mut LoweringContext<'_, '_>, arg: &Expr) {
    let value = lower_expr(ctx, arg);
    if ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(arg.span));
    }
}

/// Returns the explicit object argument passed to `ReflectionProperty::getValue()`.
pub(super) fn reflection_property_get_value_arg(args: &[Expr]) -> Option<Expr> {
    let args = reflection_class_new_instance_args(args);
    if args.iter().any(is_spread_arg) {
        return None;
    }
    let object = if !crate::types::call_args::has_named_args(&args) {
        match args.as_slice() {
            [object] => object.clone(),
            _ => return None,
        }
    } else {
        reflection_property_named_object_arg(&args)?
    };
    (!matches!(&object.kind, ExprKind::Null)).then_some(object)
}

/// Returns the explicit object and value arguments passed to `ReflectionProperty::setValue()`.
pub(super) fn reflection_property_set_value_args(args: &[Expr]) -> Option<(Expr, Expr)> {
    let args = reflection_class_new_instance_args(args);
    if args.iter().any(is_spread_arg) {
        return None;
    }
    let (object, value) =
        reflection_class_static_property_regular_args(&args, "object", Some("value"))?;
    let object = object?;
    if matches!(&object.kind, ExprKind::Null) {
        return None;
    }
    Some((object, value?))
}

/// Returns the optional ignored object argument for static `ReflectionProperty::getValue()`.
pub(super) fn reflection_property_static_get_value_ignored_arg(args: &[Expr]) -> Option<Option<Expr>> {
    let args = reflection_class_new_instance_args(args);
    if args.iter().any(is_spread_arg) {
        return None;
    }
    if !crate::types::call_args::has_named_args(&args) {
        return match args.as_slice() {
            [] => Some(None),
            [object] => Some(Some(object.clone())),
            _ => None,
        };
    }
    reflection_property_named_optional_object_arg(&args)
}

/// Returns the ignored object and value arguments for static `ReflectionProperty::setValue()`.
pub(super) fn reflection_property_static_set_value_args(args: &[Expr]) -> Option<(Expr, Expr)> {
    let args = reflection_class_new_instance_args(args);
    if args.iter().any(is_spread_arg) {
        return None;
    }
    let (object, value) =
        reflection_class_static_property_regular_args(&args, "object", Some("value"))?;
    Some((object?, value?))
}

/// Returns a required named `object` argument for ReflectionProperty value access.
pub(super) fn reflection_property_named_object_arg(args: &[Expr]) -> Option<Expr> {
    reflection_property_named_optional_object_arg(args)?
}

/// Returns an optional named `object` argument for ReflectionProperty value access.
pub(super) fn reflection_property_named_optional_object_arg(args: &[Expr]) -> Option<Option<Expr>> {
    let mut object = None;
    for arg in args {
        match &arg.kind {
            ExprKind::NamedArg { name, value } if php_symbol_key(name) == "object" => {
                object = Some((**value).clone());
            }
            _ => return None,
        }
    }
    Some(object)
}

/// Resolves a known non-static ReflectionProperty target without enforcing visibility.
pub(super) fn reflection_property_any_instance_target(
    ctx: &LoweringContext<'_, '_>,
    object_expr: &Expr,
) -> Option<(String, String, PhpType)> {
    let (class_name, property) = reflection_property_reflected_target(ctx, object_expr)?;
    let class_info = ctx.classes.get(class_name.trim_start_matches('\\'))?;
    if class_info
        .static_properties
        .iter()
        .any(|(name, _)| name == &property)
    {
        return None;
    }
    let (_, (_, property_ty)) = class_info.visible_property(&property)?;
    Some((
        class_name,
        property,
        normalize_value_php_type(property_ty.codegen_repr()),
    ))
}

/// Resolves an inline `ReflectionProperty` target for a static property.
pub(super) fn reflection_property_static_target(
    ctx: &LoweringContext<'_, '_>,
    object_expr: &Expr,
) -> Option<(String, String, PhpType)> {
    let (class_name, property) = reflection_property_reflected_target(ctx, object_expr)?;
    let (declaring_class, property_ty) =
        reflection_class_static_property_target(ctx, &class_name, &property)?;
    Some((declaring_class, property, property_ty))
}

/// Extracts the known class and property name from a supported ReflectionProperty source.
pub(super) fn reflection_property_reflected_target(
    ctx: &LoweringContext<'_, '_>,
    object_expr: &Expr,
) -> Option<(String, String)> {
    reflection_property_constructor_target(ctx, object_expr)
        .or_else(|| reflection_property_class_get_property_target(ctx, object_expr))
        .or_else(|| reflection_property_class_get_properties_index_target(ctx, object_expr))
        .or_else(|| {
            let ExprKind::Variable(name) = &object_expr.kind else {
                return None;
            };
            ctx.reflection_property_local(name)
        })
}

/// Extracts the known class and method name from a supported ReflectionMethod source.
pub(super) fn reflection_method_reflected_target(
    ctx: &LoweringContext<'_, '_>,
    object_expr: &Expr,
) -> Option<(String, String)> {
    reflection_method_constructor_target(ctx, object_expr)
        .or_else(|| reflection_method_class_get_constructor_target(ctx, object_expr))
        .or_else(|| reflection_method_class_get_method_target(ctx, object_expr))
        .or_else(|| reflection_method_class_get_methods_index_target(ctx, object_expr))
        .or_else(|| {
            let ExprKind::Variable(name) = &object_expr.kind else {
                return None;
            };
            ctx.reflection_method_local(name)
        })
}

/// Extracts the known function name from a supported ReflectionFunction source.
pub(super) fn reflection_function_reflected_target(
    ctx: &LoweringContext<'_, '_>,
    object_expr: &Expr,
) -> Option<String> {
    reflection_function_constructor_target(ctx, object_expr).or_else(|| {
        let ExprKind::Variable(name) = &object_expr.kind else {
            return None;
        };
        ctx.reflection_function_local(name)
    })
}
