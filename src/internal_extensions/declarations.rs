//! Purpose:
//! Builds synthetic checker declarations for locked PHP internal classes and interfaces.
//! Exposes exact inheritance, signatures, modifiers, properties, and constants without userland shims.
//!
//! Called from:
//! - `crate::types::checker::driver::check_types_impl()` before schema construction.
//!
//! Key details:
//! - Export aliases participate in redeclaration checks but only canonical definitions are inserted.
//! - Synthetic method bodies are never runtime implementations; EIR lowers internal operations separately.

use std::collections::{HashMap, HashSet};

use serde_json::Value;

use crate::errors::CompileError;
use crate::names::{Name, php_symbol_key};
use crate::parser::ast::{
    ClassConst, ClassMethod, ClassProperty, Expr, ExprKind, PropertyHooks, Stmt, StmtKind,
    TypeExpr, Visibility,
};
use crate::span::Span;
use crate::types::checker::InterfaceDeclInfo;
use crate::types::traits::FlattenedClass;

use super::{ClassConstantSpec, ClassSpec, MethodSpec, ParameterSpec, PropertySpec};

/// Injects every canonical non-enum internal class/interface after rejecting exported-name collisions.
pub(crate) fn inject_checker_declarations(
    interface_map: &mut HashMap<String, InterfaceDeclInfo>,
    class_map: &mut HashMap<String, FlattenedClass>,
    declared_traits: &HashSet<String>,
) -> Result<(), CompileError> {
    super::validate_locked_snapshots();
    ensure_export_names_available(interface_map, class_map, declared_traits)?;

    let mut inserted = HashSet::new();
    for class in super::registry().classes() {
        let canonical_key = php_symbol_key(&class.canonical_name);
        if class.enum_type || !inserted.insert(canonical_key) {
            continue;
        }
        if class.interface {
            interface_map.insert(class.canonical_name.clone(), interface_declaration(class)?);
        } else {
            class_map.insert(class.canonical_name.clone(), class_declaration(class)?);
        }
    }
    Ok(())
}

/// Rejects userland or existing builtin declarations colliding with any exported internal name.
fn ensure_export_names_available(
    interface_map: &HashMap<String, InterfaceDeclInfo>,
    class_map: &HashMap<String, FlattenedClass>,
    declared_traits: &HashSet<String>,
) -> Result<(), CompileError> {
    for class in super::registry().classes() {
        let export_key = php_symbol_key(&class.exported_name);
        if interface_map
            .keys()
            .chain(class_map.keys())
            .chain(declared_traits.iter())
            .any(|name| php_symbol_key(name) == export_key)
        {
            return Err(CompileError::new(
                Span::dummy(),
                &format!(
                    "Cannot redeclare built-in internal-extension type: {}",
                    class.exported_name
                ),
            ));
        }
    }
    Ok(())
}

/// Canonical names of DOM live collections whose `getIterator()` must return an
/// `InternalIterator`.
const DOM_ITERATOR_COLLECTION_CLASSES: &[&str] = &[
    "DOMNodeList",
    "DOMNamedNodeMap",
    "Dom\\NodeList",
    "Dom\\NamedNodeMap",
    "Dom\\DtdNamedNodeMap",
    "Dom\\HTMLCollection",
    "Dom\\TokenList",
];

/// DOM collections whose iterator keys are the member name rather than a numeric
/// position. The `InternalIterator` wrapper uses this flag to decide between
/// returning `nodeName` and the cursor index.
const DOM_NAMED_KEY_COLLECTION_CLASSES: &[&str] = &[
    "DOMNamedNodeMap",
    "Dom\\NamedNodeMap",
    "Dom\\DtdNamedNodeMap",
];

/// Replaces the locked `getIterator()` shell on DOM live collections with the
/// synthetic body `return new InternalIterator($this, $named_keys);`.
fn inject_dom_collection_get_iterator_body(
    methods: &mut [ClassMethod],
    class_name: &str,
) {
    let normalized = class_name.trim_start_matches('\\');
    if !DOM_ITERATOR_COLLECTION_CLASSES.contains(&normalized) {
        return;
    }
    let named_keys = DOM_NAMED_KEY_COLLECTION_CLASSES.contains(&normalized);
    let target_key = php_symbol_key("getIterator");
    for method in methods {
        if php_symbol_key(&method.name) == target_key {
            method.has_body = true;
            let mut args = vec![Expr::new(ExprKind::This, Span::dummy())];
            if named_keys {
                args.push(Expr::new(ExprKind::BoolLiteral(true), Span::dummy()));
            }
            method.body = vec![Stmt::new(
                StmtKind::Return(Some(Expr::new(
                    ExprKind::NewObject {
                        class_name: Name::unqualified("InternalIterator"),
                        args,
                    },
                    Span::dummy(),
                ))),
                Span::dummy(),
            )];
        }
    }
}

/// Builds one canonical internal interface declaration for checker schema construction.
fn interface_declaration(class: &ClassSpec) -> Result<InterfaceDeclInfo, CompileError> {
    Ok(InterfaceDeclInfo {
        name: class.canonical_name.clone(),
        extends: class.interfaces.clone(),
        properties: class
            .properties
            .iter()
            .map(property_declaration)
            .collect::<Result<Vec<_>, _>>()?,
        methods: class
            .methods
            .iter()
            .map(|method| method_declaration(method, true))
            .collect::<Result<Vec<_>, _>>()?,
        span: Span::dummy(),
        constants: class
            .constants
            .iter()
            .map(constant_declaration)
            .collect::<Result<Vec<_>, _>>()?,
    })
}

/// Builds one canonical internal class declaration for checker schema construction.
fn class_declaration(class: &ClassSpec) -> Result<FlattenedClass, CompileError> {
    let mut methods = class
        .methods
        .iter()
        .map(|method| method_declaration(method, false))
        .collect::<Result<Vec<_>, _>>()?;
    for method in &mut methods {
        method.substitute_relative_class_types(&class.canonical_name, class.parent.as_deref());
    }
    inject_dom_collection_get_iterator_body(&mut methods, &class.canonical_name);

    Ok(FlattenedClass {
        name: class.canonical_name.clone(),
        span: Span::dummy(),
        extends: class.parent.clone(),
        implements: class.interfaces.clone(),
        is_abstract: class.abstract_class,
        is_final: class.final_class,
        is_readonly_class: class.readonly_class,
        properties: class
            .properties
            .iter()
            .map(property_declaration)
            .collect::<Result<Vec<_>, _>>()?,
        methods,
        attributes: Vec::new(),
        constants: class
            .constants
            .iter()
            .map(constant_declaration)
            .collect::<Result<Vec<_>, _>>()?,
        used_traits: Vec::new(),
        trait_aliases: Vec::new(),
    })
}

/// Converts one internal method into a checker-only class method shell.
fn method_declaration(
    method: &MethodSpec,
    interface_method: bool,
) -> Result<ClassMethod, CompileError> {
    let mut params = Vec::new();
    let mut param_attributes = Vec::new();
    let mut variadic = None;
    let mut variadic_by_ref = false;
    let mut variadic_type = None;

    for parameter in &method.signature.parameters {
        let type_expr = parameter_type_expr(parameter, method)?;
        if parameter.variadic {
            variadic = Some(parameter.name.clone());
            variadic_by_ref = parameter.by_reference;
            variadic_type = type_expr;
            param_attributes.push(Vec::new());
            continue;
        }
        let default = parameter
            .default
            .as_ref()
            .map(super::value_expression)
            .transpose()
            .map_err(|error| metadata_error(method, &error))?;
        params.push((
            parameter.name.clone(),
            type_expr,
            default,
            parameter.by_reference,
        ));
        param_attributes.push(Vec::new());
    }

    let return_type = if method.signature.name == "getAttributeNames"
        && matches!(
            method.declaring_class.as_str(),
            "DOMElement" | "Dom\\Element"
        )
    {
        Some(TypeExpr::Array(Box::new(TypeExpr::Str)))
    } else {
        super::types::method_return_type_name(method)
            .map(super::type_expr_from_name)
            .transpose()
            .map_err(|error| metadata_error(method, &error))?
    };
    let is_abstract = interface_method || method.abstract_method;

    Ok(ClassMethod {
        name: method.signature.name.clone(),
        visibility: visibility(method.public, method.protected),
        is_static: method.static_method,
        is_abstract,
        is_final: method.final_method,
        has_body: !is_abstract,
        params,
        param_attributes,
        variadic,
        variadic_by_ref,
        variadic_type,
        return_type,
        by_ref_return: method.signature.returns_reference,
        body: Vec::new(),
        span: Span::dummy(),
        attributes: Vec::new(),
    })
}

/// Converts one internal parameter type and annotates malformed metadata with its method name.
fn parameter_type_expr(
    parameter: &ParameterSpec,
    method: &MethodSpec,
) -> Result<Option<TypeExpr>, CompileError> {
    if method.signature.name == "registerPhpFunctions"
        && matches!(
            method.declaring_class.as_str(),
            "DOMXPath" | "Dom\\XPath"
        )
        && parameter.name == "restrict"
    {
        return Ok(None);
    }
    if method.signature.name == "isDefaultNamespace"
        && method.declaring_class == "DOMNode"
        && parameter.name == "namespace"
    {
        return Ok(Some(TypeExpr::Nullable(Box::new(TypeExpr::Str))));
    }
    parameter
        .php_type
        .as_deref()
        .map(super::type_expr_from_name)
        .transpose()
        .map_err(|error| metadata_error(method, &error))
}

/// Converts one internal property into checker metadata.
fn property_declaration(property: &PropertySpec) -> Result<ClassProperty, CompileError> {
    let type_expr = property
        .php_type
        .as_deref()
        .map(super::type_expr_from_name)
        .transpose()
        .map_err(|error| {
            CompileError::new(
                Span::dummy(),
                &format!(
                    "Invalid internal property metadata for {}::${}: {}",
                    property.declaring_class, property.name, error
                ),
            )
        })?
        .or_else(|| {
            (property.declaring_class == "DOMException" && property.name == "code")
                .then_some(TypeExpr::Int)
        });
    let default = property
        .default
        .as_ref()
        .map(super::value_expression)
        .transpose()
        .map_err(|error| {
            CompileError::new(
                Span::dummy(),
                &format!(
                    "Invalid internal property default for {}::${}: {}",
                    property.declaring_class, property.name, error
                ),
            )
        })?;

    Ok(ClassProperty {
        name: property.name.clone(),
        visibility: visibility(property.public, property.protected),
        set_visibility: None,
        type_expr,
        hooks: PropertyHooks::none(),
        readonly: property.readonly,
        is_final: false,
        is_static: property.static_property,
        is_abstract: false,
        by_ref: false,
        is_promoted: false,
        default,
        span: Span::dummy(),
        attributes: Vec::new(),
    })
}

/// Converts one internal class constant into checker metadata.
fn constant_declaration(constant: &ClassConstantSpec) -> Result<ClassConst, CompileError> {
    let type_expr = constant
        .php_type
        .as_deref()
        .map(super::type_expr_from_name)
        .transpose()
        .map_err(|error| {
            CompileError::new(
                Span::dummy(),
                &format!(
                    "Invalid internal class constant type for {}::{}: {}",
                    constant.declaring_class, constant.name, error
                ),
            )
        })?;
    let value = class_constant_expression(&constant.value).map_err(|error| {
        CompileError::new(
            Span::dummy(),
            &format!(
                "Invalid internal class constant value for {}::{}: {}",
                constant.declaring_class, constant.name, error
            ),
        )
    })?;

    Ok(ClassConst {
        name: constant.name.clone(),
        visibility: visibility(constant.public, constant.protected),
        is_final: constant.final_constant,
        type_expr,
        value,
        span: Span::dummy(),
        attributes: Vec::new(),
    })
}

/// Converts a non-enum internal class constant value into a literal expression.
fn class_constant_expression(value: &Value) -> Result<Expr, String> {
    if value
        .as_object()
        .and_then(|object| object.get("kind"))
        .and_then(Value::as_str)
        == Some("backed-enum")
    {
        return Err("enum cases must be injected through enum metadata".to_string());
    }
    super::value_expression(value)
}

/// Maps Reflection visibility flags onto the AST visibility enum.
fn visibility(public: bool, protected: bool) -> Visibility {
    if public {
        Visibility::Public
    } else if protected {
        Visibility::Protected
    } else {
        Visibility::Private
    }
}

/// Builds a precise compiler diagnostic for malformed method metadata.
fn metadata_error(method: &MethodSpec, error: &str) -> CompileError {
    CompileError::new(
        Span::dummy(),
        &format!(
            "Invalid internal method metadata for {}::{}: {}",
            method.declaring_class, method.signature.name, error
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies canonical declarations are inserted once while aliases remain collision-only.
    #[test]
    fn checker_declarations_insert_canonical_classes_once() {
        let mut interfaces = HashMap::new();
        let mut classes = HashMap::new();
        inject_checker_declarations(&mut interfaces, &mut classes, &HashSet::new())
            .expect("locked declarations");

        assert!(classes.contains_key("DOMException"));
        assert!(!classes.contains_key("dom\\domexception"));
        assert!(interfaces.contains_key("Dom\\ParentNode"));
        assert_eq!(
            classes
                .get("Dom\\HTMLDocument")
                .and_then(|class| class.extends.as_deref()),
            Some("Dom\\Document")
        );
    }

    /// Verifies every locked non-enum class member converts into checker declarations.
    #[test]
    fn checker_declarations_convert_complete_locked_surface() {
        let mut interfaces = HashMap::new();
        let mut classes = HashMap::new();
        inject_checker_declarations(&mut interfaces, &mut classes, &HashSet::new())
            .expect("complete locked declarations");

        let direct_method_count = interfaces
            .values()
            .map(|interface| interface.methods.len())
            .sum::<usize>()
            + classes
                .values()
                .map(|class| class.methods.len())
                .sum::<usize>();
        assert_eq!(direct_method_count, 332);
    }
}
