//! Purpose:
//! Exports Elephc's current stream-facing declarations and capabilities as JSON.
//! The frozen php-src oracle compares this inventory without inventing PHP expectations.
//!
//! Called from:
//! - `tools/gen_builtins.rs` with `--streams-compliance --target=<target>`.
//!
//! Key details:
//! - Checker signatures and class metadata remain the Elephc declaration source.
//! - Wrapper, transport, filter, and constant values share the runtime/checker slices.
//! - The export reports incompatibilities; it does not normalize them into PHP values.
#![allow(dead_code)]

use crate::codegen::platform::Target;
use crate::parser::ast::{ClassMethod, Expr, ExprKind, Visibility};
use crate::types::{ClassInfo, FunctionSig};
use serde_json::{json, Value};
use std::collections::HashMap;

/// Builds the machine-readable Elephc stream surface used by the PHP oracle drift ledger.
///
/// The builtin registry remains the source for signatures, while constants and advertised
/// capabilities come from the exact slices consumed by the checker and EIR lowering.
pub fn export_json(target: Target) -> Value {
    let checked = crate::types::check_with_target(&Vec::new(), target)
        .expect("empty-program builtin surface type check");
    json!({
        "kind": "elephc-stream-surface",
        "target": {
            "platform": format!("{:?}", target.platform),
            "arch": format!("{:?}", target.arch),
        },
        "builtins": crate::builtins::docs::export_builtins_json(),
        "classes": class_inventory_json(&checked.classes),
        "constants": crate::types::stream_constants::STREAM_INT_CONSTANTS
            .iter()
            .map(|(name, value)| json!({"name": name, "type": "int", "value": value}))
            .collect::<Vec<_>>(),
        "wrappers": crate::types::stream_constants::STREAM_WRAPPERS,
        "transports": crate::types::stream_constants::STREAM_TRANSPORTS,
        "filters": crate::types::stream_constants::STREAM_FILTERS,
    })
}

/// Renders one checker signature for machine comparison with PHP reflection.
fn function_sig_json(signature: &FunctionSig) -> Value {
    let parameters = signature
        .params
        .iter()
        .enumerate()
        .map(|(index, (name, ty))| {
            json!({
                "position": index,
                "name": name,
                "type": ty.to_string(),
                "by_reference": signature.ref_params.get(index).copied().unwrap_or(false),
                "optional": signature.defaults.get(index).is_some_and(Option::is_some),
                "default": signature
                    .defaults
                    .get(index)
                    .and_then(Option::as_ref)
                    .map(expr_json),
                "variadic": signature.variadic.as_ref().is_some_and(|variadic| {
                    signature.params.get(index).is_some_and(|(param, _)| param == variadic)
                }),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "parameters": parameters,
        "return_type": signature.return_type.to_string(),
        "returns_reference": signature.by_ref_return,
        "variadic": signature.variadic,
        "deprecation": signature.deprecation,
    })
}

/// Renders a checker default expression without relying on PHP execution.
fn expr_json(expression: &Expr) -> Value {
    match &expression.kind {
        ExprKind::Null => json!({"kind": "null"}),
        ExprKind::BoolLiteral(value) => json!({"kind": "bool", "value": value}),
        ExprKind::IntLiteral(value) => json!({"kind": "int", "value": value}),
        ExprKind::FloatLiteral(value) => {
            json!({"kind": "float", "ieee754_be_hex": format!("{:016x}", value.to_bits())})
        }
        ExprKind::StringLiteral(value) => json!({"kind": "string", "value": value}),
        ExprKind::ArrayLiteral(values) if values.is_empty() => {
            json!({"kind": "array", "entries": []})
        }
        ExprKind::ArrayLiteralAssoc(values) if values.is_empty() => {
            json!({"kind": "array", "entries": []})
        }
        ExprKind::ConstRef(name) => json!({"kind": "constant", "name": name.as_str()}),
        _ => json!({"kind": "checker-expression", "debug": format!("{:?}", expression.kind)}),
    }
}

/// Maps one parser visibility to its PHP spelling.
fn visibility_str(visibility: &Visibility) -> &'static str {
    match visibility {
        Visibility::Public => "public",
        Visibility::Protected => "protected",
        Visibility::Private => "private",
    }
}

/// Finds the source declaration for one inherited checker method.
fn method_declaration<'a>(
    classes: &'a HashMap<String, ClassInfo>,
    class: &'a ClassInfo,
    key: &str,
    static_method: bool,
) -> Option<&'a ClassMethod> {
    let declaring = if static_method {
        class.static_method_declaring_classes.get(key)
    } else {
        class.method_declaring_classes.get(key)
    };
    declaring
        .and_then(|name| classes.get(name))
        .and_then(|info| {
            info.method_decls
                .iter()
                .find(|method| method.name.eq_ignore_ascii_case(key))
        })
        .or_else(|| {
            class
                .method_decls
                .iter()
                .find(|method| method.name.eq_ignore_ascii_case(key))
        })
}

/// Finds the case-preserved declaration name for one inherited checker method.
fn method_display_name(
    classes: &HashMap<String, ClassInfo>,
    class: &ClassInfo,
    key: &str,
    static_method: bool,
) -> String {
    method_declaration(classes, class, key, static_method)
        .map_or_else(|| key.to_string(), |method| method.name.clone())
}

/// Renders every checker-injected class, property, constant, and visible method.
fn class_inventory_json(classes: &HashMap<String, ClassInfo>) -> Value {
    let mut names = classes.keys().collect::<Vec<_>>();
    names.sort_unstable_by_key(|name| name.to_ascii_lowercase());
    Value::Array(
        names
            .into_iter()
            .map(|name| {
                let class = &classes[name];
                let mut methods = class
                    .methods
                    .iter()
                    .map(|(key, signature)| {
                        let declaration =
                            method_declaration(classes, class, key, false);
                        json!({
                            "name": method_display_name(classes, class, key, false),
                            "canonical_name": key,
                            "alias_of": Value::Null,
                            "declaring_class": class
                                .method_declaring_classes
                                .get(key)
                                .map_or(name.as_str(), String::as_str),
                            "static": false,
                            "visibility": class
                                .method_visibilities
                                .get(key)
                                .map_or("public", visibility_str),
                            "final": class.final_methods.contains(key),
                            "abstract": declaration.is_some_and(|method| method.is_abstract),
                            "deprecated": false,
                            "signature": function_sig_json(signature),
                        })
                    })
                    .chain(class.static_methods.iter().map(|(key, signature)| {
                        let declaration =
                            method_declaration(classes, class, key, true);
                        json!({
                            "name": method_display_name(classes, class, key, true),
                            "canonical_name": key,
                            "alias_of": Value::Null,
                            "declaring_class": class
                                .static_method_declaring_classes
                                .get(key)
                                .map_or(name.as_str(), String::as_str),
                            "static": true,
                            "visibility": class
                                .static_method_visibilities
                                .get(key)
                                .map_or("public", visibility_str),
                            "final": class.final_static_methods.contains(key),
                            "abstract": declaration.is_some_and(|method| method.is_abstract),
                            "deprecated": false,
                            "signature": function_sig_json(signature),
                        })
                    }))
                    .collect::<Vec<_>>();
                methods.sort_unstable_by_key(|method| {
                    method["canonical_name"].as_str().unwrap_or("").to_string()
                });
                let mut properties = class
                    .properties
                    .iter()
                    .enumerate()
                    .map(|(index, (property, ty))| {
                        json!({
                            "name": property,
                            "declaring_class": class
                                .property_declaring_classes
                                .get(property)
                                .map_or(name.as_str(), String::as_str),
                            "type": ty.to_string(),
                            "visibility": class
                                .property_visibilities
                                .get(property)
                                .map_or("public", visibility_str),
                            "set_visibility": class
                                .property_set_visibilities
                                .get(property)
                                .or_else(|| class.property_visibilities.get(property))
                                .map_or("public", visibility_str),
                            "static": false,
                            "readonly": class.readonly_properties.contains(property),
                            "final": class.final_properties.contains(property),
                            "virtual": class.abstract_properties.contains(property),
                            "has_hooks": class.abstract_property_hooks.contains_key(property),
                            "settable_type": ty.to_string(),
                            "default_available": class
                                .defaults
                                .get(index)
                                .is_some_and(Option::is_some),
                            "default": class
                                .defaults
                                .get(index)
                                .and_then(Option::as_ref)
                                .map(expr_json),
                        })
                    })
                    .collect::<Vec<_>>();
                properties.extend(
                    class
                        .static_properties
                        .iter()
                        .enumerate()
                        .map(|(index, (property, ty))| {
                            json!({
                                "name": property,
                                "declaring_class": class
                                    .static_property_declaring_classes
                                    .get(property)
                                    .map_or(name.as_str(), String::as_str),
                                "type": ty.to_string(),
                                "visibility": class
                                    .static_property_visibilities
                                    .get(property)
                                    .map_or("public", visibility_str),
                                "set_visibility": class
                                    .static_property_visibilities
                                    .get(property)
                                    .map_or("public", visibility_str),
                                "static": true,
                                "readonly": false,
                                "final": class.final_static_properties.contains(property),
                                "virtual": false,
                                "has_hooks": false,
                                "settable_type": ty.to_string(),
                                "default_available": class
                                    .static_defaults
                                    .get(index)
                                    .is_some_and(Option::is_some),
                                "default": class
                                    .static_defaults
                                    .get(index)
                                    .and_then(Option::as_ref)
                                    .map(expr_json),
                            })
                        }),
                );
                properties.sort_unstable_by_key(|property| {
                    (
                        property["name"].as_str().unwrap_or("").to_string(),
                        property["static"].as_bool().unwrap_or(false),
                    )
                });
                let mut constants = class
                    .constants
                    .iter()
                    .map(|(constant, value)| {
                        json!({
                            "name": constant,
                            "declaring_class": name,
                            "visibility": "public",
                            "final": false,
                            "deprecated": false,
                            "value": expr_json(value),
                        })
                    })
                    .collect::<Vec<_>>();
                constants.sort_unstable_by_key(|constant| {
                    constant["name"].as_str().unwrap_or("").to_string()
                });
                json!({
                    "name": name,
                    "parent": class.parent,
                    "interfaces": class.interfaces,
                    "traits": class.used_traits,
                    "abstract": class.is_abstract,
                    "final": class.is_final,
                    "readonly": class.is_readonly_class,
                    "internal": true,
                    "instantiable": !class.is_abstract,
                    "interface": false,
                    "trait": false,
                    "enum": false,
                    "anonymous": false,
                    "properties": properties,
                    "methods": methods,
                    "constants": constants,
                })
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    /// Verifies the compliance export shares the runtime-advertised capability slices.
    #[test]
    fn export_uses_runtime_capability_sources() {
        let export = super::export_json(crate::codegen::platform::Target::new(
            crate::codegen::platform::Platform::MacOS,
            crate::codegen::platform::Arch::AArch64,
        ));
        assert_eq!(
            export["wrappers"],
            serde_json::json!(crate::types::stream_constants::STREAM_WRAPPERS)
        );
        assert_eq!(
            export["transports"],
            serde_json::json!(crate::types::stream_constants::STREAM_TRANSPORTS)
        );
        assert_eq!(
            export["filters"],
            serde_json::json!(crate::types::stream_constants::STREAM_FILTERS)
        );
        assert_eq!(
            export["constants"].as_array().expect("constant array").len(),
            crate::types::stream_constants::STREAM_INT_CONSTANTS.len()
        );
    }
}
