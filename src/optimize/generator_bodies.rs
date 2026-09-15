//! Purpose:
//! Keeps a declaration that PHP calls a generator looking like one through the AST passes.
//!
//! Called from:
//! - `crate::optimize::propagate` and `crate::optimize::control::prune`, at every site that
//!   rewrites a function, method or closure body.
//!
//! Key details:
//! - PHP decides whether a declaration is a GENERATOR syntactically, before any folding, and the
//!   checker does the same — it types `g()` as `Generator` from the `yield` it can see. These
//!   passes run after checking, and both of them stop rewriting a block at its first terminator,
//!   so an unreachable `yield` is dropped. That leaves the two classifications disagreeing: the
//!   caller still drives a `Generator` while lowering sees an ordinary function, and the driving
//!   `foreach` spins forever on a boxed null (issue #673).
//! - Both of PHP's ways to spell an immediately-complete generator hit it:
//!   `function g() { if (false) { yield 1; } return; }` and `function g() { return; yield; }`.
//! - Keeping the un-rewritten body is the conservative answer. The retained `yield` is
//!   unreachable by construction, so the cost is one unexecuted statement in a body that is now
//!   lowered as the coroutine it is — and it is paid only by a body whose every `yield` is dead.

use crate::parser::ast::Stmt;

/// Applies `rewrite` to a callable body, unless doing so would drop its last `yield`.
pub(super) fn rewrite_preserving_yield<F>(body: Vec<Stmt>, rewrite: F) -> Vec<Stmt>
where
    F: FnOnce(Vec<Stmt>) -> Vec<Stmt>,
{
    if !crate::types::checker::yield_validation::body_contains_yield(&body) {
        return rewrite(body);
    }
    let original = body.clone();
    let rewritten = rewrite(body);
    if crate::types::checker::yield_validation::body_contains_yield(&rewritten) {
        rewritten
    } else {
        original
    }
}
