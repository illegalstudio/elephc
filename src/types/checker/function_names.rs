//! Purpose:
//! Maintains case-folded names alongside the checker's mutable declaration tables.
//!
//! Called from:
//! - Declaration collection, lazy signature publication, generic instantiation, and callable lookup.
//!
//! Key details:
//! - Publication/removal updates names; signature value edits keep the same index key.
//! - Lookup returns a name, never a cached signature or divergence verdict.

use std::collections::HashMap;
use crate::names::php_symbol_key;
use crate::types::{ExternFunctionSig, FunctionSig};
use super::{Checker, FnDecl};

/// Parallel indices preserve each table's lookup priority and declaration spelling.
#[derive(Default)]
pub(super) struct FunctionNameIndices {
    signatures: HashMap<String, String>,
    declarations: HashMap<String, String>,
    variants: HashMap<String, String>,
    externs: HashMap<String, String>,
}

/// Resolves one table without scanning its unrelated declarations.
fn indexed_key<T>(map: &HashMap<String, T>, index: &HashMap<String, String>, name: &str, folded: &str) -> Option<String> {
    if map.contains_key(name) { return Some(name.to_string()); }
    index.get(folded).filter(|canonical| map.contains_key(*canonical)).cloned()
}

impl Checker {
    /// Publishes a signature and its spelling before any lazy caller observes it.
    pub(in crate::types::checker) fn publish_function_signature(&mut self, name: String, signature: FunctionSig) -> Option<FunctionSig> {
        self.function_name_indices.signatures.insert(php_symbol_key(&name), name.clone());
        self.functions.insert(name, signature)
    }

    /// Retires a temporary signature while preserving any declaration or variant fallback.
    pub(in crate::types::checker) fn remove_function_signature(&mut self, name: &str) -> Option<FunctionSig> {
        self.function_name_indices.signatures.remove(&php_symbol_key(name));
        self.functions.remove(name)
    }

    /// Publishes source and instantiated declarations through the same name contract.
    pub(in crate::types::checker) fn publish_function_declaration(&mut self, name: String, declaration: FnDecl) -> Option<FnDecl> {
        self.function_name_indices.declarations.insert(php_symbol_key(&name), name.clone());
        self.fn_decls.insert(name, declaration)
    }

    /// Registers the public spelling of an include-loaded or instantiated variant group.
    pub(in crate::types::checker) fn publish_function_variants(&mut self, name: String, variants: Vec<String>) -> Option<Vec<String>> {
        self.function_name_indices.variants.insert(php_symbol_key(&name), name.clone());
        self.function_variant_groups.insert(name, variants)
    }

    /// Publishes external ABI metadata without losing its source-level spelling.
    pub(in crate::types::checker) fn publish_extern_function(&mut self, name: String, signature: ExternFunctionSig) -> Option<ExternFunctionSig> {
        self.function_name_indices.externs.insert(php_symbol_key(&name), name.clone());
        self.extern_functions.insert(name, signature)
    }

    /// Reads the current signature table first, then variant and declaration names.
    pub(in crate::types::checker) fn indexed_function_name(&self, name: &str) -> Option<String> {
        if self.functions.contains_key(name) { return Some(name.to_string()); }
        let folded = php_symbol_key(name);
        indexed_key(&self.functions, &self.function_name_indices.signatures, name, &folded)
            .or_else(|| indexed_key(&self.function_variant_groups, &self.function_name_indices.variants, name, &folded))
            .or_else(|| indexed_key(&self.fn_decls, &self.function_name_indices.declarations, name, &folded))
    }

    /// Resolves a source declaration, variant group, or external name independently of specialization.
    pub(in crate::types::checker) fn indexed_declared_function_name(&self, name: &str) -> Option<String> {
        let folded = php_symbol_key(name);
        indexed_key(&self.fn_decls, &self.function_name_indices.declarations, name, &folded)
            .or_else(|| indexed_key(&self.function_variant_groups, &self.function_name_indices.variants, name, &folded))
            .or_else(|| indexed_key(&self.extern_functions, &self.function_name_indices.externs, name, &folded))
    }

    /// Looks up external ABI metadata by its canonical source name.
    pub(in crate::types::checker) fn indexed_extern_function_name(&self, name: &str) -> Option<String> {
        if self.extern_functions.contains_key(name) { return Some(name.to_string()); }
        indexed_key(&self.extern_functions, &self.function_name_indices.externs, name, &php_symbol_key(name))
    }
}
