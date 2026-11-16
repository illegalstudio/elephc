//! Purpose:
//! Collects declared symbols needed for PHP namespace fallback and redeclaration-aware lookup.
//! Records functions, constants, and class-like declarations using case-folded lookup keys.
//!
//! Called from:
//! - `crate::name_resolver::resolve()` before rewriting references.
//!
//! Key details:
//! - Builtin class-like symbols and public prelude functions use the shared contracts so
//!   independently resolved autoloaded files retain PHP builtin fallback.

use crate::names::{canonical_name_for_decl, php_symbol_key};
use crate::parser::ast::{Stmt, StmtKind};

use super::{canonical_builtin_function_name, namespace_name, Symbols};

#[cfg(test)]
mod tests {
    use crate::parser::ast::{ExprKind, StmtKind};

    /// Resolves and target-folds a standalone file, then reads its final function call bindings.
    fn resolved_calls(source: &str) -> Vec<String> {
        let tokens = crate::lexer::tokenize(source).unwrap();
        let program = crate::parser::parse(&tokens).unwrap();
        let resolved = crate::name_resolver::resolve(program).unwrap();
        let target = crate::codegen_support::platform::Target::new(
            crate::codegen_support::platform::Platform::Linux,
            crate::codegen_support::platform::Arch::X86_64,
        );
        crate::optimize::fold_constants_for_target(resolved, target)
            .into_iter()
            .filter_map(|stmt| {
                let StmtKind::ExprStmt(expr) = stmt.kind else { return None; };
                let ExprKind::FunctionCall { name, .. } = expr.kind else { return None; };
                Some(name.as_canonical())
            })
            .collect()
    }

    /// Prelude names have global PHP fallback even when the file declares no prelude itself.
    #[test]
    fn prelude_function_fallback_without_injected_declarations() {
        let _strict = crate::strict_php::scoped_enable();
        assert_eq!(
            resolved_calls("<?php namespace App; Mt_SrAnD(42); lcg_value(); getrandmax();"),
            ["mt_srand", "lcg_value", "getrandmax"],
        );
    }

    /// Local declarations, function imports, and explicit namespace references keep precedence.
    #[test]
    fn prelude_function_fallback_preserves_explicit_and_local_bindings() {
        assert_eq!(
            resolved_calls(r"<?php namespace App;
                use function Vendor\lcg_value;
                function mt_srand(int $seed): void {}
                mt_srand(42); lcg_value(); namespace\getrandmax(); Other\getrandmax();"),
            ["App\\mt_srand", "Vendor\\lcg_value", "App\\getrandmax", "App\\Other\\getrandmax"],
        );
    }

    /// Global fallback excludes internal helpers and extensions hidden by strict PHP.
    #[test]
    fn prelude_function_fallback_respects_contract_visibility() {
        let source = "<?php namespace App; read_exif_data('photo.jpg'); __elephc_array_rand([1]);";
        assert_eq!(resolved_calls(source), ["read_exif_data", "App\\__elephc_array_rand"]);
        let _strict = crate::strict_php::scoped_enable();
        assert_eq!(resolved_calls(source), ["App\\read_exif_data", "App\\__elephc_array_rand"]);
    }
}


impl Symbols {
    /// Resolves declared functions first, then registered builtins and visible prelude contracts.
    pub(super) fn canonical_function(&self, name: &str) -> Option<String> {
        let key = php_symbol_key(name);
        let extension_builtin =
            crate::types::checker::builtins::catalog::strict_php_hidden_builtin_for_profile(
                &key,
                true,
            )
            .then(|| canonical_builtin_function_name(name))
            .flatten();
        if !crate::strict_php::is_enabled() && extension_builtin.is_some() {
            return extension_builtin;
        }
        self.functions
            .get(&key)
            .or_else(|| self.extern_functions.get(&key))
            .cloned()
            .or_else(|| canonical_builtin_function_name(name))
            .or_else(|| {
                let contract = elephc_builtin_contract::lookup(name)?;
                (contract.kind == elephc_builtin_contract::BuiltinKind::PreludeProvided
                    && !contract.internal
                    && (!contract.extension || !crate::strict_php::is_enabled()))
                    .then(|| contract.name.to_string())
            })
    }

    /// Returns whether `name` resolves to a user-declared (or extern) function,
    /// ignoring compiler builtins. Used to stop procedural date/time alias
    /// rewriting from hijacking a user function whose name collides with an
    /// alias (e.g. a namespaced `App\date_diff`).
    pub(super) fn declares_function(&self, name: &str) -> bool {
        let key = php_symbol_key(name);
        self.functions.contains_key(&key) || self.extern_functions.contains_key(&key)
    }

    /// canonical_class_like
    pub(super) fn canonical_class_like(&self, name: &str) -> Option<String> {
        let key = php_symbol_key(name);
        self.classes
            .get(&key)
            .or_else(|| self.interfaces.get(&key))
            .or_else(|| self.traits.get(&key))
            .or_else(|| self.extern_classes.get(&key))
            .cloned()
            .or_else(|| {
                elephc_builtin_contract::lookup_class(name).map(|builtin| builtin.name.to_string())
            })
    }

    /// Returns whether `name` resolves to a user-declared (or extern) class,
    /// interface, or trait, ignoring compiler builtins. Used to stop builtin
    /// static-method desugaring (e.g. `DateTimeZone::listIdentifiers`) from
    /// hijacking a user class that happens to share the name.
    pub(super) fn declares_class_like(&self, name: &str) -> bool {
        let key = php_symbol_key(name);
        self.classes.contains_key(&key)
            || self.interfaces.contains_key(&key)
            || self.traits.contains_key(&key)
            || self.extern_classes.contains_key(&key)
    }

    /// has_constant
    pub(super) fn has_constant(&self, name: &str) -> bool {
        self.constants.contains(name)
    }
}

/// collect_symbols
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
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                collect_conditional_function_symbols(
                    then_body,
                    namespace.as_deref(),
                    symbols,
                );
                for (_, body) in elseif_clauses {
                    collect_conditional_function_symbols(
                        body,
                        namespace.as_deref(),
                        symbols,
                    );
                }
                if let Some(body) = else_body {
                    collect_conditional_function_symbols(
                        body,
                        namespace.as_deref(),
                        symbols,
                    );
                }
            }
            StmtKind::FunctionDecl { name, .. } => {
                insert_folded_symbol(
                    &mut symbols.functions,
                    canonical_name_for_decl(namespace.as_deref(), name),
                );
            }
            StmtKind::FunctionVariantGroup { name, .. } => {
                insert_folded_symbol(&mut symbols.functions, name.clone());
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

/// Collects function declarations from conditional branches without predeclaring class-like names.
///
/// PHP function polyfills need namespace resolution before target folding decides whether their
/// branch survives. Conditional classes are different: treating one as an unconditional symbol
/// can suppress builtin-class rewrites even after constant propagation removes its declaration.
fn collect_conditional_function_symbols(
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
                collect_conditional_function_symbols(body, block_namespace.as_deref(), symbols);
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                collect_conditional_function_symbols(
                    then_body,
                    namespace.as_deref(),
                    symbols,
                );
                for (_, body) in elseif_clauses {
                    collect_conditional_function_symbols(
                        body,
                        namespace.as_deref(),
                        symbols,
                    );
                }
                if let Some(body) = else_body {
                    collect_conditional_function_symbols(
                        body,
                        namespace.as_deref(),
                        symbols,
                    );
                }
            }
            StmtKind::FunctionDecl { name, .. } => {
                let canonical = canonical_name_for_decl(namespace.as_deref(), name);
                symbols.conditional_functions.insert(php_symbol_key(&canonical));
                insert_folded_symbol(
                    &mut symbols.functions,
                    canonical,
                );
            }
            StmtKind::FunctionVariantGroup { name, .. } => {
                insert_folded_symbol(&mut symbols.functions, name.clone());
            }
            StmtKind::ExternFunctionDecl { name, .. } => {
                insert_folded_symbol(
                    &mut symbols.extern_functions,
                    canonical_name_for_decl(namespace.as_deref(), name),
                );
            }
            _ => {}
        }
    }
}

/// Inserts folded symbol into the supplied builtin metadata registry.
fn insert_folded_symbol(symbols: &mut std::collections::HashMap<String, String>, name: String) {
    symbols.entry(php_symbol_key(&name)).or_insert(name);
}
