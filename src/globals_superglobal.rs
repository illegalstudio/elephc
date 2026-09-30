//! Purpose:
//! Turns `$GLOBALS["name"]` into the global variable it names, which is the shape php programs
//! actually write and the only one this compiler can serve.
//!
//! Called from:
//! - `crate::optimize::fold` (both halves), which already walks every expression and statement in
//!   the program and is the last phase every pipeline runs before the checker.
//!
//! Key details:
//! - `$GLOBALS` did not exist here at all. MEASURED on `php -n` 8.5.6 against elephc:
//!
//!   ```text
//!   $text = "Hello";
//!   function r() { return $GLOBALS["text"]; }
//!   php     Hello
//!   elephc  Warning: Undefined variable $GLOBALS / Warning: Trying to access array offset on null
//!   ```
//!
//!   php-src's own `filters/basic.phpt` reads its fixture that way, and six tests under
//!   `ext/standard/tests/file` do the same.
//! - Inside a function the rewrite needs a `global $name;` beside it, because a bare `$name` there
//!   is a LOCAL. That is the whole hazard: a body that ALSO spells `$name` as its own variable
//!   would have the two merged, silently. So the rewrite is refused whenever the body spells the
//!   name at all — parameters included — and such a body keeps the behaviour it has today.
//!   `prelude_prune::usage::collect` is what answers that question: it records every variable name
//!   the source SPELLS, and a `$GLOBALS["text"]` subscript spells `GLOBALS`, never `text`.
//! - At the TOP LEVEL there is nothing to declare: main's scope IS the global one, so the
//!   subscript becomes the variable and nothing is prepended.
//! - A CLOSURE body is refused outright. `global` inside a closure loses its write here (a known,
//!   separate defect), so rewriting into one would trade a warning for a wrong answer.
//! - A non-literal key (`$GLOBALS[$name]`) is left exactly as it was: it names no variable this
//!   pass can see, and php's own dynamic behaviour is not modelled.

use std::cell::RefCell;
use std::collections::HashSet;

use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};
use crate::span::Span;

thread_local! {
    /// One entry per function-like body currently being folded; empty at the top level.
    static SCOPES: RefCell<Vec<Scope>> = const { RefCell::new(Vec::new()) };
}

/// What one function-like body owes and what it may not touch.
struct Scope {
    /// Names the body already spells as plain variables, which must not be merged with a global.
    blocked: HashSet<String>,
    /// Names actually rewritten, which owe a `global` declaration at the top of the body.
    rewritten: HashSet<String>,
    /// A closure body disables the rewrite entirely.
    enabled: bool,
}

/// Returns the global variable a `$GLOBALS["name"]` subscript names, or `None` to leave it alone.
pub(crate) fn read_target(array: &Expr, index: &Expr) -> Option<String> {
    let ExprKind::Variable(base) = &array.kind else {
        return None;
    };
    if base != "GLOBALS" {
        return None;
    }
    let ExprKind::StringLiteral(name) = &index.kind else {
        return None;
    };
    claim(name.clone())
}

/// The WRITE shape, `$GLOBALS["name"] = …`, which the parser gives a statement of its own.
pub(crate) fn write_target(array: &str, index: &Expr) -> Option<String> {
    if array != "GLOBALS" {
        return None;
    }
    let ExprKind::StringLiteral(name) = &index.kind else {
        return None;
    };
    claim(name.clone())
}

/// Accepts a name for rewriting in the body being folded, recording what it will owe.
///
/// An EMPTY stack refuses. That is not a formality: `optimize::propagate` calls `fold_expr` again
/// on individual expressions, from inside function bodies, without any of this scoping — and it
/// was rewriting there, past the very guard that had refused the same subscript moments earlier.
/// Only a walk that declared which body it is in may rewrite; [`fold_program`] is what the top
/// level declares.
fn claim(name: String) -> Option<String> {
    SCOPES.with(|scopes| {
        let mut scopes = scopes.borrow_mut();
        let scope = scopes.last_mut()?;
        if !scope.enabled || scope.blocked.contains(&name) {
            return None;
        }
        scope.rewritten.insert(name.clone());
        Some(name)
    })
}

/// Folds the whole program with the TOP-LEVEL scope declared.
///
/// Main's scope IS the global one, so nothing is blocked and nothing is declared: a subscript
/// there simply becomes the variable it names.
pub(crate) fn fold_program<F>(program: Vec<Stmt>, fold: F) -> Vec<Stmt>
where
    F: FnOnce(Vec<Stmt>) -> Vec<Stmt>,
{
    push(Scope { blocked: HashSet::new(), rewritten: HashSet::new(), enabled: true });
    let program = fold(program);
    pop();
    program
}

/// Folds one function or method body, declaring `global` for every name it rewrote.
///
/// `params` are the parameter names, which count as spelled: `function f($text)` reading
/// `$GLOBALS["text"]` must keep reading the GLOBAL, and binding the parameter would lose it.
pub(crate) fn fold_function_body<F>(params: &[String], body: Vec<Stmt>, fold: F) -> Vec<Stmt>
where
    F: FnOnce(Vec<Stmt>) -> Vec<Stmt>,
{
    let mut blocked = crate::prelude_prune::usage::collect(&body).variables;
    blocked.extend(params.iter().cloned());
    push(Scope { blocked, rewritten: HashSet::new(), enabled: true });
    let body = fold(body);
    let owed = pop();
    if owed.is_empty() {
        return body;
    }
    let mut names: Vec<String> = owed.into_iter().collect();
    names.sort();
    let mut declared = Vec::with_capacity(body.len() + 1);
    declared.push(Stmt::new(StmtKind::Global { vars: names }, Span::synthetic()));
    declared.extend(body);
    declared
}

/// Folds a closure body with the rewrite switched off; see the module preamble.
pub(crate) fn fold_closure_body<F>(body: Vec<Stmt>, fold: F) -> Vec<Stmt>
where
    F: FnOnce(Vec<Stmt>) -> Vec<Stmt>,
{
    push(Scope { blocked: HashSet::new(), rewritten: HashSet::new(), enabled: false });
    let body = fold(body);
    pop();
    body
}

fn push(scope: Scope) {
    SCOPES.with(|scopes| scopes.borrow_mut().push(scope));
}

fn pop() -> HashSet<String> {
    SCOPES.with(|scopes| {
        scopes
            .borrow_mut()
            .pop()
            .map(|scope| scope.rewritten)
            .unwrap_or_default()
    })
}
