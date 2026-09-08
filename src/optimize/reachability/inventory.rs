//! Purpose:
//! Records declarations contributed by compiler-provided PHP preludes.
//! Lets `--with-<crate>` force a complete injected surface without hard-coded symbol lists.
//!
//! Called from:
//! - Prelude injectors and `crate::optimize::reachability::graph`.
//!
//! Key details:
//! - Recording happens only when a prelude is actually injected.
//! - Names are canonicalized with PHP case-insensitive keys and namespace context.

use std::collections::{HashMap, HashSet};

use crate::names::{canonical_name_for_decl, php_symbol_key};
use crate::parser::ast::{Stmt, StmtKind};

/// Declaration groups keyed by stable prelude identifiers such as `pdo` and `web`.
#[derive(Clone, Debug, Default)]
pub struct PreludeInventory {
    pub groups: HashMap<String, PreludeGroup>,
}

/// Canonical declarations contributed by one injected prelude.
#[derive(Clone, Debug, Default)]
pub struct PreludeGroup {
    pub id: String,
    pub functions: HashSet<String>,
    pub classes: HashSet<String>,
    pub methods: HashSet<(String, String, bool)>,
    pub externs: HashSet<String>,
    pub internal_callable_methods: HashSet<(String, String, bool)>,
}

impl PreludeInventory {
    /// Creates an empty prelude declaration inventory.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the mutable group for `id`, creating it with the same stable identifier if absent.
    pub fn group_mut(&mut self, id: &str) -> &mut PreludeGroup {
        self.groups
            .entry(id.to_string())
            .or_insert_with(|| PreludeGroup {
                id: id.to_string(),
                ..PreludeGroup::default()
            })
    }

    /// Records every declaration in one parsed prelude using its lexical namespace context.
    pub fn record_program(&mut self, id: &str, prelude: &[Stmt]) {
        let group = self.group_mut(id);
        record_statements(group, prelude, None);
    }

    /// Marks one compiler-owned method whose dynamic-call syntax only invokes closures created internally.
    pub fn record_internal_callable_method(
        &mut self,
        id: &str,
        class: &str,
        method: &str,
        is_static: bool,
    ) {
        let key = (php_symbol_key(class), php_symbol_key(method), is_static);
        let group = self.group_mut(id);
        debug_assert!(
            group.methods.contains(&key),
            "internal-callable method must belong to its recorded prelude group"
        );
        group.internal_callable_methods.insert(key);
    }

    /// Returns all compiler-owned methods whose internal closure calls are not user lookup hazards.
    pub(crate) fn internal_callable_methods(&self) -> HashSet<(String, String, bool)> {
        self.groups
            .values()
            .flat_map(|group| group.internal_callable_methods.iter().cloned())
            .collect()
    }
}

/// Records declarations in one statement list, carrying semicolon-style namespace state forward.
fn record_statements(group: &mut PreludeGroup, statements: &[Stmt], namespace: Option<&str>) {
    let mut active_namespace = namespace.map(str::to_string);
    for statement in statements {
        match &statement.kind {
            StmtKind::NamespaceDecl { name } => {
                active_namespace = name.as_ref().map(|name| name.as_str().to_string());
            }
            StmtKind::NamespaceBlock { name, body } => {
                record_statements(group, body, name.as_ref().map(|name| name.as_str()));
            }
            StmtKind::Synthetic(body) | StmtKind::IncludeOnceGuard { body, .. } => {
                record_statements(group, body, active_namespace.as_deref());
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                record_statements(group, then_body, active_namespace.as_deref());
                for (_, body) in elseif_clauses {
                    record_statements(group, body, active_namespace.as_deref());
                }
                if let Some(body) = else_body {
                    record_statements(group, body, active_namespace.as_deref());
                }
            }
            StmtKind::IfDef {
                then_body,
                else_body,
                ..
            } => {
                record_statements(group, then_body, active_namespace.as_deref());
                if let Some(body) = else_body {
                    record_statements(group, body, active_namespace.as_deref());
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::Foreach { body, .. } => {
                record_statements(group, body, active_namespace.as_deref());
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    record_statements(group, body, active_namespace.as_deref());
                }
                if let Some(body) = default {
                    record_statements(group, body, active_namespace.as_deref());
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                record_statements(group, try_body, active_namespace.as_deref());
                for catch in catches {
                    record_statements(group, &catch.body, active_namespace.as_deref());
                }
                if let Some(body) = finally_body {
                    record_statements(group, body, active_namespace.as_deref());
                }
            }
            StmtKind::FunctionDecl { name, .. } => {
                group.functions.insert(declaration_key(
                    active_namespace.as_deref(),
                    name,
                ));
            }
            StmtKind::ExternFunctionDecl { name, .. } => {
                group.externs.insert(declaration_key(
                    active_namespace.as_deref(),
                    name,
                ));
            }
            StmtKind::ClassDecl { name, methods, .. }
            | StmtKind::EnumDecl { name, methods, .. }
            | StmtKind::InterfaceDecl { name, methods, .. }
            | StmtKind::TraitDecl { name, methods, .. } => {
                let class_key = declaration_key(active_namespace.as_deref(), name);
                group.classes.insert(class_key.clone());
                for method in methods {
                    group.methods.insert((
                        class_key.clone(),
                        php_symbol_key(&method.name),
                        method.is_static,
                    ));
                }
            }
            StmtKind::PackedClassDecl { name, .. }
            | StmtKind::ExternClassDecl { name, .. } => {
                group.classes.insert(declaration_key(
                    active_namespace.as_deref(),
                    name,
                ));
            }
            StmtKind::Echo(_)
            | StmtKind::Assign { .. }
            | StmtKind::RefAssign { .. }
            | StmtKind::ArrayAssign { .. }
            | StmtKind::NestedArrayAssign { .. }
            | StmtKind::ArrayPush { .. }
            | StmtKind::TypedAssign { .. }
            | StmtKind::Throw(_)
            | StmtKind::Break(_)
            | StmtKind::Continue(_)
            | StmtKind::ExprStmt(_)
            | StmtKind::UseDecl { .. }
            | StmtKind::FunctionVariantGroup { .. }
            | StmtKind::FunctionVariantMark { .. }
            | StmtKind::Return(_)
            | StmtKind::ConstDecl { .. }
            | StmtKind::ListUnpack { .. }
            | StmtKind::Global { .. }
            | StmtKind::StaticVar { .. }
            | StmtKind::PropertyAssign { .. }
            | StmtKind::DynamicPropertyArrayPush { .. }
            | StmtKind::StaticPropertyAssign { .. }
            | StmtKind::StaticPropertyArrayPush { .. }
            | StmtKind::StaticPropertyArrayAssign { .. }
            | StmtKind::PropertyArrayPush { .. }
            | StmtKind::PropertyArrayAssign { .. }
            | StmtKind::ExternGlobalDecl { .. }
            | StmtKind::Include { .. }
            | StmtKind::IncludeOnceMark { .. } => {}
        }
    }
}

/// Produces the canonical PHP lookup key for a declaration in a lexical namespace.
fn declaration_key(namespace: Option<&str>, name: &str) -> String {
    php_symbol_key(&canonical_name_for_decl(namespace, name))
}
