//! Purpose:
//! Patches ReflectionClass, ReflectionObject, and ReflectionExtension collection signatures.
//!
//! Called from:
//! - patch_builtin_reflection_signatures() through the Reflection checker facade.
//!
//! Key details:
//! - Object maps, member reflectors, and dynamic construction keep their precise checked types.

use super::*;

/// Applies collection and construction signature overrides for class-like reflectors.
pub(super) fn patch_reflection_class_object(class_name: &str, class_info: &mut ClassInfo) {
            if matches!(class_name, "ReflectionClass" | "ReflectionObject") {
                for (property_name, property_type) in &mut class_info.properties {
                    match property_name.as_str() {
                        "__interfaces" | "__traits" => {
                            *property_type = reflection_class_object_map_type();
                        }
                        "__trait_aliases" => {
                            *property_type = reflection_string_map_type();
                        }
                        "__static_properties" => {
                            *property_type = reflection_static_properties_map_type();
                        }
                        _ => {}
                    }
                }
                for method_name in [
                    "isfinal",
                    "isabstract",
                    "isinterface",
                    "istrait",
                    "isenum",
                    "isreadonly",
                    "isanonymous",
                    "isinstantiable",
                    "iscloneable",
                    "isiterable",
                    "isiterateable",
                    "isinternal",
                    "isuserdefined",
                    "hasmethod",
                    "hasproperty",
                    "implementsinterface",
                    "issubclassof",
                    "isinstance",
                ] {
                    if let Some(sig) = class_info.methods.get_mut(method_name) {
                        sig.return_type = PhpType::Bool;
                    }
                }
                for method_name in ["getinterfacenames", "gettraitnames"] {
                    if let Some(sig) = class_info.methods.get_mut(method_name) {
                        sig.return_type = PhpType::Array(Box::new(PhpType::Str));
                    }
                }
                for method_name in ["getinterfaces", "gettraits"] {
                    if let Some(sig) = class_info.methods.get_mut(method_name) {
                        sig.return_type = reflection_class_object_map_type();
                    }
                }
                if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("getTraitAliases")) {
                    sig.return_type = reflection_string_map_type();
                }
                if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("getMethods")) {
                    sig.return_type =
                        PhpType::Array(Box::new(PhpType::Object("ReflectionMethod".to_string())));
                }
                if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("getMethod")) {
                    sig.return_type = PhpType::Object("ReflectionMethod".to_string());
                }
                if let Some(sig) = class_info
                    .methods
                    .get_mut(&php_symbol_key("getReflectionConstants"))
                {
                    sig.return_type = PhpType::Array(Box::new(PhpType::Object(
                        "ReflectionClassConstant".to_string(),
                    )));
                }
                if let Some(sig) = class_info
                    .methods
                    .get_mut(&php_symbol_key("getReflectionConstant"))
                {
                    sig.return_type = PhpType::Mixed;
                }
                if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("getProperties")) {
                    sig.return_type =
                        PhpType::Array(Box::new(PhpType::Object("ReflectionProperty".to_string())));
                }
                if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("getProperty")) {
                    sig.return_type = PhpType::Object("ReflectionProperty".to_string());
                }
                if let Some(sig) = class_info
                    .methods
                    .get_mut(&php_symbol_key("getStaticProperties"))
                {
                    sig.return_type = reflection_static_properties_map_type();
                }
                if let Some(sig) = class_info
                    .methods
                    .get_mut(&php_symbol_key("getConstructor"))
                {
                    sig.return_type = PhpType::Union(vec![
                        PhpType::Object("ReflectionMethod".to_string()),
                        PhpType::Void,
                    ]);
                }
                if let Some(sig) = class_info
                    .methods
                    .get_mut(&php_symbol_key("getParentClass"))
                {
                    sig.return_type = PhpType::Mixed;
                }
                if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("getModifiers")) {
                    sig.return_type = PhpType::Int;
                }
                if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("newInstance")) {
                    sig.return_type = PhpType::Object(String::new());
                    sig.variadic = Some("args".to_string());
                    let variadic_default = Some(Expr::new(
                        ExprKind::ArrayLiteral(Vec::new()),
                        crate::span::Span::dummy(),
                    ));
                    if let Some(index) = sig.params.iter().position(|(name, _)| name == "args") {
                        while sig.defaults.len() <= index {
                            sig.defaults.push(None);
                        }
                        sig.defaults[index] = variadic_default;
                    } else {
                        sig.params
                            .push(("args".to_string(), PhpType::Array(Box::new(PhpType::Mixed))));
                        sig.param_type_exprs.push(None);
                        sig.defaults.push(variadic_default);
                        sig.ref_params.push(false);
                        sig.declared_params.push(false);
                    }
                }
                if let Some(sig) = class_info
                    .methods
                    .get_mut(&php_symbol_key("newInstanceArgs"))
                {
                    sig.return_type = PhpType::Mixed;
                }
                if let Some(sig) = class_info
                    .methods
                    .get_mut(&php_symbol_key("newInstanceWithoutConstructor"))
                {
                    sig.return_type = PhpType::Mixed;
                }
            }
            if class_name == "ReflectionExtension" {
                for (property_name, property_type) in &mut class_info.properties {
                    match property_name.as_str() {
                        "__classes" => {
                            *property_type = reflection_extension_class_map_type();
                        }
                        "__functions" => {
                            *property_type = reflection_extension_function_map_type();
                        }
                        _ => {}
                    }
                }
                if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("getClasses")) {
                    sig.return_type = reflection_extension_class_map_type();
                }
                if let Some(sig) = class_info.methods.get_mut(&php_symbol_key("getFunctions")) {
                    sig.return_type = reflection_extension_function_map_type();
                }
            }
}
