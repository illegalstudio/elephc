//! Purpose:
//! Supplies conservative class-initialization throw facts when checker schemas are unavailable.
//!
//! Called from:
//! - `ExceptionFlowAnalysis::from_program`.
//!
//! Key details:
//! - A local empty constructor does not prove imported property defaults cannot throw.
//! - Metadata-aware optimization uses the precise ClassInfo error contract instead.

use super::*;

/// Retains possible property initialization errors through the AST-only compatibility API.
pub(super) fn imported_default_errors(program: &[Stmt]) -> HashSet<String> {
    let mut errors = HashSet::new();
    let mut parents = Vec::new();
    collect(program, &mut errors, &mut parents);
    loop {
        let mut changed = false;
        for (child, parent) in &parents {
            if errors.contains(parent) {
                changed |= errors.insert(child.clone());
            }
        }
        if !changed { break; }
    }
    errors
}

/// Collects consuming classes even when they occur in namespace blocks.
fn collect(program: &[Stmt], errors: &mut HashSet<String>, parents: &mut Vec<(String, String)>) {
    for statement in program {
        match &statement.kind {
            StmtKind::ClassDecl { name, trait_uses, extends, properties, .. } => {
                if !trait_uses.is_empty() || (extends.is_none()
                    && properties.iter().any(|property| property.default.is_some()))
                {
                    // Unresolved constant defaults may raise only when the class initializes.
                    errors.insert(php_symbol_key(name));
                }
                if let Some(parent) = extends {
                    parents.push((php_symbol_key(name), php_symbol_key(parent.as_str())));
                }
            }
            StmtKind::NamespaceBlock { body, .. } => collect(body, errors, parents),
            _ => {}
        }
    }
}
