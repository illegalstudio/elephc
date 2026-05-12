//! Top-level `class_alias("Original", "Alias")` collection.
//!
//! At compile time we synthesise `class Alias extends Original {}` for each
//! collected pair. This isn't strictly identical to PHP's runtime
//! class_alias (the alias becomes a *subclass* with its own class id rather
//! than a true name alias), but for the autoload-related cases — `new
//! Alias()`, `Alias::CONST`, `instanceof Alias`, `instanceof Original` — it
//! behaves the same as the user expects. The only divergence is `(new
//! Original()) instanceof Alias`, which would be `true` under real
//! class_alias and is `false` under the subclass model.

use crate::names::{Name, NameKind};
use crate::parser::ast::{Expr, ExprKind, Program, Stmt, StmtKind};

/// Walk top-level statements for `class_alias("Orig", "Alias")` calls
/// (with literal arguments). Strip every collected call and append a
/// synthesized `class Alias extends Orig {}` declaration. Calls with
/// non-literal arguments stay in the program and reach the runtime stub.
pub fn collect_aliases(program: Program) -> Program {
    let mut alias_decls: Vec<Stmt> = Vec::new();
    let mut cleaned: Program = program
        .into_iter()
        .filter_map(|stmt| match extract_class_alias(&stmt) {
            Some((orig, alias)) => {
                alias_decls.push(synthesise_alias_decl(&orig, &alias, stmt.span));
                None
            }
            None => Some(stmt),
        })
        .collect();
    cleaned.extend(alias_decls);
    cleaned
}

fn extract_class_alias(stmt: &Stmt) -> Option<(String, String)> {
    let StmtKind::ExprStmt(expr) = &stmt.kind else {
        return None;
    };
    let ExprKind::FunctionCall { name, args } = &expr.kind else {
        return None;
    };
    let canonical = name.as_canonical();
    if canonical.trim_start_matches('\\') != "class_alias" {
        return None;
    }
    let orig = literal_string(args.first()?)?.to_string();
    let alias = literal_string(args.get(1)?)?.to_string();
    Some((orig, alias))
}

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
