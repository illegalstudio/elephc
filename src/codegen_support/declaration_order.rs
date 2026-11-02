//! Purpose:
//! Tracks and reconstructs PHP class-like declaration order for introspection builtins.
//!
//! Called from:
//! - "crate::pipeline" before EIR lowering and code generation.
//!
//! Key details:
//! - Internal names are sorted and prepended; source declarations preserve PHP declaration order.

use crate::parser::ast::{Program, StmtKind};
use crate::types::{ClassInfo, InterfaceInfo};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

thread_local! {
    static DECLARED_CLASS_NAMES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static DECLARED_INTERFACE_NAMES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static DECLARED_TRAIT_NAMES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// Stores the declaration order of classes, interfaces, and traits so that
/// `declared_class_names()` / `declared_interface_names()` / `declared_trait_names()`
/// can reproduce it for class-id ordering in user assembly.
pub(crate) fn set_declared_name_order(
    classes: Vec<String>,
    interfaces: Vec<String>,
    traits: Vec<String>,
) {
    DECLARED_CLASS_NAMES.with(|names| *names.borrow_mut() = classes);
    DECLARED_INTERFACE_NAMES.with(|names| *names.borrow_mut() = interfaces);
    DECLARED_TRAIT_NAMES.with(|names| *names.borrow_mut() = traits);
}

/// Prepares declaration-order registries shared by EIR introspection builtins.
pub fn prepare_declared_name_order(
    program: &Program,
    classes: &crate::fast_hash::FastMap<String, ClassInfo>,
    interfaces: &HashMap<String, InterfaceInfo>,
) {
    let declared_trait_order = collect_declared_trait_names(program);
    set_declared_name_order(
        collect_declared_class_names(program, classes),
        collect_declared_interface_names(program, interfaces),
        declared_trait_order,
    );
}

/// Returns the ordered list of class names declared in the program,
/// including internal classes prepended by the compiler.
pub(crate) fn declared_class_names() -> Vec<String> {
    DECLARED_CLASS_NAMES.with(|names| names.borrow().clone())
}

/// Returns the ordered list of interface names declared in the program,
/// including internal interfaces prepended by the compiler.
pub(crate) fn declared_interface_names() -> Vec<String> {
    DECLARED_INTERFACE_NAMES.with(|names| names.borrow().clone())
}

/// Returns the ordered list of trait names declared in the program,
/// including internal traits prepended by the compiler.
pub(crate) fn declared_trait_names() -> Vec<String> {
    DECLARED_TRAIT_NAMES.with(|names| names.borrow().clone())
}

/// Collects user-declared class and enum names from the program AST, merges them
/// with internal class names, and returns the combined list in declaration order
/// with internal names prepended and sorted.
fn collect_declared_class_names(
    program: &Program,
    classes: &crate::fast_hash::FastMap<String, ClassInfo>,
) -> Vec<String> {
    let mut user_names = Vec::new();
    collect_program_declared_names(
        program,
        classes,
        &mut HashSet::new(),
        &mut user_names,
        |stmt| match &stmt.kind {
            StmtKind::ClassDecl { name, .. } | StmtKind::EnumDecl { name, .. } => {
                Some(name.as_str())
            }
            _ => None,
        },
    );
    prepend_internal_names(classes.keys(), &user_names)
}

/// Collects user-declared interface names from the program AST, merges them
/// with internal interface names, and returns the combined list in declaration
/// order with internal names prepended and sorted.
fn collect_declared_interface_names(
    program: &Program,
    interfaces: &HashMap<String, InterfaceInfo>,
) -> Vec<String> {
    let mut user_names = Vec::new();
    collect_program_declared_names(
        program,
        interfaces,
        &mut HashSet::new(),
        &mut user_names,
        |stmt| match &stmt.kind {
            StmtKind::InterfaceDecl { name, .. } => Some(name.as_str()),
            _ => None,
        },
    );
    prepend_internal_names(interfaces.keys(), &user_names)
}

/// Recursively collects user-declared trait names from the program AST,
/// including those inside namespace blocks, and returns them in declaration order.
fn collect_declared_trait_names(program: &Program) -> Vec<String> {
    let mut names = Vec::new();
    for stmt in program {
        match &stmt.kind {
            StmtKind::TraitDecl { name, .. } => {
                names.push(name.clone());
            }
            StmtKind::NamespaceBlock { body, .. } => {
                names.extend(collect_declared_trait_names(body));
            }
            _ => {}
        }
    }
    names
}

/// Helper for collecting declared names of a specific AST statement kind.
/// Walks the program (recursing into namespace blocks), asks the `pick` callback
/// to extract a name from each statement, and outputs it only if it exists in
/// `known` and hasn't been seen before (deduplicated by PHP symbol key).
fn collect_program_declared_names<T, S: std::hash::BuildHasher>(
    program: &Program,
    known: &std::collections::HashMap<String, T, S>,
    seen: &mut HashSet<String>,
    out: &mut Vec<String>,
    pick: impl Copy + Fn(&crate::parser::ast::Stmt) -> Option<&str>,
) {
    for stmt in program {
        match &stmt.kind {
            StmtKind::NamespaceBlock { body, .. } => {
                collect_program_declared_names(body, known, seen, out, pick);
            }
            _ => {
                let Some(name) = pick(stmt) else {
                    continue;
                };
                let key = crate::names::php_symbol_key(name);
                let is_known = known.contains_key(name)
                    || known.keys().any(|candidate| {
                        crate::names::php_symbol_key(candidate.trim_start_matches('\\')) == key
                    });
                if is_known && seen.insert(key) {
                    out.push(name.to_string());
                }
            }
        }
    }
}

/// Splits `known_names` into internal-only and user-declared by checking against
/// `user_names` (matched by PHP symbol key), sorts the internal names, and
/// appends the user names in their original order.
fn prepend_internal_names<'a>(
    known_names: impl Iterator<Item = &'a String>,
    user_names: &[String],
) -> Vec<String> {
    let user_keys: HashSet<String> = user_names
        .iter()
        .map(|name| crate::names::php_symbol_key(name))
        .collect();
    let mut names: Vec<String> = known_names
        .filter(|name| !is_internal_synthetic_class_name(name))
        .filter(|name| !user_keys.contains(&crate::names::php_symbol_key(name)))
        .cloned()
        .collect();
    names.sort();
    names.extend(user_names.iter().cloned());
    names
}

/// Returns true when internal synthetic class name.
fn is_internal_synthetic_class_name(name: &str) -> bool {
    crate::names::php_symbol_key(name).starts_with("__elephc")
}
