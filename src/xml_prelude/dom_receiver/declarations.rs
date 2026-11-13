//! Purpose:
//! Collects DOM receiver annotations and callable reference signatures before detection.
//!
//! Called from:
//! - The XML receiver detector's root scope construction.
//!
//! Key details:
//! - Conditional declarations retain their identity even before executing their bodies.

use std::collections::{HashMap, HashSet};
use crate::names::php_symbol_key;
use crate::parser::ast::{Stmt, StmtKind};
use crate::types::FunctionSig;
use super::{class_key, type_classes, Classes};

/// Declaration facts shared by all lexical receiver scopes.
#[derive(Default)]
pub(super) struct Declarations {
    pub(super) classes: HashSet<String>,
    pub(super) open_receivers: HashSet<String>,
    pub(super) parents: HashMap<String, String>,
    pub(super) properties: HashMap<(String, String), Classes>,
    pub(super) returns: HashMap<String, Classes>,
    pub(super) functions: HashMap<String, FunctionSig>,
    pub(super) methods: HashMap<(String, String), FunctionSig>,
    pub(super) globals: HashSet<String>,
}

/// Visits every declaration-bearing statement body, including alternatives and loops.
pub(super) fn collect_declarations(program: &[Stmt], declarations: &mut Declarations) {
    for stmt in program {
        match &stmt.kind {
            StmtKind::Global { vars } => declarations.globals.extend(vars.iter().cloned()),
            StmtKind::ClassDecl { name, extends, properties, methods, .. } => {
                let class = php_symbol_key(name);
                declarations.classes.insert(class.clone());
                if let Some(parent) = extends { declarations.parents.insert(class.clone(), class_key(parent)); }
                for property in properties {
                    declarations.properties.insert((class.clone(), property.name.clone()), property.type_expr.as_ref().and_then(type_classes));
                }
                for method in methods {
                    declarations.methods.insert((class.clone(), php_symbol_key(&method.name)), super::signatures::signature(&method.params, method.variadic.as_deref(), method.variadic_by_ref));
                    collect_declarations(&method.body, declarations);
                }
            }
            StmtKind::EnumDecl { name, methods, .. } | StmtKind::TraitDecl { name, methods, .. }
            | StmtKind::InterfaceDecl { name, methods, .. } => {
                let class = php_symbol_key(name);
                declarations.classes.insert(class.clone());
                if matches!(&stmt.kind, StmtKind::TraitDecl { .. } | StmtKind::InterfaceDecl { .. }) {
                    // Traits can be used by DOMDocument subclasses, which may implement interfaces.
                    declarations.open_receivers.insert(class.clone());
                }
                for method in methods {
                    declarations.methods.insert((class.clone(), php_symbol_key(&method.name)), super::signatures::signature(&method.params, method.variadic.as_deref(), method.variadic_by_ref));
                    collect_declarations(&method.body, declarations);
                }
            }
            StmtKind::FunctionDecl { name, params, variadic, variadic_by_ref, return_type, body, .. } => {
                declarations.returns.insert(php_symbol_key(name), return_type.as_ref().and_then(type_classes));
                declarations.functions.insert(php_symbol_key(name), super::signatures::signature(params, variadic.as_deref(), *variadic_by_ref));
                collect_declarations(body, declarations);
            }
            StmtKind::Synthetic(body) | StmtKind::NamespaceBlock { body, .. }
            | StmtKind::IncludeOnceGuard { body, .. } | StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. } | StmtKind::Foreach { body, .. } => collect_declarations(body, declarations),
            StmtKind::If { then_body, elseif_clauses, else_body, .. } => {
                collect_declarations(then_body, declarations);
                for (_, body) in elseif_clauses { collect_declarations(body, declarations); }
                if let Some(body) = else_body { collect_declarations(body, declarations); }
            }
            StmtKind::IfDef { then_body, else_body, .. } => {
                collect_declarations(then_body, declarations);
                if let Some(body) = else_body { collect_declarations(body, declarations); }
            }
            StmtKind::For { init, update, body, .. } => {
                if let Some(stmt) = init { collect_declarations(std::slice::from_ref(stmt.as_ref()), declarations); }
                if let Some(stmt) = update { collect_declarations(std::slice::from_ref(stmt.as_ref()), declarations); }
                collect_declarations(body, declarations);
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases { collect_declarations(body, declarations); }
                if let Some(body) = default { collect_declarations(body, declarations); }
            }
            StmtKind::Try { try_body, catches, finally_body } => {
                collect_declarations(try_body, declarations);
                for catch in catches { collect_declarations(&catch.body, declarations); }
                if let Some(body) = finally_body { collect_declarations(body, declarations); }
            }
            _ => {}
        }
    }
}
