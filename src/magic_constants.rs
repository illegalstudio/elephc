//! Lowering of PHP magic constants (`__DIR__`, `__FILE__`, `__FUNCTION__`,
//! `__CLASS__`, `__METHOD__`, `__NAMESPACE__`, `__TRAIT__`) to plain string
//! literals before the type checker and codegen run. `__LINE__` is already
//! lowered at parse time (see `parser::expr::prefix`).
//!
//! Public passes:
//! - [`substitute_file_constants`] resolves `__FILE__` and `__DIR__` against
//!   the canonical path of the file the AST nodes came from. Run once per
//!   source file before inlining (resolver) and once for the main file.
//! - [`substitute_scope_constants_in_file`] resolves the scope-dependent
//!   constants (`__FUNCTION__`, `__CLASS__`, `__METHOD__`, `__NAMESPACE__`,
//!   `__TRAIT__`) based on lexical position inside a single source file.
//! - [`substitute_file_and_scope_constants`] applies both passes for a single
//!   source file before that file is inlined into another file.
//! - [`bind_trait_class_constants`] rebinds trait-origin `__CLASS__` literals
//!   when trait members are flattened into a concrete class. `__METHOD__` and
//!   `__TRAIT__` keep the trait identity, matching PHP.

mod file_pass;
mod scope_pass;
mod trait_binding;
mod walker;

use std::path::Path;

use crate::names::Name;
use crate::parser::ast::{ClassMethod, ClassProperty, Program, Stmt};

const TRAIT_CLASS_PLACEHOLDER: &str = "\x1F__ELEPHC_TRAIT_CLASS__\x1F";

/// Replaces `MagicConstant::File` and `MagicConstant::Dir` with string
/// literals derived from `file_path`. Other magic constants are left untouched
/// for the scope pass to resolve later.
pub fn substitute_file_constants(stmts: Vec<Stmt>, file_path: &Path) -> Vec<Stmt> {
    file_pass::substitute_file_constants(stmts, file_path)
}

/// Applies file-local and lexical-scope magic-constant lowering for one PHP
/// source file. Resolver calls this before inlining included files so lexical
/// scopes from one file cannot leak into another.
pub fn substitute_file_and_scope_constants(stmts: Vec<Stmt>, file_path: &Path) -> Vec<Stmt> {
    let stmts = substitute_file_constants(stmts, file_path);
    substitute_scope_constants_in_file(stmts, file_path)
}

pub fn substitute_scope_constants_in_file(program: Program, file_path: &Path) -> Program {
    scope_pass::substitute_scope_constants_in_file(program, file_path)
}

pub fn bind_trait_class_constants(
    properties: Vec<ClassProperty>,
    methods: Vec<ClassMethod>,
    class_name: &str,
) -> (Vec<ClassProperty>, Vec<ClassMethod>) {
    trait_binding::bind_trait_class_constants(properties, methods, class_name)
}

fn namespace_string(name: &Option<Name>) -> String {
    name.as_ref().map(Name::as_canonical).unwrap_or_default()
}

fn qualify(namespace: Option<&str>, name: &str) -> String {
    match namespace {
        Some(ns) if !ns.is_empty() => format!("{}\\{}", ns, name),
        _ => name.to_string(),
    }
}
