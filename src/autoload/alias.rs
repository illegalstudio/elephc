//! Purpose:
//! Collects top-level `class_alias("Original", "Alias")` calls whose class names are
//! compile-time constants.
//! Synthesizes subclass declarations that approximate alias use in the AOT class table.
//!
//! Called from:
//! - `crate::autoload::registry::Registry::build()`
//! - `crate::autoload::collect_aliases()` after include/autoload expansion
//!
//! Key details:
//! - A class-name argument may be a string literal, a `Name::class` constant, or a `.`
//!   concatenation of those. `Name::class` is resolved here, before the name resolver runs,
//!   against the namespace and class imports in effect at the call, tracked with the name
//!   resolver's own rules so both passes agree on the name.
//! - Runtime-dynamic alias calls are left in the program and rejected by the checker.
//! - Disabled autoload is diagnosed on eligible source forms before constant folding.
//! - Resolver-created include wrappers still count as top-level for included-file aliases.
//! - The alias is a subclass, not a true PHP runtime alias, so identity checks differ in documented cases.

use std::collections::HashMap;

use crate::errors::CompileError;

use crate::names::{php_symbol_key, Name, NameKind};
use crate::parser::ast::{
    BinOp, Expr, ExprKind, Program, StaticReceiver, Stmt, StmtKind, UseKind,
};

/// The namespace and class imports that apply to one top-level statement.
///
/// Mirrors `crate::name_resolver::statements::list::resolve_stmt_list`: a `namespace X;`
/// statement switches the namespace and drops the imports for the statements after it, a
/// namespace block starts from no imports, `use` adds class imports for the statements after
/// it, and a nested wrapper list (an included file) starts from the enclosing context without
/// leaking its own changes back out.
#[derive(Clone, Default)]
struct AliasScope {
    /// Current namespace, `None` or empty for the global namespace.
    namespace: Option<String>,
    /// Class imports keyed by the lowercased alias, mapping to the imported fully qualified name.
    class_imports: HashMap<String, String>,
}

/// Walk top-level statements for `class_alias("Orig", "Alias")` calls
/// (with compile-time-constant arguments). Strip every collected call and append a
/// synthesized `class Alias extends Orig {}` declaration. Calls with
/// runtime-dependent arguments stay in the program and are rejected by the checker.
pub fn collect_aliases(program: Program) -> Program {
    let mut alias_decls: Vec<Stmt> = Vec::new();
    let mut cleaned =
        collect_aliases_in_top_level(program, AliasScope::default(), &mut alias_decls);
    cleaned.extend(alias_decls);
    cleaned
}

/// Iterates over top-level statements, removing each `class_alias("Orig", "Alias")`
/// call with constant arguments and appending the corresponding synthesized
/// `class Alias extends Orig {}` declaration to `alias_decls`. Returns the
/// filtered statements; `collect_aliases` appends the collected declarations at the end.
/// Runtime-dependent `class_alias` calls remain in the program and are not
/// collected; the caller is responsible for rejecting them.
fn collect_aliases_in_top_level(
    program: Program,
    mut scope: AliasScope,
    alias_decls: &mut Vec<Stmt>,
) -> Program {
    program
        .into_iter()
        .filter_map(|stmt| collect_aliases_in_stmt(stmt, &mut scope, alias_decls))
        .collect()
}

/// Inspects a single statement for a `class_alias` call. If found, pushes the
/// synthesized subclass declaration to `alias_decls` and returns `None` to remove
/// the original call from the program. Updates `scope` for namespace and `use`
/// statements, descends into `NamespaceBlock`, `IncludeOnceGuard`, and `Synthetic`
/// wrappers, and returns every other statement kind unchanged after the alias check.
fn collect_aliases_in_stmt(
    stmt: Stmt,
    scope: &mut AliasScope,
    alias_decls: &mut Vec<Stmt>,
) -> Option<Stmt> {
    if let Some((orig, alias)) = extract_class_alias(&stmt, scope) {
        alias_decls.push(synthesise_alias_decl(&orig, &alias, stmt.span));
        return None;
    }

    let span = stmt.span;
    let source_mode = stmt.source_mode;
    let strict_types = stmt.strict_types;
    let attributes = stmt.attributes;
    let kind = match stmt.kind {
        StmtKind::NamespaceDecl { name } => {
            scope.namespace = Some(namespace_text(&name));
            scope.class_imports.clear();
            StmtKind::NamespaceDecl { name }
        }
        StmtKind::UseDecl { imports } => {
            for item in imports.iter().filter(|item| item.kind == UseKind::Class) {
                scope
                    .class_imports
                    .insert(php_symbol_key(&item.alias), item.name.as_canonical());
            }
            StmtKind::UseDecl { imports }
        }
        StmtKind::NamespaceBlock { name, body } => {
            let block_scope = AliasScope {
                namespace: Some(namespace_text(&name)),
                class_imports: HashMap::new(),
            };
            StmtKind::NamespaceBlock {
                name,
                body: collect_aliases_in_top_level(body, block_scope, alias_decls),
            }
        }
        StmtKind::IncludeOnceGuard { label, body } => StmtKind::IncludeOnceGuard {
            label,
            body: collect_aliases_in_top_level(body, scope.clone(), alias_decls),
        },
        StmtKind::Synthetic(body) => {
            StmtKind::Synthetic(collect_aliases_in_top_level(body, scope.clone(), alias_decls))
        }
        kind => kind,
    };
    Some(Stmt {
        kind,
        span,
        source_mode,
        strict_types,
        attributes,
    })
}

/// Returns the canonical text of a namespace declaration name, empty for the global namespace.
fn namespace_text(name: &Option<Name>) -> String {
    name.as_ref().map(Name::as_canonical).unwrap_or_default()
}

/// Extract class alias pair from a statement if it is a `class_alias` call whose class names
/// are compile-time constants.
fn extract_class_alias(stmt: &Stmt, scope: &AliasScope) -> Option<(String, String)> {
    let args = positional_class_alias_args(stmt)?;
    if let Some(autoload_arg) = args.get(2) {
        match &autoload_arg.kind {
            ExprKind::BoolLiteral(true) => {}
            ExprKind::IntLiteral(n) if *n != 0 => {}
            _ => return None,
        }
    }
    let orig = constant_class_name(args.first()?, scope)?;
    let alias = constant_class_name(args.get(1)?, scope)?;
    Some((orig, alias))
}

/// Shares the collector's direct positional call shape with diagnostic eligibility.
fn positional_class_alias_args(stmt: &Stmt) -> Option<&[Expr]> {
    let StmtKind::ExprStmt(expr) = &stmt.kind else {
        return None;
    };
    let ExprKind::FunctionCall { name, args } = &expr.kind else {
        return None;
    };
    let canonical = name.as_canonical();
    if !canonical
        .trim_start_matches('\\')
        .eq_ignore_ascii_case("class_alias")
    {
        return None;
    }
    if args.len() < 2 || args.len() > 3
        || args.iter().any(|arg| matches!(arg.kind, ExprKind::NamedArg { .. } | ExprKind::Spread(_)))
    {
        return None;
    }
    Some(args)
}

/// Diagnoses disabled autoload only on source forms the collector itself accepts.
///
/// Run before constant folding: a ternary or cast that later becomes a literal
/// must retain its unsupported source-shape diagnostic regardless of this flag.
pub(super) fn validate_alias_autoload(program: &[Stmt]) -> Result<(), CompileError> {
    let scope = AliasScope::default();
    for stmt in program {
        match &stmt.kind {
            StmtKind::NamespaceBlock { body, .. } | StmtKind::IncludeOnceGuard { body, .. }
                | StmtKind::Synthetic(body) => validate_alias_autoload(body)?,
            StmtKind::ExprStmt(_) => {
                if positional_class_alias_args(stmt).is_some_and(|args| {
                    constant_class_name(&args[0], &scope).is_some()
                        && constant_class_name(&args[1], &scope).is_some()
                        && args.get(2).is_some_and(|arg| literal_autoload_is_false(&arg.kind))
                }) {
                    return Err(CompileError::new(stmt.span, UNSUPPORTED_AUTOLOAD_FALSE));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Recognizes scalar literals PHP coerces to false for the autoload parameter.
fn literal_autoload_is_false(kind: &ExprKind) -> bool {
    match kind {
        ExprKind::BoolLiteral(false) | ExprKind::IntLiteral(0) | ExprKind::Null => true,
        ExprKind::FloatLiteral(value) => *value == 0.0,
        ExprKind::StringLiteral(value) => value.is_empty() || value == "0",
        _ => false,
    }
}

/// Explains the collector's unsupported explicitly disabled autoload mode.
const UNSUPPORTED_AUTOLOAD_FALSE: &str = "class_alias() does not support autoload=false in AOT mode; \
    omit autoload or pass true";

/// Folds a compile-time-constant class-name argument to its string value.
///
/// Accepts a string literal (already fully qualified, like every PHP class-name string), a
/// `Name::class` constant, and a `.` concatenation of those, which also covers
/// `__NAMESPACE__ . '\Alias'` because magic constants are substituted before this pass.
/// Anything else (variables, `$object::class`, calls, class constants) returns `None`, so the
/// call stays in the program and keeps the checker's diagnostic.
fn constant_class_name(expr: &Expr, scope: &AliasScope) -> Option<String> {
    match &expr.kind {
        ExprKind::StringLiteral(s) => Some(s.clone()),
        ExprKind::ClassConstant {
            receiver: StaticReceiver::Named(name),
        } => Some(resolve_class_reference(name, scope)),
        ExprKind::BinaryOp {
            left,
            op: BinOp::Concat,
            right,
        } => {
            let mut folded = constant_class_name(left, scope)?;
            folded.push_str(&constant_class_name(right, scope)?);
            Some(folded)
        }
        _ => None,
    }
}

/// Resolves the class name of a `Name::class` constant to the fully qualified name PHP
/// compiles it to.
///
/// Follows `crate::name_resolver::names::resolved_class_name` without its symbol-table
/// spelling pass (the synthesized declaration goes through the name resolver afterwards): a
/// fully qualified name is taken as written, an unqualified name through a class import of the
/// same alias, a qualified name through an import of its first segment, and anything else is
/// placed in the current namespace.
fn resolve_class_reference(name: &Name, scope: &AliasScope) -> String {
    if name.is_fully_qualified() {
        return name.as_canonical();
    }
    if name.is_unqualified() {
        if let Some(target) = name
            .last_segment()
            .and_then(|segment| scope.class_imports.get(&php_symbol_key(segment)))
        {
            return target.clone();
        }
    } else if let Some(target) = name
        .parts
        .first()
        .and_then(|first| scope.class_imports.get(&php_symbol_key(first)))
    {
        let suffix = &name.parts[1..];
        return if suffix.is_empty() {
            target.clone()
        } else {
            format!("{}\\{}", target, suffix.join("\\"))
        };
    }
    match scope.namespace.as_deref() {
        Some(namespace) if !namespace.is_empty() => {
            format!("{}\\{}", namespace, name.as_canonical())
        }
        _ => name.as_canonical(),
    }
}

/// Synthesize `class Alias extends Original {}` for the given pair of
/// FQNs. The declaration is always wrapped in a `NamespaceBlock` for the alias
/// name's own namespace (the global one when it has none): the declarations are
/// appended after the program's last statement, which may sit under a statement-form
/// `namespace X;` that must not capture a global alias name.
fn synthesise_alias_decl(orig: &str, alias: &str, span: crate::span::Span) -> Stmt {
    let orig_parts: Vec<String> = orig
        .trim_start_matches('\\')
        .split('\\')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();
    let alias_parts: Vec<String> = alias
        .trim_start_matches('\\')
        .split('\\')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    let alias_local = alias_parts.last().cloned().unwrap_or_default();
    let alias_namespace_parts = alias_parts
        .iter()
        .take(alias_parts.len().saturating_sub(1))
        .cloned()
        .collect::<Vec<_>>();

    let extends_name = Name::from_parts(NameKind::FullyQualified, orig_parts);

    let class_stmt = Stmt::new(
        StmtKind::ClassDecl {
            generics: None,
            name: alias_local,
            extends: Some(extends_name),
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            trait_uses: Vec::new(),
            properties: Vec::new(),
            methods: Vec::new(),
            constants: Vec::new(),
        },
        span,
    );

    let ns_name = (!alias_namespace_parts.is_empty())
        .then(|| Name::from_parts(NameKind::Qualified, alias_namespace_parts));
    Stmt::new(
        StmtKind::NamespaceBlock {
            name: ns_name,
            body: vec![class_stmt],
        },
        span,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses one direct alias call without changing its source positions.
    fn alias_statement() -> Stmt {
        let tokens = crate::lexer::tokenize("<?php class_alias('Original', 'Alias', false);").unwrap();
        crate::parser::parse(&tokens).unwrap().remove(0)
    }

    /// Included and synthetic wrappers preserve source eligibility before any constant folding.
    #[test]
    fn alias_autoload_diagnostics_preserve_top_level_wrappers() {
        let call = alias_statement();
        let span = call.span;
        let program = vec![
            call.clone(),
            Stmt::new(StmtKind::Synthetic(vec![call.clone()]), span),
            Stmt::new(StmtKind::IncludeOnceGuard { label: "review_include".into(), body: vec![call] }, span),
        ];
        for stmt in &program {
            let direct = match &stmt.kind {
                StmtKind::Synthetic(body) | StmtKind::IncludeOnceGuard { body, .. } => &body[0],
                _ => stmt,
            };
            assert_eq!(direct.span, span);
            assert_eq!(
                validate_alias_autoload(std::slice::from_ref(stmt)).unwrap_err().message,
                UNSUPPORTED_AUTOLOAD_FALSE,
            );
        }
        assert!(validate_alias_autoload(&program.clone()).is_err());
    }
}
