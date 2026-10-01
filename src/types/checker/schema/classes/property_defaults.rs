//! Purpose:
//! Validates lexical receivers in trait properties and constructor-promoted defaults.
//!
//! Called from:
//! - `crate::types::checker::driver` before trait flattening.
//! - `super::properties::apply_properties` during class schema construction.
//!
//! Key details:
//! - Traits must reject late-static defaults even when they have no consumers.
//! - A trait's parent receiver remains unresolved until a consuming class binds its scope.

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{ClassMethod, ClassProperty, Program, StmtKind};

/// Validates defaults on every original trait declaration, including unused declarations.
pub(crate) fn validate_trait_property_defaults(program: &Program) -> Vec<CompileError> {
    let mut errors = Vec::new();
    for statement in program {
        match &statement.kind {
            StmtKind::TraitDecl { name, properties, methods, .. } => {
                // This parent is only a validation placeholder; the rewritten tree is discarded.
                // Traits have no parent of their own, so binding it here would reject valid PHP.
                for property in properties {
                    if let Some(default) = &property.default {
                        if let Err(error) = super::constants::validate_property_default_in_scope(
                            default, name, Some(name),
                        ) {
                            errors.extend(error.flatten());
                        }
                    }
                }
                if let Err(error) = validate_promoted_defaults(properties, methods, name, Some(name)) {
                    errors.extend(error.flatten());
                }
            }
            StmtKind::NamespaceBlock { body, .. } => {
                errors.extend(validate_trait_property_defaults(body));
            }
            _ => {}
        }
    }
    errors
}

/// Checks promoted initializers stored on constructor parameters instead of property defaults.
pub(super) fn validate_promoted_defaults(
    properties: &[ClassProperty],
    methods: &[ClassMethod],
    class_name: &str,
    parent_name: Option<&str>,
) -> Result<(), CompileError> {
    for method in methods.iter().filter(|method| php_symbol_key(&method.name) == "__construct") {
        for (name, _, default, _) in &method.params {
            if !properties.iter().any(|property| property.is_promoted && property.name == *name) {
                continue;
            }
            if let Some(default) = default {
                super::constants::validate_property_default_in_scope(default, class_name, parent_name)?;
            }
        }
    }
    Ok(())
}
