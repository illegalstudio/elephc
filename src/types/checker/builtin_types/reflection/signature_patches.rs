//! Purpose:
//! Coordinates PHP-facing signature overrides for synthesized Reflection classes.
//!
//! Called from:
//! - The Reflection checker metadata facade after synthetic class initialization.
//!
//! Key details:
//! - Common overrides run before owner-specific patches and final cross-class adjustments.

use super::*;

/// Overrides synthesized Reflection method and property types to their PHP-facing contracts.
pub(crate) fn patch_builtin_reflection_signatures(checker: &mut Checker) {
    patch_reflection_attribute(checker);
    for class_name in [
        "ReflectionClass",
        "ReflectionObject",
        "ReflectionFunction",
        "ReflectionMethod",
        "ReflectionProperty",
        "ReflectionParameter",
        "ReflectionNamedType",
        "ReflectionUnionType",
        "ReflectionIntersectionType",
        "ReflectionClassConstant",
        "ReflectionEnumUnitCase",
        "ReflectionEnumBackedCase",
    ] {
        if let Some(class_info) = checker.classes.get_mut(class_name) {
            patch_initial_reflection_owner(class_name, class_info);
            patch_reflection_class_object(class_name, class_info);
            patch_shared_reflection_owner(class_name, class_info);
            patch_reflection_property(class_name, class_info);
            patch_reflection_method(class_name, class_info);
            patch_reflection_function(class_name, class_info);
            patch_reflection_parameter(class_name, class_info);
            patch_reflection_named_type(class_name, class_info);
            patch_reflection_union_type(class_name, class_info);
            patch_reflection_intersection_type(class_name, class_info);
            patch_reflection_attribute_result(class_info);
        }
    }
    // ReflectionEnum is built from a copy of ReflectionClass's members, so its `getAttributes()`
    // needs the same element type; the owner loop's other patches key on the owner names above.
    if let Some(class_info) = checker.classes.get_mut("ReflectionEnum") {
        patch_reflection_attribute_result(class_info);
    }
    patch_final_reflection_overrides(checker);
    propagate_reflection_patches_to_user_subclasses(checker);
}

/// Copies the patched Reflection property types and method signatures into user subclasses.
///
/// User classes are flattened before these patches run, so a `class Mine extends
/// ReflectionClass` inherited the unpatched declarations: `__interfaces` stayed an indexed
/// array while `ReflectionClass` itself now holds a name-keyed map. Codegen populates a
/// subclass through the Reflection owner path, which writes the patched shapes, and then frees
/// the object through the subclass's own property tags; the stale tags released a hash as an
/// indexed array. Only members still declared by the builtin ancestor are copied, so user
/// overrides and user-declared properties keep their own types.
fn propagate_reflection_patches_to_user_subclasses(checker: &mut Checker) {
    let is_builtin = is_builtin_reflection_class;
    let subclass_names = checker
        .classes
        .iter()
        .filter(|(name, info)| {
            !is_builtin(name) && reflection_builtin_ancestor(checker, info).is_some()
        })
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    for subclass_name in subclass_names {
        let Some(subclass) = checker.classes.get(&subclass_name) else {
            continue;
        };
        let property_types = subclass
            .properties
            .iter()
            .enumerate()
            .filter_map(|(index, (property_name, _))| {
                let declaring = subclass.property_declaring_classes.get(property_name)?;
                if !is_builtin(declaring) {
                    return None;
                }
                let builtin_type = checker
                    .classes
                    .get(declaring)?
                    .properties
                    .iter()
                    .find(|(name, _)| name == property_name)
                    .map(|(_, ty)| ty.clone())?;
                Some((index, builtin_type))
            })
            .collect::<Vec<_>>();
        let method_sigs = subclass
            .methods
            .keys()
            .filter_map(|method_key| {
                let declaring = subclass.method_declaring_classes.get(method_key)?;
                if !is_builtin(declaring) {
                    return None;
                }
                let sig = checker.classes.get(declaring)?.methods.get(method_key)?.clone();
                Some((method_key.clone(), sig))
            })
            .collect::<Vec<_>>();
        let Some(subclass) = checker.classes.get_mut(&subclass_name) else {
            continue;
        };
        for (index, builtin_type) in property_types {
            subclass.properties[index].1 = builtin_type;
        }
        for (method_key, sig) in method_sigs {
            subclass.methods.insert(method_key, sig);
        }
    }
}

/// Returns whether `name` is one of the builtin Reflection classes the checker injects.
fn is_builtin_reflection_class(name: &str) -> bool {
    gate::REFLECTION_CLASS_NAMES.contains(&name)
}

/// Returns the nearest builtin Reflection ancestor of a class, walking its parent chain.
fn reflection_builtin_ancestor<'a>(
    checker: &'a Checker,
    class_info: &'a ClassInfo,
) -> Option<&'a str> {
    let mut parent = class_info.parent.as_deref();
    for _ in 0..=checker.classes.len() {
        let name = parent?;
        if is_builtin_reflection_class(name) {
            return Some(name);
        }
        parent = checker.classes.get(name)?.parent.as_deref();
    }
    None
}
