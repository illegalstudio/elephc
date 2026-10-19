//! Purpose:
//! Injects `SessionHandlerInterface` and the enum-only checks for PHP's `UnitEnum` /
//! `BackedEnum` (declared with `Serializable` by `builtin_interfaces`).
//!
//! Called from:
//! - `crate::types::checker::driver` during builtin type/schema initialization.
//!
//! Key details:
//! - `UnitEnum` / `BackedEnum` are implemented IMPLICITLY by every enum, exactly as in PHP:
//!   `schema::enums::insert_enum_metadata` appends them to the enum's interface list, which is
//!   what makes `$case instanceof UnitEnum` and a `BackedEnum $e` parameter work. A class may not
//!   implement them (`reject_non_enum_unit_enum`).
//! - `SessionHandlerInterface` is also declared by the `--web` prelude (`web_prelude::build`).
//!   That internal declaration wins and this injection skips the name; a declaration in the
//!   user's own source is a redeclaration error, as PHP reports it.

use std::collections::HashMap;

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{ClassMethod, Expr, Program, StmtKind, TypeExpr, Visibility};
use crate::source::SourceMode;
use crate::types::traits::FlattenedClass;

use super::builtin_types::InterfaceDeclInfo;

/// Name of PHP's implicit interface for every enum.
pub(crate) const UNIT_ENUM: &str = "UnitEnum";
/// Name of PHP's implicit interface for every backed enum.
pub(crate) const BACKED_ENUM: &str = "BackedEnum";

/// Whether `name` is `UnitEnum`, `BackedEnum` or an interface extending them (never a class).
///
/// Such a receiver has no slot layout of its own, so `ir_lower` and codegen read its
/// `->name` / `->value` through runtime (mixed) property dispatch on the actual enum class.
pub(crate) fn is_enum_contract_interface(
    interfaces: &HashMap<String, crate::types::InterfaceInfo>,
    name: &str,
) -> bool {
    let mut stack = vec![name.trim_start_matches('\\').to_string()];
    let mut seen = std::collections::HashSet::new();
    while let Some(current) = stack.pop() {
        if current == UNIT_ENUM {
            return true;
        }
        if !seen.insert(current.clone()) {
            continue;
        }
        if let Some(info) = interfaces.get(&current) {
            stack.extend(info.parents.iter().cloned());
        }
    }
    false
}
const SESSION_HANDLER_INTERFACE: &str = "SessionHandlerInterface";

/// Registers `SessionHandlerInterface` and rejects classes implementing the enum interfaces.
///
/// Rejects user redeclarations (PHP case-insensitive). A same-named declaration that came from
/// an internal prelude (the `--web` `SessionHandlerInterface`) is kept and not duplicated.
pub(crate) fn inject_builtin_native_interfaces(
    program: &Program,
    interface_map: &mut HashMap<String, InterfaceDeclInfo>,
    class_map: &mut HashMap<String, FlattenedClass>,
) -> Result<(), CompileError> {
    let mut skip = Vec::new();
    for builtin_name in [SESSION_HANDLER_INTERFACE] {
        let builtin_key = php_symbol_key(builtin_name);
        let collides = interface_map
            .keys()
            .chain(class_map.keys())
            .any(|name| php_symbol_key(name) == builtin_key);
        if !collides {
            continue;
        }
        if builtin_name == SESSION_HANDLER_INTERFACE && declared_internally(program, &builtin_key) {
            skip.push(builtin_name);
            continue;
        }
        let span = interface_map
            .iter()
            .filter(|(name, _)| php_symbol_key(name) == builtin_key)
            .map(|(_, info)| info.span)
            .chain(
                class_map
                    .iter()
                    .filter(|(name, _)| php_symbol_key(name) == builtin_key)
                    .map(|(_, class)| class.span),
            )
            .next()
            .unwrap_or_else(crate::span::Span::dummy);
        return Err(CompileError::new(
            span,
            &format!("Cannot redeclare built-in interface: {}", builtin_name),
        ));
    }

    if !skip.contains(&SESSION_HANDLER_INTERFACE) {
        interface_map.insert(
            SESSION_HANDLER_INTERFACE.to_string(),
            interface(
                SESSION_HANDLER_INTERFACE,
                Vec::new(),
                vec![
                    abstract_method(
                        "open",
                        false,
                        vec![("path", TypeExpr::Str), ("name", TypeExpr::Str)],
                        Some(TypeExpr::Bool),
                    ),
                    abstract_method("close", false, Vec::new(), Some(TypeExpr::Bool)),
                    abstract_method(
                        "read",
                        false,
                        vec![("id", TypeExpr::Str)],
                        Some(TypeExpr::Union(vec![TypeExpr::Str, TypeExpr::False])),
                    ),
                    abstract_method(
                        "write",
                        false,
                        vec![("id", TypeExpr::Str), ("data", TypeExpr::Str)],
                        Some(TypeExpr::Bool),
                    ),
                    abstract_method(
                        "destroy",
                        false,
                        vec![("id", TypeExpr::Str)],
                        Some(TypeExpr::Bool),
                    ),
                    abstract_method(
                        "gc",
                        false,
                        vec![("max_lifetime", TypeExpr::Int)],
                        Some(TypeExpr::Union(vec![TypeExpr::Int, TypeExpr::False])),
                    ),
                ],
            ),
        );
    }
    reject_non_enum_unit_enum(interface_map, class_map)
}

/// Rejects a class that implements `UnitEnum` (or `BackedEnum`), which PHP reserves for enums:
/// `Non-enum class X cannot implement interface UnitEnum`.
fn reject_non_enum_unit_enum(
    interface_map: &HashMap<String, InterfaceDeclInfo>,
    class_map: &HashMap<String, FlattenedClass>,
) -> Result<(), CompileError> {
    let mut names: Vec<&String> = class_map.keys().collect();
    names.sort();
    for name in names {
        let class = &class_map[name];
        if class_implements(class_map, interface_map, class, UNIT_ENUM) {
            // php names BackedEnum when the class lists it itself, UnitEnum otherwise (its
            // interface table reaches UnitEnum first through any derived interface).
            let named = if class
                .implements
                .iter()
                .any(|i| php_symbol_key(i) == php_symbol_key(BACKED_ENUM))
            {
                BACKED_ENUM
            } else {
                UNIT_ENUM
            };
            return Err(CompileError::new(
                class.span,
                &format!("Non-enum class {} cannot implement interface {}", class.name, named),
            ));
        }
    }
    Ok(())
}

/// Returns whether `class` (or a parent class) lists `target` among its interfaces, directly or
/// through interface inheritance.
fn class_implements(
    class_map: &HashMap<String, FlattenedClass>,
    interface_map: &HashMap<String, InterfaceDeclInfo>,
    class: &FlattenedClass,
    target: &str,
) -> bool {
    let target_key = php_symbol_key(target);
    let mut current = Some(class);
    let mut guard = 0;
    while let Some(class) = current {
        let mut stack: Vec<&str> = class.implements.iter().map(String::as_str).collect();
        let mut seen = std::collections::HashSet::new();
        while let Some(interface) = stack.pop() {
            if php_symbol_key(interface) == target_key {
                return true;
            }
            if !seen.insert(interface.to_string()) {
                continue;
            }
            if let Some(info) = interface_map.get(interface) {
                stack.extend(info.extends.iter().map(String::as_str));
            }
        }
        guard += 1;
        if guard > 256 {
            return false;
        }
        current = class.extends.as_deref().and_then(|parent| class_map.get(parent));
    }
    false
}

/// Returns whether a top-level declaration named `key` came from an internal prelude.
fn declared_internally(program: &Program, key: &str) -> bool {
    program.iter().any(|stmt| {
        stmt.source_mode == SourceMode::Internal
            && match &stmt.kind {
                StmtKind::InterfaceDecl { name, .. } | StmtKind::ClassDecl { name, .. } => {
                    php_symbol_key(name) == key
                }
                _ => false,
            }
    })
}

/// Builds an interface declaration with no properties or constants.
fn interface(name: &str, extends: Vec<String>, methods: Vec<ClassMethod>) -> InterfaceDeclInfo {
    InterfaceDeclInfo {
        name: name.to_string(),
        extends,
        properties: Vec::new(),
        methods,
        span: crate::span::Span::dummy(),
        constants: Vec::new(),
    }
}

/// Builds a public abstract interface method.
fn abstract_method(
    name: &str,
    is_static: bool,
    params: Vec<(&str, TypeExpr)>,
    return_type: Option<TypeExpr>,
) -> ClassMethod {
    let params: Vec<(String, Option<TypeExpr>, Option<Expr>, bool)> = params
        .into_iter()
        .map(|(param_name, ty)| (param_name.to_string(), Some(ty), None, false))
        .collect();
    ClassMethod {
        type_params: Vec::new(),
        name: name.to_string(),
        visibility: Visibility::Public,
        is_static,
        is_abstract: true,
        is_final: false,
        has_body: false,
        params,
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type,
        by_ref_return: false,
        body: Vec::new(),
        span: crate::span::Span::dummy(),
        attributes: Vec::new(),
    }
}
