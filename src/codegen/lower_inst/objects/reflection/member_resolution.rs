//! Purpose:
//! Declaring-owner, prototype, visibility, and member flags.
//!
//! Called from:
//! - `crate::codegen::lower_inst::objects::reflection`.
//!
//! Key details:
//! - Preserves compile-time metadata, target-aware object layout, and ownership.

use super::*;

/// Returns true when a property should be visible for `ReflectionClass::hasProperty()`.
///
/// Delegates to `ClassInfo::reflection_lists_property`, the rule the static EIR lowering of
/// `getProperties()` also uses, so the two paths cannot diverge.
pub(super) fn reflection_property_visible_from_class(
    info: &crate::types::ClassInfo,
    reflected_class: &str,
    property_name: &str,
    is_static: bool,
) -> bool {
    info.reflection_lists_property(reflected_class, property_name, is_static)
}

/// Returns the class that declares one reflected instance or static method.
pub(super) fn reflection_method_declaring_class_name(
    info: &crate::types::ClassInfo,
    reflected_class_name: &str,
    method_key: &str,
) -> Option<String> {
    info.method_declaring_classes
        .get(method_key)
        .or_else(|| info.static_method_declaring_classes.get(method_key))
        .cloned()
        .or_else(|| {
            reflection_class_has_method_kind(info, method_key, false)
                .then(|| reflected_class_name.to_string())
        })
        .or_else(|| {
            reflection_class_has_method_kind(info, method_key, true)
                .then(|| reflected_class_name.to_string())
        })
}

/// Returns a prototype method for a reflected generated/AOT class method, if PHP exposes one.
pub(super) fn reflection_class_method_prototype_member(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    info: &crate::types::ClassInfo,
    method_key: &str,
    flags: ReflectionMemberFlags,
) -> Result<Option<Box<ReflectionListedMember>>> {
    if !reflection_method_is_declared_on_class(info, class_name, method_key, flags.is_static) {
        return Ok(None);
    }
    if let Some(member) =
        reflection_parent_method_prototype_member(ctx, info, method_key, flags.is_static)?
    {
        return Ok(Some(Box::new(member)));
    }
    reflection_interface_method_prototype_member(ctx, info, method_key, flags.is_static)
        .map(|member| member.map(Box::new))
}

/// Returns whether a visible method entry is declared by the reflected class itself.
pub(super) fn reflection_method_is_declared_on_class(
    info: &crate::types::ClassInfo,
    class_name: &str,
    method_key: &str,
    is_static: bool,
) -> bool {
    let declaring_class = if is_static {
        info.static_method_declaring_classes.get(method_key)
    } else {
        info.method_declaring_classes.get(method_key)
    };
    declaring_class
        .map(|declaring_class| {
            php_symbol_key(declaring_class.trim_start_matches('\\'))
                == php_symbol_key(class_name.trim_start_matches('\\'))
        })
        .unwrap_or(false)
}

/// Finds the nearest parent-class method that is a valid PHP prototype.
pub(super) fn reflection_parent_method_prototype_member(
    ctx: &FunctionContext<'_>,
    info: &crate::types::ClassInfo,
    method_key: &str,
    is_static: bool,
) -> Result<Option<ReflectionListedMember>> {
    let mut current = reflection_parent_class_name(ctx, info);
    let mut seen = std::collections::HashSet::new();
    while let Some(parent_name) = current {
        if !seen.insert(php_symbol_key(&parent_name)) {
            break;
        }
        let Some((resolved_parent_name, parent_info)) = resolve_reflection_class(ctx, &parent_name)
        else {
            break;
        };
        if reflection_class_has_method_kind(parent_info, method_key, is_static) {
            if let Some(member) =
                reflection_class_method_member(ctx, resolved_parent_name, parent_info, method_key)?
            {
                if !member.flags.is_private && member.flags.is_static == is_static {
                    return Ok(Some(member));
                }
            }
        }
        current = reflection_parent_class_name(ctx, parent_info);
    }
    Ok(None)
}

/// Returns whether a class metadata entry has a method with the requested staticness.
pub(super) fn reflection_class_has_method_kind(
    info: &crate::types::ClassInfo,
    method_key: &str,
    is_static: bool,
) -> bool {
    if is_static {
        info.static_methods.contains_key(method_key)
    } else {
        info.methods.contains_key(method_key)
    }
}

/// Finds the first implemented interface method that is a valid PHP prototype.
pub(super) fn reflection_interface_method_prototype_member(
    ctx: &FunctionContext<'_>,
    info: &crate::types::ClassInfo,
    method_key: &str,
    is_static: bool,
) -> Result<Option<ReflectionListedMember>> {
    for interface_name in &info.interfaces {
        let Some(interface_name) = resolve_reflection_interface(ctx, interface_name) else {
            continue;
        };
        let Some(interface_info) = ctx.module.interface_infos.get(interface_name) else {
            continue;
        };
        let has_method = if is_static {
            interface_info.static_methods.contains_key(method_key)
        } else {
            interface_info.methods.contains_key(method_key)
        };
        if !has_method {
            continue;
        }
        if let Some(member) =
            reflection_interface_method_member(ctx, interface_info, interface_name, method_key)?
        {
            if member.flags.is_static == is_static {
                return Ok(Some(member));
            }
        }
    }
    Ok(None)
}

/// Returns the class that declares one reflected instance or static property.
pub(super) fn reflection_property_declaring_class_name(
    info: &crate::types::ClassInfo,
    property_name: &str,
) -> Option<String> {
    info.property_declaring_classes
        .get(property_name)
        .or_else(|| info.static_property_declaring_classes.get(property_name))
        .cloned()
}

/// Returns ReflectionMethod predicate flags for a method visible on one class.
pub(super) fn reflection_method_member_flags(
    info: &crate::types::ClassInfo,
    method_key: &str,
) -> Option<ReflectionMemberFlags> {
    if info.methods.contains_key(method_key) {
        let visibility = info
            .method_visibilities
            .get(method_key)
            .unwrap_or(&Visibility::Public);
        return Some(reflection_member_flags(
            false,
            visibility,
            info.final_methods.contains(method_key),
            !info.method_impl_classes.contains_key(method_key),
            false,
            false,
        ));
    }
    if info.static_methods.contains_key(method_key) {
        let visibility = info
            .static_method_visibilities
            .get(method_key)
            .unwrap_or(&Visibility::Public);
        return Some(reflection_member_flags(
            true,
            visibility,
            info.final_static_methods.contains(method_key),
            !info.static_method_impl_classes.contains_key(method_key),
            false,
            false,
        ));
    }
    None
}

/// Returns ReflectionProperty predicate flags for a property visible on one class.
pub(super) fn reflection_property_member_flags(
    info: &crate::types::ClassInfo,
    property_name: &str,
) -> Option<ReflectionMemberFlags> {
    if info
        .properties
        .iter()
        .any(|(name, _)| name == property_name)
    {
        let visibility = info
            .property_visibilities
            .get(property_name)
            .unwrap_or(&Visibility::Public);
        let mut flags = reflection_member_flags(
            false,
            visibility,
            info.final_properties.contains(property_name),
            info.abstract_properties.contains(property_name),
            info.readonly_properties.contains(property_name),
            info.promoted_properties.contains(property_name),
        );
        flags.is_virtual = reflection_property_is_virtual(info, property_name);
        return Some(flags);
    }
    if info
        .static_properties
        .iter()
        .any(|(name, _)| name == property_name)
    {
        let visibility = info
            .static_property_visibilities
            .get(property_name)
            .unwrap_or(&Visibility::Public);
        return Some(reflection_member_flags(
            true,
            visibility,
            info.final_static_properties.contains(property_name),
            false,
            false,
            false,
        ));
    }
    None
}

