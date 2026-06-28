//! Purpose:
//! Injects PHP builtin interfaces used by SPL, object contracts, and scalar interoperability.
//! Provides declarations before class/interface schema validation runs.
//!
//! Called from:
//! - `crate::types::checker::driver`
//!
//! Key details:
//! - Builtin names are checked with PHP case-insensitive collision rules before insertion.

use std::collections::HashMap;

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::names::Name;
use crate::parser::ast::{ClassMethod, Expr, TypeExpr, Visibility};
use crate::types::{traits::FlattenedClass, ClassInfo, PhpType};

use super::builtin_types::InterfaceDeclInfo;

const BUILTIN_INTERFACE_NAMES: &[&str] = &[
    "Traversable",
    "Iterator",
    "IteratorAggregate",
    "ArrayAccess",
    "Countable",
    "OuterIterator",
    "RecursiveIterator",
    "SeekableIterator",
    "SplObserver",
    "SplSubject",
    "Stringable",
];

/// Injects PHP SPL builtin interfaces into the type environment.
///
/// Adds `Traversable`, `Iterator`, `IteratorAggregate`, `ArrayAccess`, `Countable`,
/// `OuterIterator`, `RecursiveIterator`, `SeekableIterator`, `SplObserver`, `SplSubject`,
/// and `Stringable` as declared interfaces with their full method signatures.
///
/// ## Errors
/// Returns an error if any user-defined interface or class has a PHP-case-insensitive
/// name collision with a builtin interface name.
///
/// ## Inputs
/// - `interface_map`: target map receiving the builtin interface declarations
/// - `class_map`: checked for name collisions before insertion
pub(crate) fn inject_builtin_interfaces(
    interface_map: &mut HashMap<String, InterfaceDeclInfo>,
    class_map: &mut HashMap<String, FlattenedClass>,
) -> Result<(), CompileError> {
    for builtin_name in BUILTIN_INTERFACE_NAMES {
        let builtin_key = php_symbol_key(builtin_name);
        if interface_map
            .keys()
            .any(|name| php_symbol_key(name) == builtin_key)
            || class_map
                .keys()
                .any(|name| php_symbol_key(name) == builtin_key)
        {
            return Err(CompileError::new(
                crate::span::Span::dummy(),
                &format!("Cannot redeclare built-in interface: {}", builtin_name),
            ));
        }
    }

    interface_map.insert(
        "Traversable".to_string(),
        marker_interface("Traversable"),
    );

    interface_map.insert(
        "Iterator".to_string(),
        InterfaceDeclInfo {
            name: "Iterator".to_string(),
            extends: vec!["Traversable".to_string()],
            properties: Vec::new(),
            methods: vec![
                builtin_interface_method("current", TypeExpr::Named(Name::unqualified("mixed"))),
                builtin_interface_method("key", TypeExpr::Named(Name::unqualified("mixed"))),
                builtin_interface_method("next", TypeExpr::Void),
                builtin_interface_method("valid", TypeExpr::Bool),
                builtin_interface_method("rewind", TypeExpr::Void),
            ],
            span: crate::span::Span::dummy(),
            constants: Vec::new(),
        },
    );

    interface_map.insert(
        "IteratorAggregate".to_string(),
        InterfaceDeclInfo {
            name: "IteratorAggregate".to_string(),
            extends: vec!["Traversable".to_string()],
            properties: Vec::new(),
            methods: vec![builtin_interface_method(
                "getIterator",
                TypeExpr::Named(Name::unqualified("Traversable")),
            )],
            span: crate::span::Span::dummy(),
            constants: Vec::new(),
        },
    );

    interface_map.insert(
        "ArrayAccess".to_string(),
        InterfaceDeclInfo {
            name: "ArrayAccess".to_string(),
            extends: Vec::new(),
            properties: Vec::new(),
            methods: vec![
                builtin_interface_method_with_params(
                    "offsetExists",
                    vec![("offset", mixed_type())],
                    TypeExpr::Bool,
                ),
                builtin_interface_method_with_params(
                    "offsetGet",
                    vec![("offset", mixed_type())],
                    mixed_type(),
                ),
                builtin_interface_method_with_params(
                    "offsetSet",
                    vec![("offset", mixed_type()), ("value", mixed_type())],
                    TypeExpr::Void,
                ),
                builtin_interface_method_with_params(
                    "offsetUnset",
                    vec![("offset", mixed_type())],
                    TypeExpr::Void,
                ),
            ],
            span: crate::span::Span::dummy(),
            constants: Vec::new(),
        },
    );

    interface_map.insert(
        "Countable".to_string(),
        InterfaceDeclInfo {
            name: "Countable".to_string(),
            extends: Vec::new(),
            properties: Vec::new(),
            methods: vec![builtin_interface_method("count", TypeExpr::Int)],
            span: crate::span::Span::dummy(),
            constants: Vec::new(),
        },
    );

    interface_map.insert(
        "OuterIterator".to_string(),
        InterfaceDeclInfo {
            name: "OuterIterator".to_string(),
            extends: vec!["Iterator".to_string()],
            properties: Vec::new(),
            methods: vec![builtin_interface_method(
                "getInnerIterator",
                TypeExpr::Nullable(Box::new(TypeExpr::Named(Name::unqualified("Iterator")))),
            )],
            span: crate::span::Span::dummy(),
            constants: Vec::new(),
        },
    );

    interface_map.insert(
        "RecursiveIterator".to_string(),
        InterfaceDeclInfo {
            name: "RecursiveIterator".to_string(),
            extends: vec!["Iterator".to_string()],
            properties: Vec::new(),
            methods: vec![
                builtin_interface_method(
                    "getChildren",
                    TypeExpr::Nullable(Box::new(TypeExpr::Named(Name::unqualified(
                        "RecursiveIterator",
                    )))),
                ),
                builtin_interface_method("hasChildren", TypeExpr::Bool),
            ],
            span: crate::span::Span::dummy(),
            constants: Vec::new(),
        },
    );

    interface_map.insert(
        "SeekableIterator".to_string(),
        InterfaceDeclInfo {
            name: "SeekableIterator".to_string(),
            extends: vec!["Iterator".to_string()],
            properties: Vec::new(),
            methods: vec![builtin_interface_method_with_params(
                "seek",
                vec![("offset", TypeExpr::Int)],
                TypeExpr::Void,
            )],
            span: crate::span::Span::dummy(),
            constants: Vec::new(),
        },
    );

    interface_map.insert(
        "SplObserver".to_string(),
        InterfaceDeclInfo {
            name: "SplObserver".to_string(),
            extends: Vec::new(),
            properties: Vec::new(),
            methods: vec![builtin_interface_method_with_params(
                "update",
                vec![(
                    "subject",
                    TypeExpr::Named(Name::unqualified("SplSubject")),
                )],
                TypeExpr::Void,
            )],
            span: crate::span::Span::dummy(),
            constants: Vec::new(),
        },
    );

    interface_map.insert(
        "SplSubject".to_string(),
        InterfaceDeclInfo {
            name: "SplSubject".to_string(),
            extends: Vec::new(),
            properties: Vec::new(),
            methods: vec![
                builtin_interface_method_with_params(
                    "attach",
                    vec![(
                        "observer",
                        TypeExpr::Named(Name::unqualified("SplObserver")),
                    )],
                    TypeExpr::Void,
                ),
                builtin_interface_method_with_params(
                    "detach",
                    vec![(
                        "observer",
                        TypeExpr::Named(Name::unqualified("SplObserver")),
                    )],
                    TypeExpr::Void,
                ),
                builtin_interface_method("notify", TypeExpr::Void),
            ],
            span: crate::span::Span::dummy(),
            constants: Vec::new(),
        },
    );

    interface_map.insert(
        "Stringable".to_string(),
        InterfaceDeclInfo {
            name: "Stringable".to_string(),
            extends: Vec::new(),
            properties: Vec::new(),
            methods: vec![builtin_interface_method("__toString", TypeExpr::Str)],
            span: crate::span::Span::dummy(),
            constants: Vec::new(),
        },
    );

    Ok(())
}

/// Applies implicit `Stringable` interface to classes that implement `__toString`.
///
/// After all user class declarations are resolved, this scans every class and adds
/// `Stringable` to its interface list if the class has a `public __toString(): string`
/// method. This mirrors PHP's implicit interface behavior.
///
/// ## Inputs
/// - `classes`: mutable map of `ClassInfo` entries; `Stringable` is pushed into each
///   qualifying class's `interfaces` vector in-place
pub(crate) fn apply_implicit_stringable_interfaces(
    classes: &mut HashMap<String, ClassInfo>,
) {
    let tostring_key = php_symbol_key("__toString");
    for class_info in classes.values_mut() {
        let has_compatible_tostring = class_info
            .methods
            .get(&tostring_key)
            .is_some_and(|sig| sig.return_type == PhpType::Str)
            && class_info
                .method_visibilities
                .get(&tostring_key)
                .is_some_and(|visibility| *visibility == Visibility::Public);
        if has_compatible_tostring
            && !class_info
                .interfaces
                .iter()
                .any(|iface| php_symbol_key(iface) == php_symbol_key("Stringable"))
        {
            class_info.interfaces.push("Stringable".to_string());
        }
    }
}

/// Constructs a marker `InterfaceDeclInfo` with no methods, no properties, and no extends.
///
/// Used for `Traversable` (which is an empty marker interface in SPL) and for any
/// interface that needs no own members.
fn marker_interface(name: &str) -> InterfaceDeclInfo {
    InterfaceDeclInfo {
        name: name.to_string(),
        extends: Vec::new(),
        properties: Vec::new(),
        methods: Vec::new(),
        span: crate::span::Span::dummy(),
        constants: Vec::new(),
    }
}

/// Returns a `TypeExpr::Named("mixed")` used as the parameter/return type for
/// generic interface methods.
fn mixed_type() -> TypeExpr {
    TypeExpr::Named(Name::unqualified("mixed"))
}

/// Builds a parameterless `ClassMethod` for a builtin interface method.
///
/// The method is always `public`, non-static, `abstract`, no-body, and uses a dummy span.
/// Used for simple methods like `current()`, `key()`, `valid()`, `rewind()`, `next()`.
fn builtin_interface_method(name: &str, return_type: TypeExpr) -> ClassMethod {
    ClassMethod {
        name: name.to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: true,
        is_final: false,
        has_body: false,
        params: Vec::new(),
        variadic: None,
        variadic_type: None,
        return_type: Some(return_type),
        by_ref_return: false,
        body: Vec::new(),
        span: crate::span::Span::dummy(),
        attributes: Vec::new(),
    }
}

/// Builds a `ClassMethod` with parameters for a builtin interface method.
///
/// The method is always `public`, non-static, `abstract`, no-body, and uses a dummy span.
/// Used for methods that take typed parameters, such as `offsetGet(offset: mixed): mixed`.
fn builtin_interface_method_with_params(
    name: &str,
    params: Vec<(&str, TypeExpr)>,
    return_type: TypeExpr,
) -> ClassMethod {
    let params: Vec<(String, Option<TypeExpr>, Option<Expr>, bool)> = params
        .into_iter()
        .map(|(param_name, ty)| (param_name.to_string(), Some(ty), None, false))
        .collect();
    ClassMethod {
        name: name.to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: true,
        is_final: false,
        has_body: false,
        params,
        variadic: None,
        variadic_type: None,
        return_type: Some(return_type),
        by_ref_return: false,
        body: Vec::new(),
        span: crate::span::Span::dummy(),
        attributes: Vec::new(),
    }
}
