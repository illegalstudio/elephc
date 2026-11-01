//! Purpose:
//! Coordinates PHP namespace and import resolution across a parsed program.
//! Rewrites names to canonical forms and flattens namespace wrapper statements.
//!
//! Called from:
//! - `crate::pipeline::compile()` after include resolution and before optimization/type checking.
//!
//! Key details:
//! - Builtin fallback and case-insensitive symbol lookup must match PHP visibility rules.
//! - Static class-string aliases are materialized only after every name in their call is canonical.

mod expressions;
mod function_fallbacks;
mod generic_calls;
mod names;
mod promoted_attributes;
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
    // Expose declaration groups without erasing lowered array-append boundaries.
    let program = crate::parser::declaration_groups::flatten_top_level_declaration_groups(program);
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
    let resolved = statements::resolve_stmt_list(&program, None, &Imports::default(), &symbols)?;
    Ok(crate::autoload::collect_resolved_aliases(resolved))
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
            // `ob_start([$shell, 'writeStdout'], 1)` (psysh's execution closures): the output
            // handler is the bound method, which the backend runs as the closure
            // `$shell->writeStdout(...)` (an array pair is no handler shape it can call).
            if idx == 0 && function_name.eq_ignore_ascii_case("ob_start") {
                if let ExprKind::ArrayLiteral(items) = &arg.kind {
                    if let [object, method] = items.as_slice() {
                        if let ExprKind::StringLiteral(method) = &method.kind {
                            if matches!(
                                object.kind,
                                ExprKind::Variable(_)
                                    | ExprKind::This
                                    | ExprKind::PropertyAccess { .. }
                            ) {
                                return Expr::new(
                                    ExprKind::FirstClassCallable(
                                        crate::parser::ast::CallableTarget::Method {
                                            object: Box::new(object.clone()),
                                            method: method.clone(),
                                        },
                                    ),
                                    arg.span,
                                );
                            }
                        }
                    }
                }
            }
            if callback_positions.contains(&idx) {
                if let ExprKind::StringLiteral(raw_name) = &arg.kind {
                    // A function alias names the function it stands for (`array_map('sizeof', ...)`).
                    let bare = raw_name.strip_prefix('\\').unwrap_or(raw_name);
                    if let Some(canonical) = canonical_function_alias(bare) {
                        if !symbols.declares_function(bare) {
                            return Expr::new(ExprKind::StringLiteral(canonical.to_string()), arg.span);
                        }
                    }
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

/// Returns the canonical global name of a function supplied by the late compatibility prelude.
///
/// Autoloaded files are name-resolved before that prelude is injected, so namespaced bare calls
/// need the same PHP global fallback that registry builtins receive.
pub(crate) fn canonical_compat_prelude_function_name(name: &str) -> Option<String> {
    const FUNCTIONS: &[&str] = &[
        "levenshtein",
        "strip_tags",
        "is_countable",
        "random_bytes",
        "http_build_query",
        "escapeshellarg",
        "cli_set_process_title",
        "setproctitle",
        "str_getcsv",
        "error_reporting",
        // `error_get_last` and `error_clear_last` join their family for the reason
        // `ini_get_all` below documents: Symfony's `ErrorHandler` lives in
        // `namespace Symfony\Component\ErrorHandler` and calls `error_get_last()` BARE from
        // its shutdown handler. Without the global fallback that call stays
        // `Symfony\Component\ErrorHandler\error_get_last` and dies at run time while
        // `error_reporting()` in the same file resolves.
        "error_get_last",
        "error_clear_last",
        // `trigger_error` was MISSING from this list, and had been since the family landed —
        // found by `compat_prelude_gate_tests::an_autoloaded_shutdown_function_reads_the_last_error`,
        // which died with `Call to undefined function App\trigger_error()`. It belongs here for
        // exactly the same reason as its neighbours and is NOT covered by the ordinary builtin
        // fallback: that one is driven by the compiler's registry-derived name set, and a
        // `BuiltinKind::PreludeProvided` contract is deliberately absent from it. `error_log`
        // is the one family member that does NOT need an entry, because its contract is an
        // ordinary `BuiltinKind::Function` and the registry fallback already answers it.
        "trigger_error",
        "set_error_handler",
        "get_error_handler",
        "restore_error_handler",
        "register_shutdown_function",
        "set_exception_handler",
        "restore_exception_handler",
        "ini_get",
        "ini_set",
        // `ini_get_all` joins its two neighbours now that the CLI INI wrappers have a second,
        // POST-AUTOLOAD injection site (`opcache_prelude::inject_cli_ini_if_used`). Without it
        // a namespaced bare `ini_get_all()` in an autoloaded class stayed `Probe\ini_get_all`
        // and died at run time while `ini_get()` one line above resolved — measured.
        "ini_get_all",
        "get_cfg_var",
        "gc_enabled",
        "gc_enable",
        "gc_disable",
        "gc_collect_cycles",
        "flush",
        "strnatcmp",
        "strnatcasecmp",
        "parse_str",
        // `get_class_vars` is an eval-only contract, absent from the registry fallback, and
        // Eloquent's `Model` calls it bare from `namespace Illuminate\Database\Eloquent`.
        "get_class_vars",
        "hash_init",
        "hash_update",
        "hash_final",
        "hash_copy",
        // `var_export` is not injected by the LATE prelude at all — `var_export_prelude` runs in
        // its own phase before `autoload-run` — but it needs this entry for the same reason
        // `ini_get_all` does, and the shape is measured rather than assumed: an entry that names
        // `var_export` gets the prelude injected, and an AUTOLOADED class in `namespace App;`
        // that calls the bare `var_export($v, true)` still died with `Call to undefined function
        // App\var_export()`. Autoloaded files are name-resolved individually, and without a
        // global fallback here `App\var_export` is simply an unknown function.
        //
        // The fallback is consulted only after the declared-function lookup in
        // `symbols::canonical_function`, so a user's own `App\var_export()` still wins — which is
        // the hijack `rewrite_var_export_return_flag`'s first guard exists to prevent, and it
        // stays prevented. Once the name folds to the global one that rewrite also fires, so the
        // autoloaded call is typed `string` rather than `string|null`, exactly as the entry
        // file's call already was.
        "var_export",
        // A `BuiltinKind::PreludeProvided` contract like `trigger_error` above, and called bare
        // from namespaced library code (Symfony's password hasher, Composer's platform check).
        "version_compare",
        "hash_pbkdf2",
        "array_change_key_case",
        // `assert()` has no registry contract: `assert_prelude` supplies its one-argument form and
        // `compat_preludes::lower_single_arg_assert` routes the call by its GLOBAL name. A bare
        // `assert($x)` inside a namespace stayed `A\assert` and died with `Call to undefined
        // function A\assert()` (Termwind's `HtmlRenderer` asserts `instanceof DOMNode` that way).
        "assert",
    ];
    let bare = name.trim_start_matches('\\');
    FUNCTIONS
        .iter()
        .find(|candidate| bare.eq_ignore_ascii_case(candidate))
        .map(|candidate| (*candidate).to_string())
}

/// Reports whether `name` matches one of PHP's procedural date/time aliases
/// (e.g. `date_create`, `idate`, `gmstrftime`). The name set is the same as the one
/// rewritten by `expressions::rewrite_date_procedural_alias`, minus the per-arity guards,
/// so `function_exists()` and other introspection builtins see the same surface that the
/// resolver rewrites.
pub(crate) fn is_date_procedural_alias(name: &str) -> bool {
    expressions::is_date_procedural_alias(name)
}

/// Returns the function a global PHP function alias (`sizeof`, `pos`, ...) names, if `name` is one.
pub(crate) fn canonical_function_alias(name: &str) -> Option<&'static str> {
    if name.contains('\\') {
        return None;
    }
    expressions::canonical_function_alias(name)
}

/// Returns every PHP function alias name the resolver rewrites.
pub(crate) fn function_alias_names() -> impl Iterator<Item = &'static str> {
    expressions::FUNCTION_ALIASES.iter().map(|(alias, _)| *alias)
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
