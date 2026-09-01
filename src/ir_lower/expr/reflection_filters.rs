//! Purpose:
//! Reflection member list targets, filters, and modifier computation.
//!
//! Called from:
//! - `crate::ir_lower::expr`.
//!
//! Key details:
//! - Preserves source-order evaluation, EIR typing, effects, and ownership contracts.

use super::*;

/// Extracts a known ReflectionMethod from `ReflectionClass::getMethods()[N]`.
pub(super) fn reflection_method_class_get_methods_index_target(
    ctx: &LoweringContext<'_, '_>,
    object_expr: &Expr,
) -> Option<(String, String)> {
    let ExprKind::ArrayAccess { array, index } = &object_expr.kind else {
        return None;
    };
    let ExprKind::IntLiteral(raw_index) = &index.kind else {
        return None;
    };
    if *raw_index < 0 {
        return None;
    }
    let ExprKind::MethodCall {
        object,
        method,
        args,
    } = &array.kind
    else {
        return None;
    };
    if php_symbol_key(method) != "getmethods" {
        return None;
    }
    let filter = reflection_class_get_methods_filter_arg(ctx, args)?;
    let class_name = reflection_class_reflected_class(ctx, object)?;
    let method =
        reflection_class_method_name_at_index(ctx, &class_name, *raw_index as usize, filter)?;
    Some((class_name, method))
}

/// Returns the `ReflectionClass::getMethods()` method name at a known index.
pub(super) fn reflection_class_method_name_at_index(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    index: usize,
    filter: Option<i64>,
) -> Option<String> {
    reflection_class_method_names_for_filter(ctx, class_name, filter)?
        .into_iter()
        .nth(index)
}

/// Extracts a known ReflectionProperty from `ReflectionClass::getProperties()[N]`.
pub(super) fn reflection_property_class_get_properties_index_target(
    ctx: &LoweringContext<'_, '_>,
    object_expr: &Expr,
) -> Option<(String, String)> {
    let ExprKind::ArrayAccess { array, index } = &object_expr.kind else {
        return None;
    };
    let ExprKind::IntLiteral(raw_index) = &index.kind else {
        return None;
    };
    if *raw_index < 0 {
        return None;
    }
    let ExprKind::MethodCall {
        object,
        method,
        args,
    } = &array.kind
    else {
        return None;
    };
    if php_symbol_key(method) != "getproperties" {
        return None;
    }
    if reflection_owner_receiver_is_object(ctx, object) {
        return None;
    }
    let filter = reflection_class_get_properties_filter_arg(ctx, args)?;
    let class_name = reflection_class_reflected_class(ctx, object)?;
    let property =
        reflection_class_property_name_at_index(ctx, &class_name, *raw_index as usize, filter)?;
    Some((class_name, property))
}

/// Returns whether a Reflection owner expression is a `ReflectionObject` rather than a class.
pub(super) fn reflection_owner_receiver_is_object(
    ctx: &LoweringContext<'_, '_>,
    object_expr: &Expr,
) -> bool {
    isset_object_expr_class(ctx, object_expr).is_some_and(|(class_name, _)| {
        php_symbol_key(class_name.trim_start_matches('\\')) == "reflectionobject"
    })
}

/// Returns the `ReflectionClass::getProperties()` property name at a known index.
pub(super) fn reflection_class_property_name_at_index(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    index: usize,
    filter: Option<i64>,
) -> Option<String> {
    reflection_class_property_names_for_filter(ctx, class_name, filter)?
        .into_iter()
        .nth(index)
}

/// Returns `ReflectionClass::getProperties()` names after applying a known filter.
pub(super) fn reflection_class_property_names_for_filter(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    filter: Option<i64>,
) -> Option<Vec<String>> {
    let class_info = ctx.classes.get(class_name.trim_start_matches('\\'))?;
    if let Some(property_names) = crate::types::php_src_date_property_names(class_name) {
        return Some(
            property_names
                .iter()
                .filter(|name| reflection_property_matches_filter(class_info, name, filter))
                .map(|name| (*name).to_string())
                .collect(),
        );
    }
    Some(
        class_info
            .properties
            .iter()
            .chain(class_info.static_properties.iter())
            .map(|(name, _)| name)
            .filter(|name| reflection_property_matches_filter(class_info, name, filter))
            .cloned()
            .collect(),
    )
}

/// Returns `ReflectionClass::getMethods()` names after applying a known filter.
pub(super) fn reflection_class_method_names_for_filter(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    filter: Option<i64>,
) -> Option<Vec<String>> {
    let class_info = ctx.classes.get(class_name.trim_start_matches('\\'))?;
    if let Some(method_names) = crate::types::php_src_date_method_names(class_name) {
        return Some(
            method_names
                .iter()
                .filter(|name| reflection_method_matches_filter(class_info, name, filter))
                .map(|name| (*name).to_string())
                .collect(),
        );
    }
    let mut names = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for name in class_info
        .methods
        .keys()
        .chain(class_info.static_methods.keys())
    {
        if seen.insert(php_symbol_key(name))
            && reflection_method_matches_filter(class_info, name, filter)
        {
            names.push(name.clone());
        }
    }
    Some(names)
}

/// Returns the optional `ReflectionClass::getProperties()` modifier filter.
pub(super) fn reflection_class_get_properties_filter_arg(
    ctx: &LoweringContext<'_, '_>,
    args: &[Expr],
) -> Option<Option<i64>> {
    reflection_class_member_filter_arg(ctx, args, "ReflectionProperty")
}

/// Returns the optional `ReflectionClass::getMethods()` modifier filter.
pub(super) fn reflection_class_get_methods_filter_arg(
    ctx: &LoweringContext<'_, '_>,
    args: &[Expr],
) -> Option<Option<i64>> {
    reflection_class_member_filter_arg(ctx, args, "ReflectionMethod")
}

/// Returns the optional ReflectionClass member-list modifier filter.
pub(super) fn reflection_class_member_filter_arg(
    ctx: &LoweringContext<'_, '_>,
    args: &[Expr],
    constant_class: &str,
) -> Option<Option<i64>> {
    let args = reflection_class_new_instance_args(args);
    if args.iter().any(is_spread_arg) {
        return None;
    }
    if !crate::types::call_args::has_named_args(&args) {
        return match args.as_slice() {
            [] => Some(None),
            [filter] => reflection_member_filter_value(ctx, filter, constant_class),
            _ => None,
        };
    }
    let (filter, _) = reflection_class_static_property_regular_args(&args, "filter", None)?;
    filter
        .as_ref()
        .map(|filter| reflection_member_filter_value(ctx, filter, constant_class))
        .unwrap_or(Some(None))
}

/// Returns a known integer modifier filter expression.
pub(super) fn reflection_member_filter_value(
    ctx: &LoweringContext<'_, '_>,
    expr: &Expr,
    constant_class: &str,
) -> Option<Option<i64>> {
    match &expr.kind {
        ExprKind::Null => Some(None),
        ExprKind::IntLiteral(value) => Some(Some(*value)),
        ExprKind::ScopedConstantAccess { receiver, name } => Some(Some(
            reflection_member_filter_constant(ctx, receiver, name, constant_class)?,
        )),
        _ => None,
    }
}

/// Resolves a `Reflection*::IS_*` class constant to its integer value.
pub(super) fn reflection_member_filter_constant(
    ctx: &LoweringContext<'_, '_>,
    receiver: &StaticReceiver,
    name: &str,
    constant_class: &str,
) -> Option<i64> {
    let class_name = static_receiver_class_name(ctx, receiver)?;
    if php_symbol_key(class_name.trim_start_matches('\\')) != php_symbol_key(constant_class) {
        return None;
    }
    let value = ctx.scoped_constant_value(&class_name, name)?;
    let ExprKind::IntLiteral(value) = value.kind else {
        return None;
    };
    Some(value)
}

/// Returns whether a method should be present for a modifier filter.
pub(super) fn reflection_method_matches_filter(
    class_info: &crate::types::ClassInfo,
    method: &str,
    filter: Option<i64>,
) -> bool {
    let Some(filter) = filter else {
        return true;
    };
    reflection_method_filter_modifiers(class_info, method)
        .is_some_and(|modifiers| modifiers & filter != 0)
}

/// Returns whether a property should be present for a modifier filter.
pub(super) fn reflection_property_matches_filter(
    class_info: &crate::types::ClassInfo,
    property: &str,
    filter: Option<i64>,
) -> bool {
    let Some(filter) = filter else {
        return true;
    };
    reflection_property_filter_modifiers(class_info, property)
        .is_some_and(|modifiers| modifiers & filter != 0)
}

/// Computes ReflectionMethod modifier bits for static filter resolution.
pub(super) fn reflection_method_filter_modifiers(
    class_info: &crate::types::ClassInfo,
    method: &str,
) -> Option<i64> {
    let method_key = php_symbol_key(method);
    if class_info.methods.contains_key(&method_key) {
        let visibility = class_info
            .method_visibilities
            .get(&method_key)
            .unwrap_or(&Visibility::Public);
        return Some(reflection_method_filter_modifier_bits(
            visibility,
            false,
            class_info.final_methods.contains(&method_key),
            !class_info.method_impl_classes.contains_key(&method_key),
        ));
    }
    if class_info.static_methods.contains_key(&method_key) {
        let visibility = class_info
            .static_method_visibilities
            .get(&method_key)
            .unwrap_or(&Visibility::Public);
        return Some(reflection_method_filter_modifier_bits(
            visibility,
            true,
            class_info.final_static_methods.contains(&method_key),
            !class_info
                .static_method_impl_classes
                .contains_key(&method_key),
        ));
    }
    None
}

/// Computes ReflectionProperty modifier bits for static filter resolution.
pub(super) fn reflection_property_filter_modifiers(
    class_info: &crate::types::ClassInfo,
    property: &str,
) -> Option<i64> {
    if class_info
        .properties
        .iter()
        .any(|(name, _)| name == property)
    {
        let visibility = class_info
            .property_visibilities
            .get(property)
            .unwrap_or(&Visibility::Public);
        return Some(reflection_property_filter_modifier_bits(
            visibility,
            false,
            class_info.final_properties.contains(property),
            class_info.abstract_properties.contains(property),
            class_info.readonly_properties.contains(property),
            reflection_property_filter_is_virtual(class_info, property),
            class_info.property_set_visibilities.get(property),
        ));
    }
    if class_info
        .static_properties
        .iter()
        .any(|(name, _)| name == property)
    {
        let visibility = class_info
            .static_property_visibilities
            .get(property)
            .unwrap_or(&Visibility::Public);
        return Some(reflection_property_filter_modifier_bits(
            visibility,
            true,
            class_info.final_static_properties.contains(property),
            false,
            false,
            false,
            None,
        ));
    }
    None
}

/// Builds the ReflectionMethod modifier bitmask for filter matching.
pub(super) fn reflection_method_filter_modifier_bits(
    visibility: &Visibility,
    is_static: bool,
    is_final: bool,
    is_abstract: bool,
) -> i64 {
    let mut modifiers = match visibility {
        Visibility::Public => 1,
        Visibility::Protected => 2,
        Visibility::Private => 4,
    };
    if is_static {
        modifiers |= 16;
    }
    if is_final {
        modifiers |= 32;
    }
    if is_abstract {
        modifiers |= 64;
    }
    modifiers
}

/// Returns whether a property has hook metadata that makes it virtual.
pub(super) fn reflection_property_filter_is_virtual(
    class_info: &crate::types::ClassInfo,
    property: &str,
) -> bool {
    let get_method = php_symbol_key(&property_hook_get_method(property));
    let set_method = php_symbol_key(&property_hook_set_method(property));
    class_info.abstract_property_hooks.contains_key(property)
        || class_info.methods.contains_key(&get_method)
        || class_info.methods.contains_key(&set_method)
}

/// Builds the ReflectionProperty modifier bitmask for filter matching.
pub(super) fn reflection_property_filter_modifier_bits(
    visibility: &Visibility,
    is_static: bool,
    is_final: bool,
    is_abstract: bool,
    is_readonly: bool,
    is_virtual: bool,
    set_visibility: Option<&Visibility>,
) -> i64 {
    let mut modifiers = match visibility {
        Visibility::Public => 1,
        Visibility::Protected => 2,
        Visibility::Private => 4,
    };
    if is_static {
        modifiers |= 16;
    }
    if is_final {
        modifiers |= 32;
    }
    if is_abstract {
        modifiers |= 64;
    }
    if is_readonly {
        modifiers |= 128;
    }
    if is_virtual {
        modifiers |= 512;
    }
    match set_visibility {
        Some(Visibility::Private) => modifiers |= 32 | 4096,
        Some(Visibility::Protected) => modifiers |= 2048,
        Some(Visibility::Public) | None => {
            if is_readonly && visibility == &Visibility::Public {
                modifiers |= 2048;
            }
        }
    }
    modifiers
}
