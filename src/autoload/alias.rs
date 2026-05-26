//! Purpose:
//! Collects top-level literal `class_alias("Original", "Alias")` calls.
//! Synthesizes subclass declarations that approximate alias use in the AOT class table.
//!
//! Called from:
//! - `crate::autoload::registry::Registry::build()`
//! - `crate::autoload::collect_aliases()` after include/autoload expansion
//!
//! Key details:
//! - Runtime-dynamic alias calls are left in the program and rejected by the checker.
//! - Resolver-created include wrappers still count as top-level for included-file aliases.
//! - The alias is a subclass, not a true PHP runtime alias, so identity checks differ in documented cases.

use crate::names::{Name, NameKind};
use crate::parser::ast::{Expr, ExprKind, Program, Stmt, StmtKind};

/// Walk top-level statements for `class_alias("Orig", "Alias")` calls
/// (with literal arguments). Strip every collected call and append a
/// synthesized `class Alias extends Orig {}` declaration. Calls with
/// non-literal or runtime-dependent arguments stay in the program and are
/// rejected by the checker.
pub fn collect_aliases(program: Program) -> Program {
    let mut alias_decls: Vec<Stmt> = Vec::new();
    let mut cleaned = collect_aliases_in_top_level(program, &mut alias_decls);
    cleaned.extend(alias_decls);
    cleaned
}

/// Iterates over top-level statements, removing each `class_alias("Orig", "Alias")`
/// call with literal arguments and appending the corresponding synthesized
/// `class Alias extends Orig {}` declaration to `alias_decls`. Returns the
/// filtered program with all collected alias declarations appended at the end.
/// Non-literal or runtime-dependent `class_alias` calls remain in the program
/// and are not collected — the caller is responsible for rejecting them.
fn collect_aliases_in_top_level(program: Program, alias_decls: &mut Vec<Stmt>) -> Program {
    program
        .into_iter()
        .filter_map(|stmt| collect_aliases_in_stmt(stmt, alias_decls))
        .collect()
}

/// Inspects a single statement for a `class_alias` call. If found, pushes the
/// synthesized subclass declaration to `alias_decls` and returns `None` to remove
/// the original call from the program. Descends into `NamespaceBlock`,
/// `IncludeOnceGuard`, and `Synthetic` wrappers; all other statement kinds are
/// returned unchanged after the alias check.
fn collect_aliases_in_stmt(stmt: Stmt, alias_decls: &mut Vec<Stmt>) -> Option<Stmt> {
    if let Some((orig, alias)) = extract_class_alias(&stmt) {
        alias_decls.push(synthesise_alias_decl(&orig, &alias, stmt.span));
        return None;
    }

    let span = stmt.span;
    let attributes = stmt.attributes;
    match stmt.kind {
        StmtKind::NamespaceBlock { name, body } => Some(Stmt {
            kind: StmtKind::NamespaceBlock {
                name,
                body: collect_aliases_in_top_level(body, alias_decls),
            },
            span,
            attributes,
        }),
        StmtKind::IncludeOnceGuard { label, body } => Some(Stmt {
            kind: StmtKind::IncludeOnceGuard {
                label,
                body: collect_aliases_in_top_level(body, alias_decls),
            },
            span,
            attributes,
        }),
        StmtKind::Synthetic(body) => Some(Stmt {
            kind: StmtKind::Synthetic(collect_aliases_in_top_level(body, alias_decls)),
            span,
            attributes,
        }),
        kind => Some(Stmt {
            kind,
            span,
            attributes,
        }),
    }
}

/// Extract class alias pair from a statement if it is a literal `class_alias` call.
fn extract_class_alias(stmt: &Stmt) -> Option<(String, String)> {
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
    if args.len() < 2 || args.len() > 3 {
        return None;
    }
    if let Some(autoload_arg) = args.get(2) {
        match &autoload_arg.kind {
            ExprKind::BoolLiteral(true) => {}
            ExprKind::IntLiteral(n) if *n != 0 => {}
            _ => return None,
        }
    }
    let orig = literal_string(args.first()?)?.to_string();
    let alias = literal_string(args.get(1)?)?.to_string();
    Some((orig, alias))
}

/// Extract a string value from a literal string expression.
fn literal_string(expr: &Expr) -> Option<&str> {
    match &expr.kind {
        ExprKind::StringLiteral(s) => Some(s.as_str()),
        _ => None,
    }
}

/// Synthesize `class Alias extends Original {}` for the given pair of
/// FQNs. When the alias name itself is namespaced, wrap the declaration
/// in a `NamespaceBlock` so name resolution canonicalises it correctly.
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

    if alias_namespace_parts.is_empty() {
        class_stmt
    } else {
        let ns_name = Name::from_parts(NameKind::Qualified, alias_namespace_parts);
        Stmt::new(
            StmtKind::NamespaceBlock {
                name: Some(ns_name),
                body: vec![class_stmt],
            },
            span,
        )
    }
}
