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
        "ReflectionExtension",
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
    patch_final_reflection_overrides(checker);
}
