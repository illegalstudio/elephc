//! Purpose:
//! Injects SPL exception classes into checker metadata.
//! Models the standard hierarchy as builtin subclasses of `Exception`.
//!
//! Called from:
//! - `crate::types::checker::driver`
//!
//! Key details:
//! - These classes inherit behavior from `Exception`; only their nominal hierarchy is inserted here.

use std::collections::HashMap;

use crate::errors::CompileError;
use crate::types::traits::FlattenedClass;

use super::builtin_types::InterfaceDeclInfo;

/// (class_name, parent_name) — every SPL exception is a marker subclass that
/// inherits constructor, getMessage, and the message property from Exception
/// transitively.
const SPL_EXCEPTION_HIERARCHY: &[(&str, &str)] = &[
    ("LogicException", "Exception"),
    ("BadFunctionCallException", "LogicException"),
    ("BadMethodCallException", "BadFunctionCallException"),
    ("DomainException", "LogicException"),
    ("InvalidArgumentException", "LogicException"),
    ("LengthException", "LogicException"),
    ("OutOfRangeException", "LogicException"),
    ("RuntimeException", "Exception"),
    ("OutOfBoundsException", "RuntimeException"),
    ("OverflowException", "RuntimeException"),
    ("RangeException", "RuntimeException"),
    ("UnderflowException", "RuntimeException"),
    ("UnexpectedValueException", "RuntimeException"),
];

pub(crate) fn inject_builtin_spl_exceptions(
    interface_map: &mut HashMap<String, InterfaceDeclInfo>,
    class_map: &mut HashMap<String, FlattenedClass>,
) -> Result<(), CompileError> {
    for (name, _) in SPL_EXCEPTION_HIERARCHY {
        if *name == "RuntimeException" && class_map.contains_key(*name) {
            continue;
        }
        if interface_map.contains_key(*name) || class_map.contains_key(*name) {
            return Err(CompileError::new(
                crate::span::Span::dummy(),
                &format!("Cannot redeclare built-in SPL exception: {}", name),
            ));
        }
    }

    for (name, parent) in SPL_EXCEPTION_HIERARCHY {
        if class_map.contains_key(*name) {
            continue;
        }
        class_map.insert(
            (*name).to_string(),
            FlattenedClass {
                name: (*name).to_string(),
                extends: Some((*parent).to_string()),
                implements: Vec::new(),
                is_abstract: false,
                is_final: false,
                is_readonly_class: false,
                properties: Vec::new(),
                methods: Vec::new(),
                attributes: Vec::new(),
                constants: Vec::new(),
            },
        );
    }

    Ok(())
}
