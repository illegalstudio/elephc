//! Purpose:
//! Rewrites a function `static` variable that a closure in the same body captures BY REFERENCE
//! into a hidden program global bound by reference, which every backend path already supports.
//!
//! Called from:
//! - `crate::pipeline::compile()`, before the type checker reads the declarations.
//!
//! Key details:
//! - php's `static $x;` binds `$x` to storage that outlives every call of the function. A
//!   per-function hidden global has exactly that lifetime, and `global $hidden; $x = &$hidden;`
//!   binds the local the same way `static` does, so `use (&$x)` captures the persistent storage.
//!   The backend could not capture a static slot by reference: Symfony's
//!   `HttpClientTrait::normalizeBody()` (`static $cookie;` captured by an `array_walk_recursive`
//!   closure) refused `CurlHttpClient`, sent it to the interpreter, and cost the Symfony demo a
//!   whole second compile.
//! - Only `static $x;` and `static $x = null;` are rewritten: an unset global and an
//!   uninitialized static both read `null`, so no initializer has to run once. Any other
//!   initializer keeps the ordinary static lowering.
//! - The hidden name is unique per function or per `Class::method`, so two functions keep two
//!   statics, as php does.

use std::collections::HashSet;

use crate::parser::ast::visit::{walk_nodes, Node};
use crate::parser::ast::{ClassMethod, Expr, ExprKind, Program, Stmt, StmtKind};

/// Applies the rewrite to every function and class-like method of `program`.
pub fn rewrite_captured_statics(program: Program) -> Program {
    program.into_iter().map(|stmt| rewrite_stmt(stmt, None)).collect()
}

fn rewrite_stmt(mut stmt: Stmt, namespace: Option<&str>) -> Stmt {
    match &mut stmt.kind {
        StmtKind::NamespaceBlock { name, body } => {
            let ns = name.as_ref().map(|name| name.as_str().to_string());
            let taken = std::mem::take(body);
            *body = taken
                .into_iter()
                .map(|stmt| rewrite_stmt(stmt, ns.as_deref()))
                .collect();
        }
        StmtKind::FunctionDecl { name, body, .. } => {
            let owner = match namespace {
                Some(ns) => format!("{ns}\\{name}"),
                None => name.clone(),
            };
            rewrite_body(body, &owner);
        }
        StmtKind::ClassDecl { name, methods, .. }
        | StmtKind::TraitDecl { name, methods, .. }
        | StmtKind::EnumDecl { name, methods, .. } => {
            let class = match namespace {
                Some(ns) => format!("{ns}\\{name}"),
                None => name.clone(),
            };
            rewrite_methods(methods, &class);
        }
        _ => {}
    }
    stmt
}

fn rewrite_methods(methods: &mut [ClassMethod], class: &str) {
    for method in methods {
        let owner = format!("{class}::{}", method.name);
        rewrite_body(&mut method.body, &owner);
    }
}

/// Rewrites the body's `static` declarations, at any statement depth, of names a closure
/// captures by reference. Symfony's `normalizeBody()` declares its static inside an `if`.
fn rewrite_body(body: &mut [Stmt], owner: &str) {
    let captured = names_captured_by_reference(body);
    if captured.is_empty() {
        return;
    }
    rewrite_statics_in(body, owner, &captured);
}

fn rewrite_statics_in(body: &mut [Stmt], owner: &str, captured: &HashSet<String>) {
    for stmt in body.iter_mut() {
        rewrite_static_stmt(stmt, owner, captured);
    }
}

fn rewrite_static_stmt(stmt: &mut Stmt, owner: &str, captured: &HashSet<String>) {
    match &mut stmt.kind {
        StmtKind::StaticVar { name, init } => {
            if !captured.contains(name) || !matches!(init.kind, ExprKind::Null) {
                return;
            }
            let hidden = hidden_global_name(owner, name);
            let span = stmt.span;
            let local = name.clone();
            stmt.kind = StmtKind::Synthetic(vec![
                Stmt::new(StmtKind::Global { vars: vec![hidden.clone()] }, span),
                Stmt::new(
                    StmtKind::RefAssign {
                        target: local,
                        source: Expr::new(ExprKind::Variable(hidden), span),
                    },
                    span,
                ),
            ]);
        }
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            rewrite_statics_in(then_body, owner, captured);
            for (_, body) in elseif_clauses {
                rewrite_statics_in(body, owner, captured);
            }
            if let Some(body) = else_body {
                rewrite_statics_in(body, owner, captured);
            }
        }
        StmtKind::IfDef {
            then_body,
            else_body,
            ..
        } => {
            rewrite_statics_in(then_body, owner, captured);
            if let Some(body) = else_body {
                rewrite_statics_in(body, owner, captured);
            }
        }
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::For { body, .. }
        | StmtKind::Foreach { body, .. }
        | StmtKind::Synthetic(body) => rewrite_statics_in(body, owner, captured),
        StmtKind::Switch { cases, default, .. } => {
            for (_, body) in cases {
                rewrite_statics_in(body, owner, captured);
            }
            if let Some(body) = default {
                rewrite_statics_in(body, owner, captured);
            }
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            rewrite_statics_in(try_body, owner, captured);
            for catch in catches {
                rewrite_statics_in(&mut catch.body, owner, captured);
            }
            if let Some(body) = finally_body {
                rewrite_statics_in(body, owner, captured);
            }
        }
        _ => {}
    }
}

/// Every variable some closure of `body` (at any depth) captures by reference.
fn names_captured_by_reference(body: &[Stmt]) -> HashSet<String> {
    let mut names = HashSet::new();
    walk_nodes(body, &mut |node| {
        if let Node::Expr(Expr {
            kind: ExprKind::Closure { capture_refs, .. },
            ..
        }) = node
        {
            names.extend(capture_refs.iter().cloned());
        }
    });
    names
}

/// The hidden global that holds `owner`'s static `name`.
fn hidden_global_name(owner: &str, name: &str) -> String {
    let owner: String = owner
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect();
    format!("__elephc_static_{owner}__{name}")
}
