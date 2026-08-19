//! Purpose:
//! Models throwable class and interface relationships for exception-aware optimization.
//! Combines checker metadata with source declarations needed for conservative constructor analysis.
//!
//! Called from:
//! - `crate::optimize::exception_flow::ExceptionFlowAnalysis`
//!
//! Key details:
//! - Symbol comparisons are case-insensitive while stored names retain their canonical spelling.
//! - Trait users form constructor barriers because a trait may supply the effective constructor.

use super::php_symbol_key;
use crate::parser::ast::{Stmt, StmtKind};
use crate::types::{ClassInfo, InterfaceInfo};
use std::collections::{HashMap, HashSet};

/// Canonical class/interface relations used to compare thrown and caught types.
#[derive(Clone, Debug, Default)]
pub(super) struct ExceptionHierarchy {
    pub(super) parents: HashMap<String, String>,
    interfaces: HashMap<String, HashSet<String>>,
    interface_parents: HashMap<String, HashSet<String>>,
    class_names: HashSet<String>,
    interface_names: HashSet<String>,
    declared_classes: HashSet<String>,
    trait_method_barriers: HashSet<String>,
}

impl ExceptionHierarchy {
    /// Builds authoritative hierarchy facts from type-checker metadata.
    pub(super) fn from_type_metadata(
        classes: &HashMap<String, ClassInfo>,
        interfaces: &HashMap<String, InterfaceInfo>,
        declared_classes: HashSet<String>,
    ) -> Self {
        let mut hierarchy = Self {
            declared_classes,
            ..Self::default()
        };
        for (name, info) in classes {
            let key = php_symbol_key(name);
            hierarchy.class_names.insert(key.clone());
            if let Some(parent) = &info.parent {
                hierarchy.parents.insert(key.clone(), parent.clone());
            }
            hierarchy
                .interfaces
                .insert(key, info.interfaces.iter().cloned().collect());
        }
        for (name, info) in interfaces {
            let key = php_symbol_key(name);
            hierarchy.interface_names.insert(key.clone());
            hierarchy
                .interface_parents
                .insert(key, info.parents.iter().cloned().collect());
        }
        hierarchy.add_throwable_roots();
        hierarchy
    }

    /// Builds best-effort hierarchy facts directly from AST declarations for public test helpers.
    pub(super) fn from_program(program: &[Stmt], declared_classes: HashSet<String>) -> Self {
        let mut hierarchy = Self {
            declared_classes,
            ..Self::default()
        };
        hierarchy.collect_program_declarations(program);
        hierarchy.add_throwable_roots();
        hierarchy
    }

    /// Adds the PHP root throwable relations needed even when no checker metadata is supplied.
    fn add_throwable_roots(&mut self) {
        self.interface_names.insert(php_symbol_key("Throwable"));
        for root in ["Exception", "Error"] {
            let key = php_symbol_key(root);
            self.class_names.insert(key.clone());
            self.interfaces
                .entry(key)
                .or_default()
                .insert("Throwable".to_string());
        }
    }

    /// Recursively collects class and interface declarations from namespace/grouping blocks.
    pub(super) fn collect_program_declarations(&mut self, stmts: &[Stmt]) {
        for stmt in stmts {
            match &stmt.kind {
                StmtKind::ClassDecl {
                    name,
                    extends,
                    implements,
                    trait_uses,
                    ..
                } => {
                    let key = php_symbol_key(name);
                    self.class_names.insert(key.clone());
                    if !trait_uses.is_empty() {
                        self.trait_method_barriers.insert(key.clone());
                    }
                    if let Some(parent) = extends {
                        self.parents.insert(key.clone(), parent.as_str().to_string());
                    }
                    self.interfaces.insert(
                        key,
                        implements
                            .iter()
                            .map(|name| name.as_str().to_string())
                            .collect(),
                    );
                }
                StmtKind::InterfaceDecl { name, extends, .. } => {
                    let key = php_symbol_key(name);
                    self.interface_names.insert(key.clone());
                    self.interface_parents.insert(
                        key,
                        extends
                            .iter()
                            .map(|name| name.as_str().to_string())
                            .collect(),
                    );
                }
                StmtKind::NamespaceBlock { body, .. } | StmtKind::Synthetic(body) => {
                    self.collect_program_declarations(body);
                }
                _ => {}
            }
        }
    }

    /// Returns whether `candidate` is the same as, extends, or implements `base`.
    pub(super) fn is_subtype(&self, candidate: &str, base: &str) -> bool {
        let candidate_key = php_symbol_key(candidate);
        let base_key = php_symbol_key(base);
        if candidate_key == base_key {
            return true;
        }
        if base_key == php_symbol_key("Throwable")
            && (self.class_names.contains(&candidate_key)
                || self.interface_names.contains(&candidate_key))
        {
            return self.type_reaches_interface(&candidate_key, &base_key)
                || self.class_reaches_root(&candidate_key, "Exception")
                || self.class_reaches_root(&candidate_key, "Error");
        }
        if self.class_reaches_root(&candidate_key, base) {
            return true;
        }
        self.type_reaches_interface(&candidate_key, &base_key)
    }

    /// Walks a class parent chain looking for `base`.
    fn class_reaches_root(&self, candidate_key: &str, base: &str) -> bool {
        let base_key = php_symbol_key(base);
        let mut current = Some(candidate_key.to_string());
        let mut seen = HashSet::new();
        while let Some(class_key) = current {
            if !seen.insert(class_key.clone()) {
                return false;
            }
            if class_key == base_key {
                return true;
            }
            current = self
                .parents
                .get(&class_key)
                .map(|parent| php_symbol_key(parent));
        }
        false
    }

    /// Walks class implementations, parent classes, and interface parents for a target interface.
    fn type_reaches_interface(&self, candidate_key: &str, base_key: &str) -> bool {
        let mut pending = vec![candidate_key.to_string()];
        let mut seen = HashSet::new();
        while let Some(current) = pending.pop() {
            if !seen.insert(current.clone()) {
                continue;
            }
            if current == base_key {
                return true;
            }
            if let Some(parent) = self.parents.get(&current) {
                pending.push(php_symbol_key(parent));
            }
            if let Some(interfaces) = self.interfaces.get(&current) {
                pending.extend(interfaces.iter().map(|name| php_symbol_key(name)));
            }
            if let Some(parents) = self.interface_parents.get(&current) {
                pending.extend(parents.iter().map(|name| php_symbol_key(name)));
            }
        }
        false
    }

    /// Returns whether two upper-bound types can contain at least one common runtime class.
    pub(super) fn types_overlap(&self, left: &str, right: &str) -> bool {
        if self.is_subtype(left, right) || self.is_subtype(right, left) {
            return true;
        }
        let left_key = php_symbol_key(left);
        let right_key = php_symbol_key(right);
        if self.class_names.contains(&left_key) && self.class_names.contains(&right_key) {
            return false;
        }
        true
    }

    /// Returns whether a class came from user/source AST rather than injected builtin metadata.
    pub(super) fn is_declared_class(&self, class_name: &str) -> bool {
        self.declared_classes.contains(&php_symbol_key(class_name))
    }

    /// Returns whether a trait may provide a method absent from the class's explicit method list.
    pub(super) fn class_has_trait_method_barrier(&self, class_name: &str) -> bool {
        self.trait_method_barriers
            .contains(&php_symbol_key(class_name))
    }

    /// Returns whether constructor lookup is closed over source classes or builtin throwables.
    pub(super) fn constructor_hierarchy_is_closed(&self, class_name: &str) -> bool {
        let mut current = Some(php_symbol_key(class_name));
        let mut seen = HashSet::new();
        while let Some(class_key) = current {
            if !seen.insert(class_key.clone()) {
                return false;
            }
            if self.trait_method_barriers.contains(&class_key) {
                return false;
            }
            if !self.declared_classes.contains(&class_key)
                && !self.is_subtype(&class_key, "Throwable")
            {
                return false;
            }
            current = self
                .parents
                .get(&class_key)
                .map(|parent| php_symbol_key(parent));
        }
        true
    }
}
