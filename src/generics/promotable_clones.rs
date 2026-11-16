//! Purpose:
//! Typed clones of user functions whose undeclared parameters receive a promotable `array<int>`:
//! the clone declares those parameters `array<int>`, so the caller hands its packed-or-hash local
//! on raw instead of boxing it into the dynamic `mixed` ABI every undeclared parameter has.
//!
//! Called from:
//! - `crate::types::checker::promotable_arrays` (requests a clone at a direct call and builds the
//!   checker's copy of its declaration).
//! - `crate::generics::monomorphize` (splices the requested clones into the program, as the
//!   fourth kind of fixpoint work).
//! - `crate::pipeline` (reachability roots) and `crate::ir_lower::function` (the PHP-visible name).
//!
//! Key details:
//! - A clone name is the original declaration name, `#__promotable_`, and the cloned positions.
//!   `#` is never part of a PHP identifier, so it cannot collide with a user function, and it
//!   stays clear of the `<` that generic instantiation names and their decoders rely on.
//! - The clone body is retagged with a fresh synthetic source identity registered under the
//!   original file. Every span-keyed checker table (`builtin_call_types`, the loop storage and
//!   retype sites, the promotable raw sites) then keeps the clone's entries apart from the
//!   original's, while diagnostics still name the same file, line and column.
//! - Warnings raised inside a clone body duplicate the original's, so `monomorphize` drops every
//!   warning whose span carries a clone identity.
//! - The checker and the AST splice build the same declaration from the same request (same
//!   positions, same source identity), which keeps the checked and the lowered body identical.

use std::cell::RefCell;
use std::collections::HashSet;

use crate::magic_constants::walker::{walk_expression, walk_program, Pass};
use crate::parser::ast::{Expr, ExprKind, MagicConstant, Program, Stmt, StmtKind, TypeExpr};
use crate::span::Span;

/// Separates the original name from the cloned positions in a clone name.
const CLONE_MARKER: &str = "#__promotable_";

std::thread_local! {
    /// Source identities handed to clone bodies in this compile, for the warning filter.
    static CLONE_SOURCE_IDS: RefCell<HashSet<u32>> = RefCell::new(HashSet::new());
}

/// One clone a direct call asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PromotableCloneRequest {
    /// The original declaration's name, as the program spells it.
    pub base: String,
    /// The clone's name (`promotable_clone_name(base, positions)`).
    pub clone: String,
    /// The undeclared by-value parameters the clone declares `array<int>`.
    pub positions: Vec<usize>,
    /// The source identity the clone body is retagged with.
    pub source_id: u32,
}

/// Returns the name of the clone of `base` that declares `positions` as `array<int>`.
pub fn promotable_clone_name(base: &str, positions: &[usize]) -> String {
    let positions: Vec<String> = positions.iter().map(usize::to_string).collect();
    format!("{}{}{}", base, CLONE_MARKER, positions.join("_"))
}

/// Returns the PHP-visible name of a clone, or `None` for any other function name.
pub fn promotable_clone_source_name(name: &str) -> Option<&str> {
    name.split_once(CLONE_MARKER).map(|(base, _)| base)
}

/// Returns a fresh source identity for a clone of the declaration at `origin`, displayed as the
/// original's file.
pub fn fresh_clone_source_id(origin: Span) -> u32 {
    let source_id = Span::fresh_synthetic_source_id();
    if let Some(path) = origin.source_path() {
        Span::register_source_path(source_id, path);
    }
    CLONE_SOURCE_IDS.with(|ids| ids.borrow_mut().insert(source_id));
    source_id
}

/// Returns whether `span` lies inside a clone body.
pub fn is_clone_span(span: Span) -> bool {
    let source_id = span.source_id();
    source_id != 0 && CLONE_SOURCE_IDS.with(|ids| ids.borrow().contains(&source_id))
}

/// Returns the declared type a cloned parameter receives.
pub fn promotable_param_type() -> TypeExpr {
    TypeExpr::Array(Box::new(TypeExpr::Int))
}

/// Returns `span` with the same coordinates under `source_id`.
pub fn retag_span(span: Span, source_id: u32) -> Span {
    if !span.identifies_a_node() {
        return span;
    }
    Span::with_end_from(Span::new_in_source(span.line, span.col, source_id), span)
}

/// Retags every statement and expression span of `body` with `source_id`.
pub fn retag_body(body: Vec<Stmt>, source_id: u32) -> Vec<Stmt> {
    walk_program(body, &mut Retag { source_id })
}

/// Retags every span of one expression, such as a parameter default.
pub fn retag_expr(expr: Expr, source_id: u32) -> Expr {
    walk_expression(expr, &mut Retag { source_id })
}

/// Rebuilds a body under a new source identity, leaving everything else as it was.
struct Retag {
    source_id: u32,
}

impl Pass for Retag {
    /// Keeps magic constants, which earlier pipeline phases have already substituted.
    fn transform_magic(&self, _span: Span, mc: MagicConstant) -> ExprKind {
        ExprKind::MagicConstant(mc)
    }

    /// Moves a rebuilt statement to the clone's source identity.
    fn transform_stmt(&self, mut stmt: Stmt) -> Stmt {
        stmt.span = retag_span(stmt.span, self.source_id);
        stmt
    }

    /// Moves a rebuilt expression to the clone's source identity.
    fn transform_expr_span(&self, span: Span) -> Span {
        retag_span(span, self.source_id)
    }
}

/// Prepends the declaration of every requested clone the program does not carry yet, and returns
/// the new program plus how many were added (zero is the fixpoint signal).
pub fn splice_promotable_clones(
    program: Program,
    requested: &[PromotableCloneRequest],
) -> (Program, usize) {
    if requested.is_empty() {
        return (program, 0);
    }
    let mut declared: HashSet<String> = program
        .iter()
        .filter_map(|stmt| match &stmt.kind {
            StmtKind::FunctionDecl { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();
    let mut clones = Vec::new();
    for request in requested {
        if declared.contains(&request.clone) {
            continue;
        }
        let Some(original) = program.iter().find(|stmt| {
            matches!(&stmt.kind, StmtKind::FunctionDecl { name, .. } if name == &request.base)
        }) else {
            continue;
        };
        let Some(clone) = clone_declaration(original, request) else {
            continue;
        };
        declared.insert(request.clone.clone());
        clones.push(clone);
    }
    let added = clones.len();
    clones.extend(program);
    (clones, added)
}

/// Builds the clone declaration a request describes from the original statement.
fn clone_declaration(original: &Stmt, request: &PromotableCloneRequest) -> Option<Stmt> {
    let mut clone = original.clone();
    let StmtKind::FunctionDecl {
        name, params, body, ..
    } = &mut clone.kind
    else {
        return None;
    };
    *name = request.clone.clone();
    for (position, (_, declared, default, _)) in params.iter_mut().enumerate() {
        if request.positions.contains(&position) {
            *declared = Some(promotable_param_type());
        }
        if let Some(value) = default.take() {
            *default = Some(retag_expr(value, request.source_id));
        }
    }
    *body = retag_body(std::mem::take(body), request.source_id);
    clone.span = retag_span(clone.span, request.source_id);
    Some(clone)
}

/// Returns the clone names the program now declares, as reachability roots: no call site names a
/// clone, lowering redirects to it.
pub fn promotable_clone_roots(requested: &[PromotableCloneRequest]) -> Vec<String> {
    let mut roots: Vec<String> = requested.iter().map(|request| request.clone.clone()).collect();
    roots.sort();
    roots.dedup();
    roots
}
