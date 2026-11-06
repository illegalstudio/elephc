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
use crate::names::{php_symbol_key, Name, NameKind};
use crate::parser::ast::{CType, TypeExpr, TypeParam, UseItem, UseKind};

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
    resolve_type_expr_with_type_params(type_expr, current_namespace, imports, symbols, &[])
}

/// Resolves class names while retaining case-sensitive, unqualified template references.
/// Qualified class names are never mistaken for a same-named declaration parameter.
pub(super) fn resolve_type_expr_with_type_params(
    type_expr: &TypeExpr,
    current_namespace: Option<&str>,
    imports: &Imports,
    symbols: &Symbols,
    type_params: &[TypeParam],
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
        // A signature NAMES classes, and they resolve through the same namespace and imports as
        // any other mention: `callable(User): Order` inside a namespace means that namespace's
        // `User`, not the global one.
        TypeExpr::CallableSig { params, ret } => TypeExpr::CallableSig {
            params: params
                .iter()
                .map(|param| resolve_type_expr_with_type_params(param, current_namespace, imports, symbols, type_params))
                .collect(),
            ret: Box::new(resolve_type_expr_with_type_params(ret, current_namespace, imports, symbols, type_params)),
        },
        TypeExpr::Array(inner) => TypeExpr::Array(Box::new(resolve_type_expr_with_type_params(
            inner,
            current_namespace,
            imports,
            symbols,
            type_params,
        ))),
        // Both halves are resolved: `array<string, Foo>` names a class in its VALUE, and a
        // key may name an enum or a class constant's type, so resolving only one would leave
        // an unqualified name for a later pass to mistake for a global.
        TypeExpr::AssocArray { key, value } => TypeExpr::AssocArray {
            key: Box::new(resolve_type_expr_with_type_params(
                key,
                current_namespace,
                imports,
                symbols,
                type_params,
            )),
            value: Box::new(resolve_type_expr_with_type_params(
                value,
                current_namespace,
                imports,
                symbols,
                type_params,
            )),
        },
        TypeExpr::Buffer(inner) => {
            TypeExpr::Buffer(Box::new(resolve_type_expr_with_type_params(
                inner,
                current_namespace,
                imports,
                symbols,
                type_params,
            )))
        }
        TypeExpr::Nullable(inner) => {
            TypeExpr::Nullable(Box::new(resolve_type_expr_with_type_params(
                inner,
                current_namespace,
                imports,
                symbols,
                type_params,
            )))
        }
        TypeExpr::Union(members) => TypeExpr::Union(
            members
                .iter()
                .map(|member| resolve_type_expr_with_type_params(member, current_namespace, imports, symbols, type_params))
                .collect(),
        ),
        TypeExpr::Intersection(members) => TypeExpr::Intersection(
            members
                .iter()
                .map(|member| resolve_type_expr_with_type_params(member, current_namespace, imports, symbols, type_params))
                .collect(),
        ),
        TypeExpr::Ptr(None) => TypeExpr::Ptr(None),
        TypeExpr::Ptr(Some(name)) => {
            let raw = name.as_str();
            if raw.starts_with("ptr<") {
                TypeExpr::Ptr(Some(Name::unqualified(resolve_pointer_target(
                    raw, current_namespace, imports, symbols,
                ))))
            } else if matches!(raw, "int" | "float" | "bool" | "string" | "ptr" | "pointer")
                || is_unshadowed_width_name(name, current_namespace, imports, symbols)
            {
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
        // The HEAD is a class name and the ARGUMENTS are types: `Repository<User>` in a
        // namespace names that namespace's `Repository` holding that namespace's `User`, and
        // resolving only one of the two would instantiate a template under a name no
        // declaration matches.
        TypeExpr::GenericClass { name, args } => TypeExpr::GenericClass {
            name: resolved_name(resolve_special_or_class_name(
                name,
                current_namespace,
                imports,
                symbols,
            )),
            args: args
                .iter()
                .map(|arg| resolve_type_expr_with_type_params(arg, current_namespace, imports, symbols, type_params))
                .collect(),
        },
        TypeExpr::Named(name) => {
            if name.is_unqualified() && type_params.iter().any(|param| param.name == name.as_str()) {
                return type_expr.clone();
            }
            let raw = name.as_str();
            if matches!(raw, "array" | "mixed" | "callable" | "void" | "null")
                || raw.eq_ignore_ascii_case("object")
                || is_unshadowed_width_name(name, current_namespace, imports, symbols)
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

/// Returns whether an unqualified C width name (`u8`, `i32`, `f32`, ...) keeps its bare spelling
/// in a type position.
///
/// The width names are storage formats for packed fields, `buffer<T>` elements and pointer
/// targets, so unshadowed widths are not canonicalized into the current namespace. A program that really
/// declares a class with such a name, or imports one under it with `use ... as u8`, shadows the
/// width wherever that class is visible, and the name then resolves exactly like any PHP class
/// name (`new u8()` and a `u8` type hint agree).
fn is_unshadowed_width_name(
    name: &Name,
    current_namespace: Option<&str>,
    imports: &Imports,
    symbols: &Symbols,
) -> bool {
    let raw = name.as_str();
    if !name.is_unqualified() || !crate::parser::ast::is_c_width_type_name(raw) {
        return false;
    }
    if imports.classes.contains_key(&php_symbol_key(raw)) {
        return false;
    }
    let candidate = match current_namespace {
        Some(namespace) if !namespace.is_empty() => format!("{}\\{}", namespace, raw),
        _ => raw.to_string(),
    };
    !symbols.declares_class_like(&candidate)
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

/// Resolves the class a `ptr<T>` target names in the C grammar or in `ptr_cast<T>`, applying the
/// same namespace/import rules as a `ptr<T>` type expression.
///
/// Extern function signatures, extern class fields, extern globals and `ptr_cast<T>` spell the
/// target as a bare string, so `ptr<Pair>` inside `namespace App` must name `App\Pair` (or a
/// `use` alias) exactly like `function f(ptr<Pair> $p)` does; otherwise a field access through
/// it looks up a class that does not exist, or a same-named global one with another layout.
/// The target spellings the checker treats as builtins (`int`, `float`, `bool`, `string`, `ptr`
/// and their aliases) pass through unchanged. Unshadowed C width names also stay bare, and
/// nested `ptr<...>` targets resolve recursively. A leading `\` (kept by the `ptr_cast<T>`
/// parser) marks a fully qualified target that the current namespace must not prefix.
pub(super) fn resolve_pointer_target(
    target: &str,
    current_namespace: Option<&str>,
    imports: &Imports,
    symbols: &Symbols,
) -> String {
    if let Some(inner) = target.strip_prefix("ptr<").and_then(|s| s.strip_suffix('>')) {
        return format!("ptr<{}>", resolve_pointer_target(inner, current_namespace, imports, symbols));
    }
    let (rooted, bare) = match target.strip_prefix('\\') {
        Some(rest) => (true, rest),
        None => (false, target),
    };
    if matches!(
        bare,
        "int" | "integer" | "float" | "double" | "real" | "bool" | "boolean" | "string" | "ptr"
            | "pointer"
    ) {
        return bare.to_string();
    }
    let parts = bare.split('\\').map(str::to_string).collect();
    let name = if rooted {
        Name::from_parts(NameKind::FullyQualified, parts)
    } else {
        Name::qualified(parts)
    };
    if is_unshadowed_width_name(&name, current_namespace, imports, symbols) {
        return bare.to_string();
    }
    if rooted && crate::parser::ast::is_c_width_type_name(bare) {
        return format!("\\{}", bare);
    }
    let resolved = resolve_special_or_class_name(&name, current_namespace, imports, symbols);
    if crate::parser::ast::is_c_width_type_name(&resolved) && symbols.declares_class_like(&resolved) {
        return format!("\\{}", resolved);
    }
    resolved
}

/// Resolves a `ptr_cast<T>` target without treating width names as scalar declarations.
pub(super) fn resolve_pointer_cast_target(
    target: &str, current_namespace: Option<&str>, imports: &Imports, symbols: &Symbols,
) -> String {
    let (rooted, bare) = match target.strip_prefix('\\') {
        Some(rest) => (true, rest),
        None => (false, target),
    };
    if matches!(bare, "int" | "integer" | "float" | "double" | "real" | "bool" | "boolean" | "string" | "ptr" | "pointer") {
        return bare.to_string();
    }
    let kind = if rooted { NameKind::FullyQualified } else { NameKind::Qualified };
    let name = Name::from_parts(kind, bare.split('\\').map(str::to_string).collect());
    resolve_special_or_class_name(&name, current_namespace, imports, symbols)
}

/// Resolves the class a C `ptr<T>` type names (see `resolve_pointer_target`); every other C type
/// passes through unchanged.
pub(super) fn resolve_ctype(
    c_type: &CType,
    current_namespace: Option<&str>,
    imports: &Imports,
    symbols: &Symbols,
) -> CType {
    let CType::TypedPtr(target) = c_type else {
        return c_type.clone();
    };
    CType::TypedPtr(resolve_pointer_target(target, current_namespace, imports, symbols))
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

/// Resolves a function reference while preserving PHP fallback for conditional namespace symbols.
/// Qualified names and explicit function imports keep their exact binding even if it is pruned.
pub(super) fn resolve_function_reference(
    name: &Name,
    current_namespace: Option<&str>,
    imports: &Imports,
    symbols: &Symbols,
) -> Name {
    if let Some(resolved) = super::generic_calls::resolve_function_reference(
        name, current_namespace, imports, symbols,
    ) {
        return resolved;
    }
    let canonical = resolve_function_name(name, current_namespace, imports, symbols);
    let conditional = symbols.conditional_functions.contains(&php_symbol_key(&canonical));
    let resolved = resolved_name(canonical);
    if name.function_fallback().is_some() {
        return resolved.with_function_fallback();
    }
    if conditional && name.is_unqualified()
        && current_namespace.is_some_and(|namespace| !namespace.is_empty())
        && !imports.functions.contains_key(&php_symbol_key(name.as_str()))
    {
        if super::function_fallbacks::global_candidate(symbols, name.as_str()).is_some() {
            return resolved.with_function_fallback();
        }
    }
    resolved
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
        if matches!(name.as_str(), "PHP_OS" | "PHP_OS_FAMILY" | "SID") {
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
/// resolution: every constant the shared catalog registers unconditionally (`PHP_OS`, `SID`,
/// `STDIN`, `JSON_*`, `CURLOPT_*`, ...), so a bare mention resolves to the global even inside
/// a namespace, with or without the owning bridge or prelude being linked.
fn is_builtin_global_constant(name: &str) -> bool {
    crate::types::predefined_constants::is_registered_constant(name)
        || crate::types::pcntl_constants::is_pcntl_int_constant(name)
}
