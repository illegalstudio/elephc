//! Purpose:
//! Tracks local PHP reference groups while propagating receiver assignments.
//!
//! Called from:
//! - DOM receiver assignment, reference binding, and control-flow joins.
//!
//! Key details:
//! - Definite aliases share new facts; path-dependent alias relations stay conservative.

use std::collections::HashSet;
use crate::parser::ast::{Expr, ExprKind};
use super::{Classes, DomReceiver};

impl DomReceiver {
    /// Writes through every definite alias and invalidates possibly connected reference names.
    pub(super) fn write_reference_fact(&self, name: &str, classes: Classes) {
        let group = self.aliases.borrow().get(name).cloned().unwrap_or_else(|| HashSet::from([name.to_string()]));
        let uncertain = self.uncertain_aliases.borrow();
        let mut variables = self.variables.borrow_mut();
        let mut callables = self.callables.borrow_mut();
        if group.iter().any(|name| uncertain.contains(name)) {
            for other in uncertain.iter() {
                variables.insert(other.clone(), None);
                callables.remove(other);
            }
        }
        for alias in group {
            variables.insert(alias.clone(), classes.clone());
            callables.remove(&alias);
        }
    }

    /// Rebinds one local name to the source's reference group without retaining its old group.
    pub(super) fn bind_reference(&self, target: &str, source: &Expr) {
        if matches!(&source.kind, ExprKind::Variable(name) if name == target) { return; }
        let classes = self.classes(source);
        let signature = self.callable_signature(source);
        let mut aliases = self.aliases.borrow_mut();
        aliases.remove(target);
        for group in aliases.values_mut() { group.remove(target); }
        let mut group = if let ExprKind::Variable(name) = &source.kind {
            aliases.get(name).cloned().unwrap_or_else(|| HashSet::from([name.clone()]))
        } else { HashSet::new() };
        group.insert(target.to_string());
        for name in &group { aliases.insert(name.clone(), group.clone()); }
        drop(aliases);
        self.escaped_refs.borrow_mut().extend(group);
        self.assign(target, classes);
        self.set_callable(target, signature);
    }

    /// Shares a known callable signature with every name that aliases the same cell.
    pub(super) fn set_callable(&self, name: &str, signature: Option<crate::types::FunctionSig>) {
        let Some(signature) = signature else { return; };
        let group = self.aliases.borrow().get(name).cloned().unwrap_or_else(|| HashSet::from([name.to_string()]));
        for alias in group { self.callables.borrow_mut().insert(alias, signature.clone()); }
    }

    /// Keeps identical reference groups and marks relations that differ between paths as uncertain.
    pub(super) fn join_references(&self, other: &Self) -> bool {
        let before = self.aliases.borrow().clone();
        let uncertain_before = self.uncertain_aliases.borrow().clone();
        let right = other.aliases.borrow();
        let mut uncertain = self.uncertain_aliases.borrow_mut();
        uncertain.extend(other.uncertain_aliases.borrow().iter().cloned());
        for name in before.keys().chain(right.keys()) {
            if before.get(name) != right.get(name) {
                uncertain.insert(name.clone());
                if let Some(group) = before.get(name) { uncertain.extend(group.iter().cloned()); }
                if let Some(group) = right.get(name) { uncertain.extend(group.iter().cloned()); }
            }
        }
        self.aliases.borrow_mut().retain(|name, group| !group.iter().any(|alias| uncertain.contains(alias)) && !uncertain.contains(name));
        *self.aliases.borrow() != before || *uncertain != uncertain_before
    }
}
