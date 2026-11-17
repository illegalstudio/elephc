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

/// Classes whose initialization may raise a deprecation, found from the AST when no checked
/// class metadata exists: a property default or constant reads a deprecated global constant or
/// any class constant, in the class or an ancestor (see `initialization_may_report`).
pub(super) fn imported_initialization_reports(program: &[Stmt]) -> HashSet<String> {
    /// Collects reporting classes and parent links, entering namespace blocks.
    fn collect(program: &[Stmt], reports: &mut HashSet<String>, parents: &mut Vec<(String, String)>) {
        for statement in program {
            match &statement.kind {
                StmtKind::ClassDecl { name, extends, properties, constants, .. } => {
                    if properties.iter().filter_map(|property| property.default.as_ref()).any(super::reads_reporting_constant)
                        || constants.iter().any(|constant| super::reads_reporting_constant(&constant.value))
                    {
                        reports.insert(php_symbol_key(name));
                    }
                    if let Some(parent) = extends {
                        parents.push((php_symbol_key(name), php_symbol_key(parent.as_str())));
                    }
                }
                StmtKind::NamespaceBlock { body, .. } => collect(body, reports, parents),
                _ => {}
            }
        }
    }
    let mut reports = HashSet::new();
    let mut parents = Vec::new();
    collect(program, &mut reports, &mut parents);
    loop {
        let mut changed = false;
        for (child, parent) in &parents {
            if reports.contains(parent) {
                changed |= reports.insert(child.clone());
            }
        }
        if !changed {
            break;
        }
    }
    reports
}
