//! Purpose:
//! Compiles `foreach ($object as $key => &$value)` over a `stdClass` (laravel/serializable-closure's
//! `Native::mapPointers()`). A stdClass keeps its properties in a per-instance hash, so the walk is
//! checked and lowered as the equivalent loop over the property names that binds each one:
//! `foreach (array_keys(get_object_vars($object)) as $key) { $value = &$object->{$key}; ... }`.
//!
//! Called from:
//! - `crate::types::checker::stmt_check::control_flow` (records the site, checks the rewrite).
//! - `crate::ir_lower::stmt::typed_foreach::lower_foreach` (lowers the rewrite at a recorded site).
//! - `crate::ir_lower::program` (publishes the checker's sites for one lowering invocation).
//!
//! Key details:
//! - Only a variable source is rewritten: the rewrite reads it once per iteration.
//! - The names are read when the walk starts, as php's by-reference walk of a stdClass visits
//!   the properties present then; a property the body adds is not visited (php does visit it).
//! - The rewritten loop carries the SOURCE expression's span, so it is not itself a recorded
//!   site; the reference binding carries the statement's.

use std::cell::RefCell;
use std::collections::HashSet;

use crate::names::{Name, NameKind};
use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};
use crate::span::Span;

thread_local! {
    /// The by-reference stdClass walks the checker rewrote, for the current lowering invocation.
    static SITES: RefCell<HashSet<Span>> = RefCell::new(HashSet::new());
}

/// Restores the enclosing lowering invocation's sites on drop.
pub(crate) struct SitesGuard {
    previous: HashSet<Span>,
}

impl Drop for SitesGuard {
    fn drop(&mut self) {
        let previous = std::mem::take(&mut self.previous);
        SITES.with(|sites| *sites.borrow_mut() = previous);
    }
}

/// Publishes the checker's rewritten walks for one program-lowering invocation.
pub(crate) fn set_sites(sites: HashSet<Span>) -> SitesGuard {
    let previous = SITES.with(|set| std::mem::replace(&mut *set.borrow_mut(), sites));
    SitesGuard { previous }
}

/// Whether the checker rewrote the `foreach` at `span`.
pub(crate) fn is_site(span: Span) -> bool {
    SITES.with(|sites| sites.borrow().contains(&span))
}

/// The loop the walk at `span` is checked and lowered as, or `None` for a source the rewrite does
/// not take (anything but a variable).
pub(crate) fn rewrite(
    array: &Expr,
    key_var: Option<&str>,
    value_var: &str,
    body: &[Stmt],
    span: Span,
) -> Option<Stmt> {
    if !matches!(array.kind, ExprKind::Variable(_)) {
        return None;
    }
    let key = key_var
        .map(str::to_string)
        .unwrap_or_else(|| format!("__elephc_stdclass_walk_key_{}_{}", span.line, span.col));
    let names = call("array_keys", vec![call("get_object_vars", vec![array.clone()])]);
    let property = Expr::new(
        ExprKind::DynamicPropertyAccess {
            object: Box::new(array.clone()),
            property: Box::new(Expr::new(ExprKind::Variable(key.clone()), span)),
        },
        span,
    );
    let mut walk_body = Vec::with_capacity(body.len() + 1);
    walk_body.push(Stmt::new(
        StmtKind::RefAssign {
            target: value_var.to_string(),
            source: property,
        },
        span,
    ));
    walk_body.extend(body.iter().cloned());
    Some(Stmt::new(
        StmtKind::Foreach {
            array: names,
            key_var: None,
            value_var: key,
            value_by_ref: false,
            body: walk_body,
        },
        array.span,
    ))
}

/// `\name(args)` with a synthesized span, so no checker fact keyed by a source span lands on it.
fn call(name: &str, args: Vec<Expr>) -> Expr {
    Expr::new(
        ExprKind::FunctionCall {
            name: Name::from_parts(NameKind::FullyQualified, vec![name.to_string()]),
            args,
        },
        Span::dummy(),
    )
}
