//! Purpose:
//! Rewrites a closed-world `extract($place, ... | EXTR_REFS)` statement into the reference
//! bindings it performs, before the checker runs (laravel/serializable-closure's
//! `Native::__unserialize()` extracts `$this->code['use']` by reference).
//!
//! Called from:
//! - `crate::pipeline`, after the autoloaded files are spliced, in a closed world only.
//!
//! Key details:
//! - A closed world has no interpreter scope: the locals `extract()` can bind are the names the
//!   function body spells, so each becomes
//!   `if (is_array($place) && array_key_exists('n', $place)) { $n = &$place['n']; }`. A key
//!   naming no such local creates a variable nothing can read.
//! - The bindings are real `$n = &...` statements, so the checker promotes the place and the
//!   locals as it does for written references, and their cells are released with the frame.
//! - Only the statement form with literal flags (`EXTR_REFS`, optionally with `EXTR_OVERWRITE`)
//!   and a place that can be re-read (a variable, `$this`, a property or element of one with
//!   literal or variable keys) is rewritten; the count `extract()` returns is unused there.

use std::collections::BTreeSet;

use crate::parser::ast::visit::{walk_nodes, walk_nodes_mut, Node, NodeMut};
use crate::parser::ast::{BinOp, Expr, ExprKind, Program, Stmt, StmtKind};

/// php's `EXTR_REFS`.
const EXTR_REFS: i64 = 256;

/// Rewrites every by-reference `extract()` statement of `program`'s function bodies.
pub(crate) fn rewrite(program: &mut Program) {
    walk_nodes_mut(program, &mut |node| match node {
        NodeMut::Stmt(stmt) => match &mut stmt.kind {
            StmtKind::FunctionDecl { body, .. } => rewrite_body(body),
            StmtKind::ClassDecl { methods, .. }
            | StmtKind::TraitDecl { methods, .. }
            | StmtKind::EnumDecl { methods, .. } => {
                for method in methods {
                    rewrite_body(&mut method.body);
                }
            }
            _ => {}
        },
        NodeMut::Expr(expr) => {
            if let ExprKind::Closure { body, .. } = &mut expr.kind {
                rewrite_body(body);
            }
        }
    });
}

/// Rewrites the by-reference `extract()` statements of one function body.
fn rewrite_body(body: &mut Vec<Stmt>) {
    let mut found = false;
    walk_nodes(body, &mut |node| {
        if let Node::Stmt(stmt) = node {
            found |= by_ref_extract(stmt).is_some();
        }
    });
    if !found {
        return;
    }
    let names = local_names(body);
    walk_nodes_mut(body, &mut |node| {
        if let NodeMut::Stmt(stmt) = node {
            if let Some(place) = by_ref_extract(stmt).cloned() {
                let root = place_root(&place);
                let bindings = names
                    .iter()
                    .filter(|name| Some(name.as_str()) != root)
                    .map(|name| binding(&place, name, stmt.span))
                    .collect();
                stmt.kind = StmtKind::Synthetic(bindings);
            }
        }
    });
}

/// The place of an `extract($place, <flags with EXTR_REFS>);` statement this pass rewrites.
fn by_ref_extract(stmt: &Stmt) -> Option<&Expr> {
    let StmtKind::ExprStmt(call) = &stmt.kind else {
        return None;
    };
    let ExprKind::FunctionCall { name, args } = &call.kind else {
        return None;
    };
    if !name.as_str().trim_start_matches('\\').eq_ignore_ascii_case("extract") {
        return None;
    }
    let [place, flags] = args.as_slice() else {
        return None;
    };
    let flags = literal_flags(flags)?;
    (flags & EXTR_REFS != 0 && flags & !EXTR_REFS == 0 && is_rereadable_place(place))
        .then_some(place)
}

/// The value of literal flags: integers, `EXTR_OVERWRITE` / `EXTR_REFS`, and `|` of them.
fn literal_flags(flags: &Expr) -> Option<i64> {
    match &flags.kind {
        ExprKind::IntLiteral(value) => Some(*value),
        ExprKind::ConstRef(constant) => match constant.as_str().trim_start_matches('\\') {
            "EXTR_OVERWRITE" => Some(0),
            "EXTR_REFS" => Some(EXTR_REFS),
            _ => None,
        },
        ExprKind::BinaryOp { left, op: BinOp::BitOr, right } => {
            Some(literal_flags(left)? | literal_flags(right)?)
        }
        _ => None,
    }
}

/// Whether reading `expr` again names the same place: a variable, `$this`, or a property or an
/// element of one with literal or variable keys.
fn is_rereadable_place(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Variable(_) | ExprKind::This => true,
        ExprKind::PropertyAccess { object, .. } => is_rereadable_place(object),
        ExprKind::ArrayAccess { array, index } => {
            is_rereadable_place(array)
                && matches!(
                    index.kind,
                    ExprKind::StringLiteral(_) | ExprKind::IntLiteral(_) | ExprKind::Variable(_)
                )
        }
        _ => false,
    }
}

/// The variable a place is read through (`$data` of `$data['use']`), if any.
fn place_root(place: &Expr) -> Option<&str> {
    match &place.kind {
        ExprKind::Variable(name) => Some(name),
        ExprKind::PropertyAccess { object, .. } => place_root(object),
        ExprKind::ArrayAccess { array, .. } => place_root(array),
        _ => None,
    }
}

/// The local names a function body spells.
fn local_names(body: &[Stmt]) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut add = |name: &str| {
        if name != "this"
            && !crate::names::is_generated_local_name(name)
            && !crate::globals_array::is_alias(name)
            && !crate::superglobals::is_superglobal(name)
        {
            names.insert(name.to_string());
        }
    };
    walk_nodes(body, &mut |node| match node {
        Node::Expr(Expr { kind: ExprKind::Variable(name), .. }) => add(name),
        Node::Stmt(stmt) => match &stmt.kind {
            StmtKind::Assign { name, .. }
            | StmtKind::TypedAssign { name, .. }
            | StmtKind::RefAssign { target: name, .. }
            | StmtKind::ArrayAssign { array: name, .. }
            | StmtKind::ArrayPush { array: name, .. } => add(name),
            StmtKind::Foreach { key_var, value_var, .. } => {
                add(value_var);
                if let Some(key) = key_var {
                    add(key);
                }
            }
            _ => {}
        },
        _ => {}
    });
    names
}

/// `if (is_array($place) && array_key_exists('n', $place)) { $n = &$place['n']; }`
fn binding(place: &Expr, name: &str, span: crate::span::Span) -> Stmt {
    use crate::synthetic_class::{e_binop, e_call, e_index, e_str, s_if};
    let present = e_binop(
        e_call("is_array", vec![place.clone()]),
        BinOp::And,
        e_call("array_key_exists", vec![e_str(name), place.clone()]),
    );
    let bind = Stmt::new(
        StmtKind::RefAssign {
            target: name.to_string(),
            source: e_index(place.clone(), e_str(name)),
        },
        span,
    );
    s_if(present, vec![bind], vec![], None)
}
