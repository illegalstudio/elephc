use crate::names::canonical_name_for_decl;
use crate::parser::ast::{Stmt, StmtKind};

use super::{is_builtin_function, namespace_name, Symbols};

impl Symbols {
    pub(super) fn has_function(&self, name: &str) -> bool {
        self.functions.contains(name)
            || self.extern_functions.contains(name)
            || is_builtin_function(name)
    }

    pub(super) fn has_constant(&self, name: &str) -> bool {
        self.constants.contains(name)
    }
}

pub(super) fn collect_symbols(
    stmts: &[Stmt],
    current_namespace: Option<&str>,
    symbols: &mut Symbols,
) {
    let mut namespace = current_namespace.map(str::to_string);
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::NamespaceDecl { name } => {
                namespace = Some(namespace_name(name));
            }
            StmtKind::NamespaceBlock { name, body } => {
                let block_namespace = Some(namespace_name(name));
                collect_symbols(body, block_namespace.as_deref(), symbols);
            }
            StmtKind::FunctionDecl { name, .. } => {
                symbols
                    .functions
                    .insert(canonical_name_for_decl(namespace.as_deref(), name));
            }
            StmtKind::ClassDecl { name, .. }
            | StmtKind::EnumDecl { name, .. }
            | StmtKind::PackedClassDecl { name, .. } => {
                symbols
                    .classes
                    .insert(canonical_name_for_decl(namespace.as_deref(), name));
            }
            StmtKind::InterfaceDecl { name, .. } => {
                symbols
                    .interfaces
                    .insert(canonical_name_for_decl(namespace.as_deref(), name));
            }
            StmtKind::TraitDecl { name, .. } => {
                symbols
                    .traits
                    .insert(canonical_name_for_decl(namespace.as_deref(), name));
            }
            StmtKind::ExternFunctionDecl { name, .. } => {
                symbols
                    .extern_functions
                    .insert(canonical_name_for_decl(namespace.as_deref(), name));
            }
            StmtKind::ExternClassDecl { name, .. } => {
                symbols
                    .extern_classes
                    .insert(canonical_name_for_decl(namespace.as_deref(), name));
            }
            StmtKind::ConstDecl { name, .. } => {
                symbols
                    .constants
                    .insert(canonical_name_for_decl(namespace.as_deref(), name));
            }
            _ => {}
        }
    }
}
