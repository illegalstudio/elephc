use crate::names::{canonical_name_for_decl, php_symbol_key};
use crate::parser::ast::{Stmt, StmtKind};

use super::{canonical_builtin_function_name, namespace_name, Symbols};

const BUILTIN_CLASS_LIKE_SYMBOLS: &[&str] = &[
    "Throwable",
    "Exception",
    "Iterator",
    "IteratorAggregate",
];

impl Symbols {
    pub(super) fn canonical_function(&self, name: &str) -> Option<String> {
        let key = php_symbol_key(name);
        self.functions
            .get(&key)
            .or_else(|| self.extern_functions.get(&key))
            .cloned()
            .or_else(|| canonical_builtin_function_name(name))
    }

    pub(super) fn canonical_class_like(&self, name: &str) -> Option<String> {
        let key = php_symbol_key(name);
        self.classes
            .get(&key)
            .or_else(|| self.interfaces.get(&key))
            .or_else(|| self.traits.get(&key))
            .or_else(|| self.extern_classes.get(&key))
            .cloned()
            .or_else(|| {
                BUILTIN_CLASS_LIKE_SYMBOLS
                    .iter()
                    .find(|builtin| php_symbol_key(builtin) == key)
                    .map(|builtin| (*builtin).to_string())
            })
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
                insert_folded_symbol(
                    &mut symbols.functions,
                    canonical_name_for_decl(namespace.as_deref(), name),
                );
            }
            StmtKind::ClassDecl { name, .. }
            | StmtKind::EnumDecl { name, .. }
            | StmtKind::PackedClassDecl { name, .. } => {
                insert_folded_symbol(
                    &mut symbols.classes,
                    canonical_name_for_decl(namespace.as_deref(), name),
                );
            }
            StmtKind::InterfaceDecl { name, .. } => {
                insert_folded_symbol(
                    &mut symbols.interfaces,
                    canonical_name_for_decl(namespace.as_deref(), name),
                );
            }
            StmtKind::TraitDecl { name, .. } => {
                insert_folded_symbol(
                    &mut symbols.traits,
                    canonical_name_for_decl(namespace.as_deref(), name),
                );
            }
            StmtKind::ExternFunctionDecl { name, .. } => {
                insert_folded_symbol(
                    &mut symbols.extern_functions,
                    canonical_name_for_decl(namespace.as_deref(), name),
                );
            }
            StmtKind::ExternClassDecl { name, .. } => {
                insert_folded_symbol(
                    &mut symbols.extern_classes,
                    canonical_name_for_decl(namespace.as_deref(), name),
                );
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

fn insert_folded_symbol(symbols: &mut std::collections::HashMap<String, String>, name: String) {
    symbols.entry(php_symbol_key(&name)).or_insert(name);
}
