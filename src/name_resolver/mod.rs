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
mod function_fallbacks;
mod names;
mod declarations;
mod statements;
mod symbols;

use std::collections::{HashMap, HashSet};

use crate::errors::CompileError;
use crate::names::{php_symbol_key, Name, NameKind};
use crate::parser::ast::{Expr, ExprKind, Program};

pub(crate) use function_fallbacks::FunctionFallbacks;

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
    conditional_functions: HashSet<String>,
    classes: HashMap<String, String>,
    interfaces: HashMap<String, String>,
    traits: HashMap<String, String>,
    constants: HashSet<String>,
    extern_functions: HashMap<String, String>,
    extern_classes: HashMap<String, String>,
}

/// An impossible PHP namespace used to retain the provenance of symbols seeded only for a
/// prelude detector. The NUL byte cannot occur in a parsed PHP identifier.
const PRELUDE_FALLBACK_NAMESPACE: &str = "\0elephc-prelude-fallback";

/// Resolves PHP namespace/use statements and rewrites names to canonical forms across the program.
pub fn resolve(program: Program) -> Result<Program, CompileError> {
    resolve_with_additional_global_symbols(program, &[], &[])
}

/// Resolves a program while seeding additional global function and class-like symbols as
/// distinguishable fallbacks behind user declarations.
///
/// Prelude detectors use this to classify raw names with the same namespace/import rules as
/// the real resolver before deciding whether to inject their declarations. References bound
/// to a seed resolve under [`PRELUDE_FALLBACK_NAMESPACE`], while real user declarations keep
/// their canonical names. Seeded symbols do not replace regular or extern user symbols.
pub(crate) fn resolve_with_additional_global_symbols(
    program: Program,
    global_functions: &[&str],
    global_classes: &[&str],
) -> Result<Program, CompileError> {
    crate::compiler_stack::with_compiler_stack(|| {
        resolve_on_compiler_stack(program, global_functions, global_classes)
    })
}

/// The resolver body, running on the stack its entry point established.
fn resolve_on_compiler_stack(
    program: Program,
    global_functions: &[&str],
    global_classes: &[&str],
) -> Result<Program, CompileError> {
    let mut symbols = Symbols::default();
    symbols::collect_symbols(&program, None, &mut symbols);
    for function in global_functions {
        if !symbols.declares_function(function) {
            symbols.functions.insert(
                php_symbol_key(function),
                format!("{PRELUDE_FALLBACK_NAMESPACE}\\{function}"),
            );
        }
    }
    for class in global_classes {
        if !symbols.declares_class_like(class) {
            symbols.classes.insert(
                php_symbol_key(class),
                format!("{PRELUDE_FALLBACK_NAMESPACE}\\{class}"),
            );
        }
    }
    statements::resolve_stmt_list(&program, None, &Imports::default(), &symbols)
}

/// Returns whether `name` was bound to an additional global fallback named `symbol` by
/// [`resolve_with_additional_global_symbols`].
pub(crate) fn is_additional_global_symbol(name: &Name, symbol: &str) -> bool {
    matches!(
        name.parts.as_slice(),
        [namespace, candidate]
            if namespace == PRELUDE_FALLBACK_NAMESPACE
                && candidate.eq_ignore_ascii_case(symbol)
    )
}

/// Rewrites string literal arguments for functions that invoke callable names.
/// For functions like `array_map` or `usort`, resolves string callback names to their canonical
/// fully-qualified form using the current namespace and imports. `function_exists()` is excluded
/// because PHP treats its argument as a literal introspection name rather than a callable lookup.
fn rewrite_callback_literal_args(
    function_name: &str,
    args: &[Expr],
    current_namespace: Option<&str>,
    imports: &Imports,
    symbols: &Symbols,
) -> Vec<Expr> {
    let callback_positions: &[usize] = match function_name {
        "call_user_func" | "call_user_func_array" => &[0],
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

/// Reports whether `name` matches one of PHP's procedural date/time aliases
/// (e.g. `date_create`, `idate`, `gmstrftime`). The name set is the same as the one
/// rewritten by `expressions::rewrite_date_procedural_alias`, minus the per-arity guards,
/// so `function_exists()` and other introspection builtins see the same surface that the
/// resolver rewrites.
pub(crate) fn is_date_procedural_alias(name: &str) -> bool {
    expressions::is_date_procedural_alias(name)
}

/// Recognizes an exact global date alias without giving explicitly namespaced strings fallback.
pub(crate) fn is_global_date_procedural_alias(name: &str) -> bool {
    !name.contains('\\') && is_date_procedural_alias(name)
}

/// Returns every lowercase procedural date/time alias name, i.e. the exact set
/// [`is_date_procedural_alias`] accepts as a bare (non-namespaced) name.
///
/// Codegen uses this to bake the alias names into a dynamic `function_exists($name)` lookup
/// table; because both functions read `expressions::DATE_PROCEDURAL_ALIASES`, the baked table
/// and the compile-time fold can never drift apart.
pub(crate) fn date_procedural_alias_names() -> &'static [&'static str] {
    expressions::DATE_PROCEDURAL_ALIASES
}

/// Returns the inclusive `(min, max)` argument arity that the resolver's date/time alias
/// desugaring accepts for `name`, or `None` when `name` is not a desugared alias. The type
/// checker uses this to report a precise arity error (instead of "Undefined function") when a
/// known alias call survives desugaring because its argument count was out of range.
pub(crate) fn date_procedural_alias_arity(name: &str) -> Option<(usize, usize)> {
    expressions::date_procedural_alias_arity(name)
}
