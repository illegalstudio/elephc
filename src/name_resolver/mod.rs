//! Purpose:
//! Coordinates PHP namespace and import resolution across a parsed program.
//! Rewrites names to canonical forms and flattens namespace wrapper statements.
//!
//! Called from:
//! - `crate::pipeline::compile()` after include resolution and before optimization/type checking.
//!
//! Key details:
//! - Builtin fallback and case-insensitive symbol lookup must match PHP visibility rules.

mod expressions;
mod names;
mod declarations;
mod statements;
mod symbols;

use std::collections::{HashMap, HashSet};

use crate::errors::CompileError;
use crate::names::{Name, NameKind};
use crate::parser::ast::{Expr, ExprKind, Program};

/// Tracks namespace use imports for classes, functions, and constants.
/// Used during name resolution to map short names to their canonical fully-qualified names.
#[derive(Default, Clone)]
struct Imports {
    classes: HashMap<String, String>,
    functions: HashMap<String, String>,
    constants: HashMap<String, String>,
}

/// Internal symbol table for tracking declared functions, classes, interfaces, traits,
/// constants, and extern symbols within a namespace scope.
#[derive(Default)]
struct Symbols {
    functions: HashMap<String, String>,
    classes: HashMap<String, String>,
    interfaces: HashMap<String, String>,
    traits: HashMap<String, String>,
    constants: HashSet<String>,
    extern_functions: HashMap<String, String>,
    extern_classes: HashMap<String, String>,
}

/// Resolves PHP namespace/use statements and rewrites names to canonical forms across the program.
pub fn resolve(program: Program) -> Result<Program, CompileError> {
    let mut symbols = Symbols::default();
    symbols::collect_symbols(&program, None, &mut symbols);
    statements::resolve_stmt_list(&program, None, &Imports::default(), &symbols)
}

/// Rewrites string literal arguments in callbacks for functions that accept callable names.
/// For functions like `array_map` or `usort`, resolves string callback names to their canonical
/// fully-qualified form using the current namespace and imports.
fn rewrite_callback_literal_args(
    function_name: &str,
    args: &[Expr],
    current_namespace: Option<&str>,
    imports: &Imports,
    symbols: &Symbols,
) -> Vec<Expr> {
    let callback_positions: &[usize] = match function_name {
        "function_exists" | "call_user_func" | "call_user_func_array" => &[0],
        "array_map" | "array_filter" | "array_reduce" | "array_walk" => &[0],
        "usort" | "uksort" | "uasort" => &[1],
        _ => &[],
    };

    args.iter()
        .enumerate()
        .map(|(idx, arg)| {
            if callback_positions.contains(&idx) {
                if let ExprKind::StringLiteral(raw_name) = &arg.kind {
                    let resolved = names::resolve_function_name(
                        &parse_callback_name(raw_name),
                        current_namespace,
                        imports,
                        symbols,
                    );
                    return Expr::new(ExprKind::StringLiteral(resolved), arg.span);
                }
            }
            arg.clone()
        })
        .collect()
}

/// Parses a string callback name (e.g., `"my_func"` or `"MyNamespace\MyClass::method"`)
/// into a `Name` with the appropriate `NameKind`. Leading backslashes are stripped;
/// names containing backslashes are treated as fully-qualified.
fn parse_callback_name(raw_name: &str) -> Name {
    if let Some(stripped) = raw_name.strip_prefix('\\') {
        return Name::from_parts(
            NameKind::FullyQualified,
            stripped.split('\\').map(str::to_string).collect(),
        );
    }
    if raw_name.contains('\\') {
        return Name::from_parts(
            NameKind::FullyQualified,
            raw_name.split('\\').map(str::to_string).collect(),
        );
    }
    Name::unqualified(raw_name)
}

/// Converts a string containing a fully-qualified name (e.g., `"Namespace\Class"`)
/// into a `Name` with `NameKind::FullyQualified`.
fn resolved_name(name: String) -> Name {
    Name::from_parts(
        NameKind::FullyQualified,
        name.split('\\').map(str::to_string).collect(),
    )
}

/// Extracts the namespace name as a dot-separated string from an optional `Name`.
/// Returns an empty string if the name is `None`.
fn namespace_name(name: &Option<Name>) -> String {
    name.as_ref().map(Name::as_canonical).unwrap_or_default()
}

/// Returns `true` if `name` is a supported builtin function in PHP.
/// Used by name resolution to apply PHP's builtin fallback rules.
pub(crate) fn is_builtin_function(name: &str) -> bool {
    crate::types::checker::builtins::is_supported_builtin_function(name)
}

/// Returns the canonical name for a builtin function, case-normalized.
/// Returns `None` if the name is not a known builtin.
pub(crate) fn canonical_builtin_function_name(name: &str) -> Option<String> {
    crate::types::checker::builtins::canonical_builtin_function_name(name)
}
