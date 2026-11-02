//! Purpose:
//! Collects declared symbols needed for PHP namespace fallback and redeclaration-aware lookup.
//! Records functions, constants, and class-like declarations using case-folded lookup keys.
//!
//! Called from:
//! - `crate::name_resolver::resolve()` before rewriting references.
//!
//! Key details:
//! - Builtin class-like symbols are seeded so unresolved user names can still bind to PHP builtins.

use crate::names::{canonical_name_for_decl, php_symbol_key};
use crate::parser::ast::{Stmt, StmtKind};

use super::{canonical_builtin_function_name, namespace_name, Symbols};



impl Symbols {
    /// canonical_function
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
            .or_else(|| super::canonical_compat_prelude_function_name(name))
            // PHP's global fallback for a bare call, applied by DECLARATION rather than by an
            // allow-list. An eager `autoload.files` entry runs before any class file is resolved,
            // so every global function it declares is known by the time a namespaced file calls
            // one bare. Without this, `trigger_deprecation()` inside
            // `namespace Symfony\Component\Console\Input` stays
            // `Symfony\Component\Console\Input\trigger_deprecation` and dies at run time while
            // the very same declaration answers a call from global scope — measured, three files,
            // no framework. `canonical_compat_prelude_function_name` above is the hand-maintained
            // version of this rule and has been caught missing a name three times; this one needs
            // no list.
            .or_else(|| {
                crate::eager_globals::declares(name)
                    .then(|| name.trim_start_matches('\\').to_string())
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
            StmtKind::ExprStmt(expr) => {
                if let Some(defined) = defined_constant_name(expr) {
                    symbols.constants.insert(defined);
                }
            }
            _ => {}
        }
    }
}

/// Returns the constant name a top-level `define('NAME', …)` statement creates, if any.
///
/// `define()` takes the name as a STRING, so the current namespace never qualifies it: inside
/// `namespace Symfony\Polyfill\Intl\Grapheme;`, `\define('SYMFONY_GRAPHEME_CLUSTER_RX', …)`
/// creates the constant in the GLOBAL namespace, and php resolves the file's own unqualified
/// `SYMFONY_GRAPHEME_CLUSTER_RX` by falling back there. That fallback is
/// [`super::names::resolve_constant_name`]'s `symbols.has_constant(&name.as_canonical())` arm,
/// which only ever saw `const` DECLARATIONS — so every `define()`d constant failed the fallback
/// and resolved to `<namespace>\NAME`, which nothing defines. Recording the name here is what
/// makes the arm reachable.
///
/// Only a LITERAL name is recorded: `define($name, …)` cannot be known here, and php would not
/// let a namespace fallback find it either. A name written with backslashes is honoured as php
/// does (`define('Foo\\BAR', 1)` really does create `Foo\BAR`), with a leading separator trimmed
/// so the key matches `Name::as_canonical`'s spelling.
///
/// Deliberately limited to an UNCONDITIONAL statement. `if (!defined('X')) { define('X', …); }`
/// is the other common polyfill shape, and recording it here would claim a constant that the
/// branch may not create — the conditional collector below takes the same care with functions.
fn defined_constant_name(expr: &crate::parser::ast::Expr) -> Option<String> {
    use crate::parser::ast::ExprKind;
    let ExprKind::FunctionCall { name, args } = &expr.kind else {
        return None;
    };
    if !is_define_call(name) {
        return None;
    }
    let ExprKind::StringLiteral(defined) = &args.first()?.kind else {
        return None;
    };
    let defined = defined.trim_start_matches('\\');
    (!defined.is_empty()).then(|| defined.to_string())
}

/// Reports whether `name` refers to the builtin `define()`.
///
/// Case-insensitive and single-segment, mirroring `is_dirname_call` in
/// `crate::resolver::path_eval`: php function names are case-insensitive, this runs BEFORE the
/// name resolver has folded anything, and a multi-segment `Foo\define` is a user function.
fn is_define_call(name: &crate::names::Name) -> bool {
    matches!(
        name.kind,
        crate::names::NameKind::Unqualified | crate::names::NameKind::FullyQualified
    ) && name.parts.len() == 1
        && name.parts[0].eq_ignore_ascii_case("define")
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
