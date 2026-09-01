//! Purpose:
//! Implements namespace/import name resolution helpers for types, classes, functions, and constants.
//! Handles alias registration, special names, class constants, and builtin fallbacks.
//!
//! Called from:
//! - `crate::name_resolver::expressions`, declarations, and statement context resolution.
//!
//! Key details:
//! - PHP class-like names are resolved differently from function and constant fallback lookups.
//! - The leading segment of a *qualified* name (`M\thing`) is expanded through the
//!   class/namespace import table for classes, functions, and constants alike
//!   (`expand_qualified_namespace_alias`); `use function` / `use const` aliases apply only to
//!   unqualified names, and fully-qualified names are never expanded.

use crate::errors::CompileError;
use crate::names::{php_symbol_key, Name};
use crate::parser::ast::{TypeExpr, UseItem, UseKind};

use super::{resolved_name, Imports, Symbols};

/// Recursively resolves a type expression, applying namespace/import rules to named types.
/// Primitive types (int, float, bool, string, etc.) are returned unchanged.
/// Pointer types and named types are resolved via `resolve_special_or_class_name`.
pub(super) fn resolve_type_expr(
    type_expr: &TypeExpr,
    current_namespace: Option<&str>,
    imports: &Imports,
    symbols: &Symbols,
) -> TypeExpr {
    match type_expr {
        TypeExpr::Int => TypeExpr::Int,
        TypeExpr::Float => TypeExpr::Float,
        TypeExpr::Bool => TypeExpr::Bool,
        TypeExpr::False => TypeExpr::False,
        TypeExpr::Str => TypeExpr::Str,
        TypeExpr::Void => TypeExpr::Void,
        TypeExpr::Never => TypeExpr::Never,
        TypeExpr::Iterable => TypeExpr::Iterable,
        TypeExpr::Array(inner) => TypeExpr::Array(Box::new(resolve_type_expr(
            inner,
            current_namespace,
            imports,
            symbols,
        ))),
        TypeExpr::Buffer(inner) => {
            TypeExpr::Buffer(Box::new(resolve_type_expr(
                inner,
                current_namespace,
                imports,
                symbols,
            )))
        }
        TypeExpr::Nullable(inner) => {
            TypeExpr::Nullable(Box::new(resolve_type_expr(
                inner,
                current_namespace,
                imports,
                symbols,
            )))
        }
        TypeExpr::Union(members) => TypeExpr::Union(
            members
                .iter()
                .map(|member| resolve_type_expr(member, current_namespace, imports, symbols))
                .collect(),
        ),
        TypeExpr::Intersection(members) => TypeExpr::Intersection(
            members
                .iter()
                .map(|member| resolve_type_expr(member, current_namespace, imports, symbols))
                .collect(),
        ),
        TypeExpr::Ptr(None) => TypeExpr::Ptr(None),
        TypeExpr::Ptr(Some(name)) => {
            let raw = name.as_str();
            if matches!(raw, "int" | "float" | "bool" | "string") {
                TypeExpr::Ptr(Some(name.clone()))
            } else {
                TypeExpr::Ptr(Some(resolved_name(resolve_special_or_class_name(
                    name,
                    current_namespace,
                    imports,
                    symbols,
                ))))
            }
        }
        TypeExpr::Named(name) => {
            let raw = name.as_str();
            if matches!(raw, "array" | "mixed" | "callable" | "void")
                || raw.eq_ignore_ascii_case("object")
            {
                TypeExpr::Named(name.clone())
            } else {
                TypeExpr::Named(resolved_name(resolve_special_or_class_name(
                    name,
                    current_namespace,
                    imports,
                    symbols,
                )))
            }
        }
    }
}

/// Registers use-item aliases (class, function, const) into the imports map by kind.
/// Returns a `DuplicateImport` error if an alias is already registered.
pub(super) fn register_imports(
    imports: &mut Imports,
    use_items: &[UseItem],
    span: crate::span::Span,
) -> Result<(), CompileError> {
    for item in use_items {
        let target = item.name.as_canonical();
        let (alias_map, alias_key) = match item.kind {
            UseKind::Class => (&mut imports.classes, php_symbol_key(&item.alias)),
            UseKind::Function => (&mut imports.functions, php_symbol_key(&item.alias)),
            UseKind::Const => (&mut imports.constants, item.alias.clone()),
        };
        if alias_map.insert(alias_key, target).is_some() {
            return Err(CompileError::new(
                span,
                &format!("Duplicate import alias: {}", item.alias),
            ));
        }
    }
    Ok(())
}

/// Expands the leading segment of a *qualified* name (contains `\`, no leading `\`)
/// through the class/namespace import table.
///
/// PHP translates the first segment of a qualified name using the class/namespace import
/// table (plain `use X as A;`) regardless of whether the name ultimately denotes a class,
/// a function, or a constant. `use function` / `use const` aliases apply only to
/// *unqualified* names, and fully-qualified names (`\A\b`) are never expanded. Alias
/// lookup is case-insensitive, and only the first segment is ever substituted.
///
/// Returns `None` when the name is unqualified or fully qualified, or when its first
/// segment is not a registered alias.
pub(super) fn expand_qualified_namespace_alias(name: &Name, imports: &Imports) -> Option<String> {
    if name.is_fully_qualified() || name.is_unqualified() {
        return None;
    }
    let first = name.parts.first()?;
    let alias = imports.classes.get(&php_symbol_key(first))?;
    let suffix = &name.parts[1..];
    if suffix.is_empty() {
        Some(alias.clone())
    } else {
        Some(format!("{}\\{}", alias, suffix.join("\\")))
    }
}

/// Resolves "self", "parent", "static" to their lowercase special-name form;
/// delegates to `resolved_class_name` for all other names.
pub(super) fn resolve_special_or_class_name(
    name: &Name,
    current_namespace: Option<&str>,
    imports: &Imports,
    symbols: &Symbols,
) -> String {
    let raw = name.as_canonical();
    if matches!(raw.to_ascii_lowercase().as_str(), "self" | "parent" | "static") {
        raw.to_ascii_lowercase()
    } else {
        resolved_class_name(name, current_namespace, imports, symbols)
    }
}

/// Resolves a class-like name to its canonical form using imports, current namespace,
/// and the symbol table. Handles fully-qualified, unqualified, and aliased names.
/// Falls back to the candidate string if no canonical form is found.
pub(super) fn resolved_class_name(
    name: &Name,
    current_namespace: Option<&str>,
    imports: &Imports,
    symbols: &Symbols,
) -> String {
    if name.is_fully_qualified() {
        let candidate = name.as_canonical();
        return symbols
            .canonical_class_like(&candidate)
            .unwrap_or(candidate);
    }
    if name.is_unqualified() {
        if let Some(alias) = name
            .last_segment()
            .and_then(|segment| imports.classes.get(&php_symbol_key(segment)))
        {
            return symbols
                .canonical_class_like(alias)
                .unwrap_or_else(|| alias.clone());
        }
    } else if let Some(candidate) = expand_qualified_namespace_alias(name, imports) {
        return symbols
            .canonical_class_like(&candidate)
            .unwrap_or(candidate);
    }
    let candidate = if let Some(namespace) = current_namespace {
        if !namespace.is_empty() {
            format!("{}\\{}", namespace, name.as_canonical())
        } else {
            name.as_canonical()
        }
    } else {
        name.as_canonical()
    };
    symbols.canonical_class_like(&candidate).unwrap_or(candidate)
}

/// Resolves a class constant name to its canonical form using imports and current namespace.
/// Unlike `resolved_class_name`, this does not consult the symbol table for canonicalization.
pub(super) fn resolved_class_constant_name(
    name: &Name,
    current_namespace: Option<&str>,
    imports: &Imports,
) -> String {
    if name.is_fully_qualified() {
        return name.as_canonical();
    }
    if name.is_unqualified() {
        if let Some(alias) = name
            .last_segment()
            .and_then(|segment| imports.classes.get(&php_symbol_key(segment)))
        {
            return alias.clone();
        }
    } else if let Some(candidate) = expand_qualified_namespace_alias(name, imports) {
        return candidate;
    }
    if let Some(namespace) = current_namespace {
        if !namespace.is_empty() {
            return format!("{}\\{}", namespace, name.as_canonical());
        }
    }
    name.as_canonical()
}

/// Resolves a function name to its canonical form using imports, current namespace,
/// and the symbol table. When unqualified and not imported, falls back to the local
/// namespace before attempting the global symbol table (PHP-style builtin fallback).
pub(super) fn resolve_function_name(
    name: &Name,
    current_namespace: Option<&str>,
    imports: &Imports,
    symbols: &Symbols,
) -> String {
    if name.is_fully_qualified() {
        let candidate = name.as_canonical();
        return symbols
            .canonical_function(&candidate)
            .unwrap_or(candidate);
    }
    if name.is_unqualified() {
        if let Some(alias) = name
            .last_segment()
            .and_then(|segment| imports.functions.get(&php_symbol_key(segment)))
        {
            return symbols
                .canonical_function(alias)
                .unwrap_or_else(|| alias.clone());
        }
        let local = if let Some(namespace) = current_namespace {
            if !namespace.is_empty() {
                format!("{}\\{}", namespace, name.as_canonical())
            } else {
                name.as_canonical()
            }
        } else {
            name.as_canonical()
        };
        if current_namespace.is_some() {
            if let Some(canonical) = symbols.canonical_function(&local) {
                return canonical;
            }
        }
        if let Some(canonical) = symbols.canonical_function(&name.as_canonical()) {
            return canonical;
        }
        return local;
    }
    if let Some(candidate) = expand_qualified_namespace_alias(name, imports) {
        return symbols.canonical_function(&candidate).unwrap_or(candidate);
    }
    let candidate = if let Some(namespace) = current_namespace {
        if !namespace.is_empty() {
            format!("{}\\{}", namespace, name.as_canonical())
        } else {
            name.as_canonical()
        }
    } else {
        name.as_canonical()
    };
    symbols.canonical_function(&candidate).unwrap_or(candidate)
}

/// Resolves a constant name to its canonical form using imports, current namespace,
/// the symbol table, and builtin globals (e.g., PHP_OS, SID, STDIN, STDOUT, STDERR).
pub(super) fn resolve_constant_name(
    name: &Name,
    current_namespace: Option<&str>,
    imports: &Imports,
    symbols: &Symbols,
) -> String {
    if name.is_fully_qualified() {
        return name.as_canonical();
    }
    if name.is_unqualified() {
        if matches!(name.as_str(), "PHP_OS" | "SID") {
            return name.as_canonical();
        }
        if let Some(alias) = name
            .last_segment()
            .and_then(|segment| imports.constants.get(segment))
        {
            return alias.clone();
        }
        let local = if let Some(namespace) = current_namespace {
            if !namespace.is_empty() {
                format!("{}\\{}", namespace, name.as_canonical())
            } else {
                name.as_canonical()
            }
        } else {
            name.as_canonical()
        };
        if current_namespace.is_some() && symbols.has_constant(&local) {
            return local;
        }
        if symbols.has_constant(&name.as_canonical()) {
            return name.as_canonical();
        }
        if is_builtin_global_constant(name.as_str()) {
            return name.as_canonical();
        }
        return local;
    }
    if let Some(candidate) = expand_qualified_namespace_alias(name, imports) {
        return candidate;
    }
    if let Some(namespace) = current_namespace {
        if !namespace.is_empty() {
            return format!("{}\\{}", namespace, name.as_canonical());
        }
    }
    name.as_canonical()
}

/// Returns true if `name` is a builtin global constant that should bypass symbol-table
/// resolution (e.g., PHP_OS, SID, STDIN, STDOUT, STDERR, FNM_* pathinfo flags).
fn is_builtin_global_constant(name: &str) -> bool {
        if matches!(
            name,
            "PHP_OS"
                // The PHP version surface, baked per compilation from `--php-version` / `--web`
                // by `codegen::prescan::collect_constants` — same mechanism as `PHP_OS`.
                | "PHP_VERSION"
                | "PHP_VERSION_ID"
                | "PHP_MAJOR_VERSION"
                | "PHP_MINOR_VERSION"
                | "PHP_RELEASE_VERSION"
                | "PHP_EXTRA_VERSION"
                | "PHP_SAPI"
                | "SID"
                | "PATHINFO_DIRNAME"
                | "PATHINFO_BASENAME"
                | "PATHINFO_EXTENSION"
                | "PATHINFO_FILENAME"
                | "PATHINFO_ALL"
                | "PHP_URL_SCHEME"
                | "PHP_URL_HOST"
                | "PHP_URL_PORT"
                | "PHP_URL_USER"
                | "PHP_URL_PASS"
                | "PHP_URL_PATH"
                | "PHP_URL_QUERY"
                | "PHP_URL_FRAGMENT"
                | "FNM_NOESCAPE"
                | "FNM_PATHNAME"
                | "FNM_PERIOD"
                | "FNM_CASEFOLD"
                | "ARRAY_FILTER_USE_VALUE"
                | "ARRAY_FILTER_USE_BOTH"
                | "ARRAY_FILTER_USE_KEY"
                | "STR_PAD_LEFT"
                | "STR_PAD_RIGHT"
                | "STR_PAD_BOTH"
                | "STDIN"
                | "STDOUT"
                | "STDERR"
                | "PHP_INT_MAX"
                | "PHP_INT_MIN"
                | "PHP_FLOAT_MAX"
                | "PHP_FLOAT_MIN"
                | "PHP_FLOAT_EPSILON"
                | "INF"
                | "NAN"
                | "M_PI"
                | "M_E"
                | "M_SQRT2"
                | "M_PI_2"
                | "M_PI_4"
                | "M_LOG2E"
                | "M_LOG10E"
                | "PHP_EOL"
                | "DIRECTORY_SEPARATOR"
                | "ICONV_IMPL"
                | "ICONV_VERSION"
        ) {
            return true;
        }
    // Shared source-of-truth slices for JSON, stream/socket, session, array, and math constants.
    crate::types::json_constants::JSON_INT_CONSTANTS
        .iter()
        .chain(crate::types::openssl_constants::OPENSSL_INT_CONSTANTS.iter())
        .chain(crate::types::stream_constants::STREAM_INT_CONSTANTS.iter())
        .chain(crate::types::session_constants::SESSION_INT_CONSTANTS.iter())
        .chain(crate::types::error_constants::ERROR_LEVEL_CONSTANTS.iter())
        .chain(crate::types::array_constants::ARRAY_INT_CONSTANTS.iter())
        .chain(crate::types::math_constants::MATH_INT_CONSTANTS.iter())
        .chain(crate::types::iconv_constants::ICONV_INT_CONSTANTS.iter())
        .any(|(constant_name, _)| *constant_name == name)
        || crate::types::locale_constants::LOCALE_CONSTANT_NAMES.contains(&name)
        || crate::types::date_constants::DATE_INT_CONSTANTS
            .iter()
            .any(|(constant_name, _)| *constant_name == name)
        || crate::types::date_constants::DATE_STR_CONSTANTS
            .iter()
            .any(|(constant_name, _)| *constant_name == name)
}
