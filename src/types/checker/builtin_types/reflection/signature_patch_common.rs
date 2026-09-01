//! Purpose:
//! Applies common and final checked-signature overrides for synthetic Reflection classes.
//!
//! Called from:
//! - patch_builtin_reflection_signatures() through the Reflection checker facade.
//!
//! Key details:
//! - Shared return types are patched before class-specific contracts, matching initialization order.

use super::*;

/// Patches the ReflectionAttribute property and method contracts.
pub(super) fn patch_reflection_attribute(checker: &mut Checker) {
    if let Some(class_info) = checker.classes.get_mut("ReflectionAttribute") {
        for (name, ty) in &mut class_info.properties {
            if name == "__args" {
                *ty = reflection_attribute_args_type();
            }
        }
        if let Some(sig) = class_info.methods.get_mut("__construct") {
            sig.return_type = PhpType::Void;
        }
        if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("getName")) {
            sig.return_type = PhpType::Str;
        }
        if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("getArguments")) {
            sig.return_type = reflection_attribute_args_type();
        }
        if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("newInstance")) {
            sig.return_type = PhpType::Mixed;
        }
        if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("getTarget")) {
            sig.return_type = PhpType::Int;
        }
        if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("isRepeated")) {
            sig.return_type = PhpType::Bool;
        }
    }
}

/// Patches contracts shared by every class in the Reflection owner loop.
pub(super) fn patch_initial_reflection_owner(class_name: &str, class_info: &mut ClassInfo) {
            if let Some(sig) = class_info.methods.get_mut("__construct") {
                sig.return_type = PhpType::Void;
            }
            if matches!(
                class_name,
                "ReflectionClass"
                    | "ReflectionObject"
                    | "ReflectionFunction"
                    | "ReflectionMethod"
                    | "ReflectionProperty"
                    | "ReflectionParameter"
                    | "ReflectionNamedType"
                    | "ReflectionUnionType"
                    | "ReflectionIntersectionType"
                    | "ReflectionClassConstant"
                    | "ReflectionEnumUnitCase"
                    | "ReflectionEnumBackedCase"
            ) {
                if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("getName")) {
                    sig.return_type = PhpType::Str;
                }
            }
            if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("getDocComment")) {
                sig.return_type = PhpType::Union(vec![PhpType::Str, PhpType::Bool]);
            }
            if let Some(sig) = class_info
                .methods
                .get_mut(&php_symbol_key("getExtensionName"))
            {
                sig.return_type = PhpType::Union(vec![PhpType::Str, PhpType::Bool]);
            }
            if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("getExtension")) {
                sig.return_type = PhpType::Mixed;
            }
            if matches!(
                class_name,
                "ReflectionMethod"
                    | "ReflectionProperty"
                    | "ReflectionClassConstant"
                    | "ReflectionEnumUnitCase"
                    | "ReflectionEnumBackedCase"
            ) {
                if let Some(sig) = class_info
                    .methods
                    .get_mut(&php_symbol_key("getDeclaringClass"))
                {
                    sig.return_type = PhpType::Object("ReflectionClass".to_string());
                }
            }
}

/// Patches contracts shared by related class, function, and member owners.
pub(super) fn patch_shared_reflection_owner(class_name: &str, class_info: &mut ClassInfo) {
            if matches!(
                class_name,
                "ReflectionClass" | "ReflectionObject" | "ReflectionEnum"
            ) {
                if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("__toString")) {
                    sig.return_type = PhpType::Str;
                }
            }
            if matches!(class_name, "ReflectionMethod" | "ReflectionProperty") {
                for method_name in ["isstatic", "ispublic", "isprotected", "isprivate"] {
                    if let Some(sig) = class_info.methods.get_mut(method_name) {
                        sig.return_type = PhpType::Bool;
                    }
                }
                if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("setAccessible")) {
                    sig.return_type = PhpType::Void;
                }
            }
            if matches!(class_name, "ReflectionFunction" | "ReflectionMethod") {
                for method_name in ["getShortName", "getNamespaceName"] {
                    if let Some(sig) = class_info.methods.get_mut(&php_symbol_key(method_name)) {
                        sig.return_type = PhpType::Str;
                    }
                }
                for method_name in [
                    "inNamespace",
                    "isInternal",
                    "isUserDefined",
                    "hasReturnType",
                    "isVariadic",
                    "isClosure",
                    "isDeprecated",
                    "returnsReference",
                    "isGenerator",
                    "hasTentativeReturnType",
                ] {
                    if let Some(sig) = class_info.methods.get_mut(&php_symbol_key(method_name)) {
                        sig.return_type = PhpType::Bool;
                    }
                }
                for method_name in ["getReturnType", "getTentativeReturnType"] {
                    if let Some(sig) = class_info.methods.get_mut(&php_symbol_key(method_name)) {
                        sig.return_type = PhpType::Mixed;
                    }
                }
            }
}

/// Patches the common getAttributes result collection.
///
/// The unfiltered path stores raw ReflectionAttribute pointers, while the filtered clone path
/// builds boxed Mixed slots. Expose the common safe element representation so callers unbox both.
pub(super) fn patch_reflection_attribute_result(class_info: &mut ClassInfo) {
    if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("getAttributes")) {
        sig.return_type = PhpType::Array(Box::new(PhpType::Mixed));
    }
}

/// Applies final cross-class overrides that depend on all owners being initialized.
pub(super) fn patch_final_reflection_overrides(checker: &mut Checker) {
    if let Some(class_info) = checker.classes.get_mut("ReflectionFunction") {
        if let Some(sig) = class_info.methods.get_mut("__construct") {
            sig.return_type = PhpType::Void;
        }
        if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("getParameters")) {
            sig.return_type = PhpType::Array(Box::new(PhpType::Object(
                "ReflectionParameter".to_string(),
            )));
        }
    }
    if let Some(class_info) = checker.classes.get_mut("ReflectionParameter") {
        if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("getType")) {
            // ReflectionNamedType|ReflectionUnionType|ReflectionIntersectionType|null.
            sig.return_type = PhpType::Union(vec![
                PhpType::Object("ReflectionNamedType".to_string()),
                PhpType::Object("ReflectionUnionType".to_string()),
                PhpType::Object("ReflectionIntersectionType".to_string()),
                PhpType::Void,
            ]);
        }
    }
}
