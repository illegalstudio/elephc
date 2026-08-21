//! Purpose:
//! Converts locked PHP Reflection type/default metadata into Elephc checker records.
//! Keeps compiler phases on one generated internal-extension signature source.
//!
//! Called from:
//! - `crate::types::signatures` for internal function call planning.
//! - Internal class/interface declaration injection and constant registration.
//!
//! Key details:
//! - PHP `null` uses Elephc's `Void` sentinel inside nullable and union contracts.
//! - Tentative internal return types guide static inference while exact Reflection metadata remains unchanged.

use serde_json::Value;

use crate::names::Name;
use crate::parser::ast::{Expr, ExprKind, TypeExpr};
use crate::span::Span;
use crate::types::{FunctionSig, PhpType};

use super::{MethodSpec, SignatureSpec};

/// Builds the checker call signature for one locked internal function or method.
pub(crate) fn function_signature(signature: &SignatureSpec) -> Result<FunctionSig, String> {
    let mut params = Vec::with_capacity(signature.parameters.len());
    let mut param_type_exprs = Vec::with_capacity(signature.parameters.len());
    let mut param_attributes = Vec::with_capacity(signature.parameters.len());
    let mut defaults = Vec::with_capacity(signature.parameters.len());
    let mut ref_params = Vec::with_capacity(signature.parameters.len());
    let mut declared_params = Vec::with_capacity(signature.parameters.len());
    let mut variadic = None;

    for parameter in &signature.parameters {
        let type_expr = parameter
            .php_type
            .as_deref()
            .map(type_expr_from_name)
            .transpose()?;
        let php_type = parameter
            .php_type
            .as_deref()
            .map(php_type_from_name)
            .transpose()?
            .unwrap_or(PhpType::Mixed);
        let default = if parameter.variadic {
            Some(Expr::new(
                ExprKind::ArrayLiteral(Vec::new()),
                Span::dummy(),
            ))
        } else {
            parameter
                .default
                .as_ref()
                .map(value_expression)
                .transpose()?
        };

        params.push((parameter.name.clone(), php_type));
        param_type_exprs.push(type_expr);
        param_attributes.push(Vec::new());
        defaults.push(default);
        ref_params.push(parameter.by_reference);
        declared_params.push(parameter.php_type.is_some());
        if parameter.variadic {
            variadic = Some(parameter.name.clone());
        }
    }

    let return_name = signature
        .return_type
        .as_deref()
        .or(signature.tentative_return_type.as_deref());
    let return_type = return_name
        .map(php_type_from_name)
        .transpose()?
        .unwrap_or(PhpType::Mixed);

    Ok(FunctionSig {
        params,
        param_type_exprs,
        param_attributes,
        defaults,
        return_type,
        declared_return: return_name.is_some(),
        by_ref_return: signature.returns_reference,
        ref_params,
        declared_params,
        variadic,
        deprecation: signature.deprecated.then(String::new),
    })
}

/// Builds one function signature and applies compiler-only precision absent from Reflection types.
pub(crate) fn function_signature_for(
    name: &str,
    signature: &SignatureSpec,
) -> Result<FunctionSig, String> {
    let mut signature = function_signature(signature)?;
    if name
        .trim_start_matches('\\')
        .eq_ignore_ascii_case("libxml_get_errors")
    {
        signature.return_type =
            PhpType::Array(Box::new(PhpType::Object("LibXMLError".to_string())));
    }
    Ok(signature)
}

/// Returns one method's declared, tentative, or php-src stub-only return contract.
pub(crate) fn method_return_type_name(method: &MethodSpec) -> Option<&str> {
    if method.constructor {
        return Some("null");
    }
    method
        .signature
        .return_type
        .as_deref()
        .or(method.signature.tentative_return_type.as_deref())
        .or_else(|| {
            stub_only_method_return_type(
                &method.declaring_class,
                &method.signature.name,
            )
        })
}

/// Refines a method result whose concrete receiver guarantees a PHP subclass.
pub(crate) fn method_result_type_override(
    receiver_class: &str,
    method: &str,
) -> Option<PhpType> {
    let receiver = receiver_class.trim_start_matches('\\');
    if receiver.eq_ignore_ascii_case("Dom\\HTMLDocument")
        && method.eq_ignore_ascii_case("createElement")
    {
        return Some(PhpType::Object("Dom\\HTMLElement".to_string()));
    }
    if (receiver.eq_ignore_ascii_case("Dom\\Element")
        || receiver.eq_ignore_ascii_case("Dom\\HTMLElement"))
        && (method.eq_ignore_ascii_case("getInScopeNamespaces")
            || method.eq_ignore_ascii_case("getDescendantNamespaces"))
    {
        return Some(PhpType::Array(Box::new(PhpType::Object(
            "Dom\\NamespaceInfo".to_string(),
        ))));
    }
    if receiver.eq_ignore_ascii_case("DOMXPath")
        && method.eq_ignore_ascii_case("evaluate")
    {
        return Some(normalize_union(vec![
            PhpType::Void,
            PhpType::Bool,
            PhpType::Float,
            PhpType::Str,
            PhpType::Object("DOMNodeList".to_string()),
            PhpType::False,
        ]));
    }
    if receiver.eq_ignore_ascii_case("DOMXPath")
        && method.eq_ignore_ascii_case("query")
    {
        return Some(normalize_union(vec![
            PhpType::Object("DOMNodeList".to_string()),
            PhpType::False,
        ]));
    }
    if receiver.eq_ignore_ascii_case("Dom\\XPath")
        && method.eq_ignore_ascii_case("query")
    {
        return Some(PhpType::Object("Dom\\NodeList".to_string()));
    }
    if receiver.eq_ignore_ascii_case("SimpleXMLElement") {
        if method.eq_ignore_ascii_case("xpath") {
            return Some(normalize_union(vec![
                PhpType::Array(Box::new(PhpType::Object(
                    "SimpleXMLElement".to_string(),
                ))),
                PhpType::False,
                PhpType::Void,
            ]));
        }
        if method.eq_ignore_ascii_case("getNamespaces")
            || method.eq_ignore_ascii_case("getDocNamespaces")
        {
            return Some(normalize_union(vec![
                PhpType::AssocArray {
                    key: Box::new(PhpType::Str),
                    value: Box::new(PhpType::Str),
                },
                PhpType::False,
            ]));
        }
        if method.eq_ignore_ascii_case("__debugInfo") {
            return Some(PhpType::AssocArray {
                key: Box::new(PhpType::Mixed),
                value: Box::new(PhpType::Mixed),
            });
        }
    }
    None
}

/// Supplies php-src `@return` contracts that internal Reflection intentionally omits.
fn stub_only_method_return_type(class_name: &str, method_name: &str) -> Option<&'static str> {
    match (class_name, method_name) {
        ("DOMNode", "appendChild")
        | ("DOMNode", "cloneNode")
        | ("DOMNode", "insertBefore")
        | ("DOMNode", "removeChild")
        | ("DOMNode", "replaceChild")
        | ("DOMDocument", "importNode") => Some("DOMNode|false"),
        ("DOMImplementation", "createDocumentType") => {
            Some("DOMDocumentType|false")
        }
        ("DOMNodeList", "item") => {
            Some("DOMElement|DOMNode|DOMNameSpaceNode|null")
        }
        ("DOMNamedNodeMap", "getNamedItem")
        | ("DOMNamedNodeMap", "getNamedItemNS")
        | ("DOMNamedNodeMap", "item") => {
            Some("DOMNode|null")
        }
        ("Dom\\DtdNamedNodeMap", "getNamedItem")
        | ("Dom\\DtdNamedNodeMap", "getNamedItemNS")
        | ("Dom\\DtdNamedNodeMap", "item") => {
            Some(r"Dom\Entity|Dom\Notation|null")
        }
        ("Dom\\NamedNodeMap", "getNamedItem") => {
            Some(r"Dom\Attr|null")
        }
        ("Dom\\NamedNodeMap", "getNamedItemNS") => {
            Some(r"Dom\Attr|null")
        }
        ("Dom\\NamedNodeMap", "item") => {
            Some(r"Dom\Attr|null")
        }
        ("DOMCharacterData", "substringData") => Some("string|false"),
        ("DOMElement", "getAttributeNode") => {
            Some("DOMAttr|DOMNameSpaceNode|false")
        }
        ("DOMElement", "getAttributeNodeNS") => {
            Some("DOMAttr|DOMNameSpaceNode|null")
        }
        ("DOMElement", "removeAttributeNode")
        | ("DOMDocument", "createAttribute")
        | ("DOMDocument", "createAttributeNS") => Some("DOMAttr|false"),
        ("DOMElement", "setAttribute") => Some("DOMAttr|bool"),
        ("DOMElement", "setAttributeNode")
        | ("DOMElement", "setAttributeNodeNS") => Some("DOMAttr|null|false"),
        ("DOMDocument", "createCDATASection") => {
            Some("DOMCdataSection|false")
        }
        ("DOMDocument", "createElement")
        | ("DOMDocument", "createElementNS") => Some("DOMElement|false"),
        ("DOMDocument", "createEntityReference") => {
            Some("DOMEntityReference|false")
        }
        ("DOMDocument", "createProcessingInstruction") => {
            Some("DOMProcessingInstruction|false")
        }
        ("DOMText", "splitText") => Some("DOMText|false"),
        _ => None,
    }
}

/// Converts one PHP Reflection type string to a synthetic AST type expression.
pub(crate) fn type_expr_from_name(name: &str) -> Result<TypeExpr, String> {
    if let Some(inner) = name.strip_prefix('?') {
        return Ok(TypeExpr::Nullable(Box::new(type_expr_from_name(inner)?)));
    }
    if name.contains('|') {
        return name
            .split('|')
            .map(type_expr_from_name)
            .collect::<Result<Vec<_>, _>>()
            .map(TypeExpr::Union);
    }
    if name.contains('&') {
        return name
            .split('&')
            .map(type_expr_from_name)
            .collect::<Result<Vec<_>, _>>()
            .map(TypeExpr::Intersection);
    }

    Ok(match name {
        "int" => TypeExpr::Int,
        "float" => TypeExpr::Float,
        "bool" | "true" => TypeExpr::Bool,
        "false" => TypeExpr::False,
        "string" => TypeExpr::Str,
        "null" | "void" => TypeExpr::Void,
        "never" => TypeExpr::Never,
        "iterable" => TypeExpr::Iterable,
        "array" | "mixed" | "callable" | "object" | "static" | "self" | "parent" => {
            TypeExpr::Named(Name::unqualified(name))
        }
        class_name if !class_name.is_empty() => {
            let canonical = super::registry()
                .class(class_name)
                .map(|class| class.canonical_name.as_str())
                .unwrap_or(class_name);
            TypeExpr::Named(Name::qualified(
                canonical.split('\\').map(str::to_string).collect(),
            ))
        }
        _ => return Err("empty PHP Reflection type".to_string()),
    })
}

/// Converts one PHP Reflection type string directly to its checker type.
pub(crate) fn php_type_from_name(name: &str) -> Result<PhpType, String> {
    if let Some(inner) = name.strip_prefix('?') {
        return Ok(normalize_union(vec![
            php_type_from_name(inner)?,
            PhpType::Void,
        ]));
    }
    if name.contains('|') {
        return name
            .split('|')
            .map(php_type_from_name)
            .collect::<Result<Vec<_>, _>>()
            .map(normalize_union);
    }
    if name.contains('&') {
        return name
            .split('&')
            .next()
            .ok_or_else(|| "empty PHP Reflection intersection type".to_string())
            .and_then(php_type_from_name);
    }

    Ok(match name {
        "int" => PhpType::Int,
        "float" => PhpType::Float,
        "bool" | "true" => PhpType::Bool,
        "false" => PhpType::False,
        "string" => PhpType::Str,
        "null" | "void" => PhpType::Void,
        "never" => PhpType::Never,
        "iterable" => PhpType::Iterable,
        "array" => PhpType::Array(Box::new(PhpType::Mixed)),
        "mixed" => PhpType::Mixed,
        "callable" => PhpType::Callable,
        "object" => PhpType::Object(String::new()),
        "static" | "self" | "parent" => PhpType::Object(name.to_string()),
        class_name if !class_name.is_empty() => {
            let canonical = super::registry()
                .class(class_name)
                .map(|class| class.canonical_name.clone())
                .unwrap_or_else(|| class_name.to_string());
            PhpType::Object(canonical)
        }
        _ => return Err("empty PHP Reflection type".to_string()),
    })
}

/// Converts one locked scalar/default value wrapper into a synthetic AST expression.
pub(crate) fn value_expression(value: &Value) -> Result<Expr, String> {
    let value = if let Some(object) = value.as_object() {
        match object.get("kind").and_then(Value::as_str) {
            Some("value") => object
                .get("value")
                .ok_or_else(|| "internal default value wrapper lacks value".to_string())?,
            Some(kind) => {
                return Err(format!(
                    "unsupported internal default value wrapper kind: {kind}"
                ))
            }
            None => value,
        }
    } else {
        value
    };

    let kind = match value {
        Value::Null => ExprKind::Null,
        Value::Bool(value) => ExprKind::BoolLiteral(*value),
        Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                ExprKind::IntLiteral(value)
            } else if let Some(value) = value.as_f64() {
                ExprKind::FloatLiteral(value)
            } else {
                return Err("internal numeric value exceeds supported range".to_string());
            }
        }
        Value::String(value) => ExprKind::StringLiteral(value.clone()),
        Value::Array(values) if values.is_empty() => ExprKind::ArrayLiteral(Vec::new()),
        Value::Array(_) => {
            return Err("non-empty internal array defaults are not supported yet".to_string())
        }
        Value::Object(_) => {
            return Err("internal object defaults cannot become scalar AST expressions".to_string())
        }
    };
    Ok(Expr::new(kind, Span::dummy()))
}

/// Infers the checker type of one locked extension constant value.
pub(crate) fn value_php_type(value: &Value) -> Result<PhpType, String> {
    match value {
        Value::Null => Ok(PhpType::Void),
        Value::Bool(_) => Ok(PhpType::Bool),
        Value::Number(value) if value.is_i64() || value.is_u64() => Ok(PhpType::Int),
        Value::Number(_) => Ok(PhpType::Float),
        Value::String(_) => Ok(PhpType::Str),
        Value::Array(_) => Ok(PhpType::Array(Box::new(PhpType::Mixed))),
        Value::Object(object)
            if object.get("kind").and_then(Value::as_str) == Some("backed-enum") =>
        {
            object
                .get("class")
                .and_then(Value::as_str)
                .ok_or_else(|| "backed enum value lacks class".to_string())
                .and_then(php_type_from_name)
        }
        Value::Object(_) => Err("unsupported internal constant object value".to_string()),
    }
}

/// Removes duplicate union members while preserving their source order.
fn normalize_union(members: Vec<PhpType>) -> PhpType {
    let mut normalized = Vec::new();
    for member in members {
        if !normalized.contains(&member) {
            normalized.push(member);
        }
    }
    if normalized.len() == 1 {
        normalized
            .into_iter()
            .next()
            .expect("one normalized union member exists")
    } else {
        PhpType::Union(normalized)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies nullable object unions retain the canonical internal class name.
    #[test]
    fn reflection_type_conversion_preserves_nullable_internal_objects() {
        assert_eq!(
            php_type_from_name("?Dom\\Document").expect("DOM type"),
            PhpType::Union(vec![
                PhpType::Object("Dom\\Document".to_string()),
                PhpType::Void,
            ])
        );
    }

    /// Verifies PHP wrapped scalar defaults become call-planner expressions.
    #[test]
    fn reflection_default_conversion_unwraps_scalar_values() {
        let wrapped = serde_json::json!({"kind": "value", "value": false});
        assert!(matches!(
            value_expression(&wrapped).expect("false default").kind,
            ExprKind::BoolLiteral(false)
        ));
    }

    /// Verifies every locked internal function signature converts without a fallback table.
    #[test]
    fn locked_internal_function_signatures_convert() {
        for extension in super::super::registry().extensions() {
            for function in &extension.functions {
                function_signature_for(&function.exported_name, &function.signature)
                    .unwrap_or_else(|error| panic!("{}: {error}", function.exported_name));
            }
        }
    }

    /// Verifies zero-argument calls remain valid for optional internal variadics.
    #[test]
    fn internal_variadic_signature_uses_an_empty_array_default() {
        let class = super::super::registry()
            .class("Dom\\TokenList")
            .expect("locked token-list class");
        let method = class
            .methods
            .iter()
            .find(|method| method.signature.name == "add")
            .expect("locked token-list add method");
        let signature =
            function_signature(&method.signature).expect("valid add signature");
        assert_eq!(signature.variadic.as_deref(), Some("tokens"));
        assert!(matches!(
            signature.defaults.last().and_then(Option::as_ref),
            Some(Expr {
                kind: ExprKind::ArrayLiteral(items),
                ..
            }) if items.is_empty()
        ));
    }

    /// Verifies libxml's Reflection-level `array` result retains its known object element type.
    #[test]
    fn libxml_get_errors_signature_has_precise_elements() {
        let function = super::super::registry()
            .function("libxml_get_errors")
            .expect("locked libxml function");
        let signature = function_signature_for(&function.exported_name, &function.signature)
            .expect("valid libxml signature");
        assert_eq!(
            signature.return_type,
            PhpType::Array(Box::new(PhpType::Object("LibXMLError".to_string())))
        );
    }

    /// Verifies php-src stub-only legacy return contracts supplement Reflection metadata.
    #[test]
    fn stub_only_dom_method_return_types_are_preserved() {
        let class = super::super::registry()
            .class("DOMDocument")
            .expect("locked DOMDocument class");
        let method = class
            .methods
            .iter()
            .find(|method| method.signature.name == "createElement")
            .expect("locked createElement method");
        assert_eq!(
            method_return_type_name(method),
            Some("DOMElement|false")
        );
    }

    /// Verifies every internal constructor has PHP's observable null call result.
    #[test]
    fn internal_constructor_calls_are_null_typed() {
        for class_name in ["DOMDocument", "SimpleXMLElement"] {
            let class = super::super::registry()
                .class(class_name)
                .unwrap_or_else(|| panic!("locked {class_name} class"));
            let constructor = class
                .methods
                .iter()
                .find(|method| method.constructor)
                .unwrap_or_else(|| panic!("locked {class_name} constructor"));
            assert_eq!(method_return_type_name(constructor), Some("null"));
        }
    }

    /// Verifies HTML document element creation sharpens the inherited base return type.
    #[test]
    fn html_document_create_element_refines_to_html_element() {
        assert_eq!(
            method_result_type_override("Dom\\HTMLDocument", "createElement"),
            Some(PhpType::Object("Dom\\HTMLElement".to_string()))
        );
        assert_eq!(
            method_result_type_override("Dom\\XMLDocument", "createElement"),
            None
        );
    }

    /// Verifies modern namespace queries retain their known value-object element type.
    #[test]
    fn modern_namespace_query_results_have_precise_elements() {
        let expected = Some(PhpType::Array(Box::new(PhpType::Object(
            "Dom\\NamespaceInfo".to_string(),
        ))));
        assert_eq!(
            method_result_type_override(
                "Dom\\Element",
                "getInScopeNamespaces",
            ),
            expected.clone()
        );
        assert_eq!(
            method_result_type_override(
                "\\dom\\element",
                "GETDESCENDANTNAMESPACES",
            ),
            expected.clone()
        );
        assert_eq!(
            method_result_type_override(
                "Dom\\HTMLElement",
                "getInScopeNamespaces",
            ),
            expected
        );
    }

    /// Verifies legacy XPath methods retain php-src's scalar, node-list, and false alternatives.
    #[test]
    fn legacy_xpath_result_overrides_preserve_runtime_variants() {
        assert_eq!(
            method_result_type_override("DOMXPath", "query"),
            Some(PhpType::Union(vec![
                PhpType::Object("DOMNodeList".to_string()),
                PhpType::False,
            ]))
        );
        assert_eq!(
            method_result_type_override("domxpath", "EVALUATE"),
            Some(PhpType::Union(vec![
                PhpType::Void,
                PhpType::Bool,
                PhpType::Float,
                PhpType::Str,
                PhpType::Object("DOMNodeList".to_string()),
                PhpType::False,
            ]))
        );
        assert_eq!(
            method_result_type_override("Dom\\XPath", "query"),
            Some(PhpType::Object("Dom\\NodeList".to_string()))
        );
    }

    /// Verifies SimpleXML debug arrays admit both numeric and string PHP keys.
    #[test]
    fn simplexml_debug_info_override_uses_mixed_array_keys_and_values() {
        assert_eq!(
            method_result_type_override("SimpleXMLElement", "__debugInfo"),
            Some(PhpType::AssocArray {
                key: Box::new(PhpType::Mixed),
                value: Box::new(PhpType::Mixed),
            })
        );
    }
}
