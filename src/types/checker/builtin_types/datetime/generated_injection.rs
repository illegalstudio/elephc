//! Purpose:
//! Installs generated direct-AST declarations for the DateTime and DatePeriod families.
//!
//! Called from:
//! - The builtin-type checker initialization gates.
//!
//! Key details:
//! - Timelib and fallback declaration variants are selected without parsing PHP source.

use std::collections::HashMap;

use crate::parser::ast::{Program, Stmt, StmtKind};
use crate::types::traits::FlattenedClass;

use super::super::declarations::InterfaceDeclInfo;

/// Builds the selected generated declaration program.
fn declarations(uses_timelib: bool) -> Program {
    if uses_timelib {
        super::generated_declarations_timelib::generated_datetime_declarations()
    } else {
        super::generated_declarations_fallback::generated_datetime_declarations()
    }
}

/// Converts one generated class declaration into checker flattening metadata.
fn flatten_class(stmt: Stmt) -> Option<FlattenedClass> {
    let attributes = stmt.attributes;
    let span = stmt.span;
    let StmtKind::ClassDecl {
        name,
        extends,
        implements,
        is_abstract,
        is_final,
        is_readonly_class,
        trait_uses,
        properties,
        methods,
        constants,
    } = stmt.kind
    else {
        return None;
    };
    debug_assert!(trait_uses.is_empty(), "generated DateTime classes use no traits");
    Some(FlattenedClass {
        name,
        span,
        extends: extends.map(|name| name.as_str().to_string()),
        implements: implements
            .into_iter()
            .map(|name| name.as_str().to_string())
            .collect(),
        is_abstract,
        is_final,
        is_readonly_class,
        properties,
        methods,
        attributes,
        constants,
        used_traits: Vec::new(),
        trait_aliases: Vec::new(),
    })
}

/// Converts one generated interface declaration into checker metadata.
fn interface_info(stmt: Stmt) -> Option<InterfaceDeclInfo> {
    let span = stmt.span;
    let StmtKind::InterfaceDecl {
        name,
        extends,
        properties,
        methods,
        constants,
    } = stmt.kind
    else {
        return None;
    };
    Some(InterfaceDeclInfo {
        name,
        extends: extends
            .into_iter()
            .map(|name| name.as_str().to_string())
            .collect(),
        properties,
        methods,
        span,
        constants,
    })
}

/// Creates one empty php-src date exception subclass.
fn date_exception_class(name: &str, parent: &str) -> FlattenedClass {
    FlattenedClass {
        name: name.to_string(),
        span: crate::span::Span::dummy(),
        extends: Some(parent.to_string()),
        implements: Vec::new(),
        is_abstract: false,
        is_final: false,
        is_readonly_class: false,
        properties: Vec::new(),
        methods: Vec::new(),
        attributes: Vec::new(),
        constants: Vec::new(),
        used_traits: Vec::new(),
        trait_aliases: Vec::new(),
    }
}

/// Injects DateTimeInterface plus the four core DateTime classes from generated AST.
pub(crate) fn inject_builtin_datetime(
    interface_map: &mut HashMap<String, InterfaceDeclInfo>,
    class_map: &mut HashMap<String, FlattenedClass>,
    uses_timelib: bool,
) {
    for stmt in declarations(uses_timelib) {
        if let Some(info) = interface_info(stmt.clone()) {
            interface_map.entry(info.name.clone()).or_insert(info);
            continue;
        }
        let Some(class) = flatten_class(stmt) else {
            continue;
        };
        if class.name != "DatePeriod" {
            class_map.entry(class.name.clone()).or_insert(class);
        }
    }
    for (name, parent) in [
        ("DateError", "Error"),
        ("DateObjectError", "DateError"),
        ("DateRangeError", "DateError"),
        ("DateException", "Exception"),
        ("DateInvalidTimeZoneException", "DateException"),
        ("DateInvalidOperationException", "DateException"),
        ("DateMalformedStringException", "DateException"),
        ("DateMalformedIntervalStringException", "DateException"),
        ("DateMalformedPeriodStringException", "DateException"),
    ] {
        class_map
            .entry(name.to_string())
            .or_insert_with(|| date_exception_class(name, parent));
    }
}

/// Injects DatePeriod from the same generated declaration variant.
pub(crate) fn inject_builtin_date_period(
    class_map: &mut HashMap<String, FlattenedClass>,
    uses_timelib: bool,
) {
    if class_map.contains_key("DatePeriod") {
        return;
    }
    for stmt in declarations(uses_timelib) {
        let Some(class) = flatten_class(stmt) else {
            continue;
        };
        if class.name == "DatePeriod" {
            class_map.insert(class.name.clone(), class);
            return;
        }
    }
}
