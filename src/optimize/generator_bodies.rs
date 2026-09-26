//! Purpose:
//! Keeps a declaration that PHP calls a generator looking like one through the AST passes.
//!
//! Called from:
//! - `crate::optimize::target_guards::fold_callable_body` (pre-check target folding),
//!   `crate::optimize::propagate`, `crate::optimize::control::prune`, and
//!   `crate::optimize::control::dce`, at every site that rewrites a function, method, or
//!   closure body.
//!
//! Key details:
//! - PHP decides whether a declaration is a GENERATOR syntactically, before any folding. The
//!   checker records that on `FunctionSig::is_generator` and lowering reads the bit. A declared
//!   `: Generator` return is not that bit: a factory declares one without containing `yield`
//!   (issue #1086).
//! - Pre-check target folding runs before the checker records the bit, so a fold that deletes
//!   the last `yield` would make the declaration look like an ordinary function.
//! - Propagation and pruning still stop at a block's first terminator, so an unreachable
//!   `yield` is dropped and the body the coroutine lowers from no longer matches that bit
//!   (issue #673).
//! - Dead-code elimination deletes a yield for a different reason: an `elseif` chain survives
//!   pruning with this guard engaged and the body restored, then DCE collapses the chain and
//!   takes the token with it (issue #1085). DCE is wrapped for that reason.
//! - Both of PHP's ways to spell an immediately-complete generator hit it:
//!   `function g() { if (false) { yield 1; } return; }` and `function g() { return; yield; }`.
//! - Keeping the un-rewritten body is the conservative answer. The retained `yield` is
//!   unreachable by construction, so the cost is one unexecuted statement in a body that is now
//!   lowered as the coroutine it is — and it is paid only by a body whose every `yield` is dead.

use crate::parser::ast::Stmt;

/// Applies `rewrite` to a callable body, keeping the original if the rewrite would leave it
/// with NO `yield` at all.
///
/// The test is "any yield survives", not "the last yield survives": a rewrite that deletes
/// some yields and keeps others is accepted, because the body still reads as a generator.
/// Only a rewrite that empties the set is refused.
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
