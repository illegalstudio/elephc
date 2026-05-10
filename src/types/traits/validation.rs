//! Purpose:
//! Implements trait validation logic for flattened class metadata.
//! Applies PHP trait composition rules before object inference and method checks consume class schemas.
//!
//! Called from:
//! - `crate::types::traits`
//!
//! Key details:
//! - Merge and validation rules must report conflicts early because downstream class metadata is treated as canonical.

use std::collections::HashSet;

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{ClassMethod, ClassProperty};
use crate::span::Span;

pub(super) fn validate_direct_members(
    properties: &[ClassProperty],
    methods: &[ClassMethod],
    span: Span,
    owner_name: &str,
) -> Result<(), CompileError> {
    let mut seen_props = HashSet::new();
    for property in properties {
        if !seen_props.insert(property.name.clone()) {
            return Err(CompileError::new(
                span,
                &format!("Duplicate property declaration in {}: {}", owner_name, property.name),
            ));
        }
    }
    validate_direct_method_duplicates(methods, span, owner_name)
}

pub(super) fn validate_direct_method_duplicates(
    methods: &[ClassMethod],
    span: Span,
    owner_name: &str,
) -> Result<(), CompileError> {
    let mut seen = HashSet::new();
    for method in methods {
        let key = (php_symbol_key(&method.name), method.is_static);
        if !seen.insert(key) {
            return Err(CompileError::new(
                span,
                &format!("Duplicate method declaration in {}: {}", owner_name, method.name),
            ));
        }
    }
    Ok(())
}
