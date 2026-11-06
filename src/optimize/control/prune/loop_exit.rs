//! Purpose:
//! Prunes constant control-flow loop exit cases.
//! Rewrites statements or expressions whose compile-time condition is known while preserving required effects.
//!
//! Called from:
//! - `crate::optimize::control::prune`
//!
//! Key details:
//! - Loop exits, empty bodies, and effectful conditions must be handled before removing structural statements.

use super::super::*;

/// Returns true if the statement list holds a `break` or `continue` that leaves the loop whose
/// body it is, considering nested control flow recursively.
///
/// DEPTH-AWARE. A `break n` inside `k` nested loops leaves this one exactly when `n > k`, so the
/// walk recurses INTO nested loops with the depth raised, rather than stopping at them. Stopping
/// was wrong: `do { foreach ($a as $x) { break 2; } } while (false);` has an exit, and treating
/// the `do` as a run-once block dissolved it, leaving the `break 2` with no loop to leave — the
/// lowering emits a trap there. MEASURED: reference prints `in1 end`; elephc crashed after `in1`
/// with an illegal instruction. The include-return rewrite produces this exact shape (a `return`
/// inside a loop becomes `break 2` out of the include's `do { … } while (false)`), which is how
/// it surfaced.
///
/// `Synthetic` blocks and the include wrappers are walked at the same depth — they are grouping,
/// not loops. A `switch` is walked at the same depth too, which over-counts a `break` that only
/// leaves the switch; that errs on the side of keeping the loop, which is always safe.
pub(crate) fn block_contains_loop_exit(body: &[Stmt]) -> bool {
    block_leaves_enclosing_loop(body, 0)
}

/// Whether `body`, nested `depth` loops below the loop in question, leaves that loop.
fn block_leaves_enclosing_loop(body: &[Stmt], depth: usize) -> bool {
    body.iter().any(|stmt| stmt_leaves_enclosing_loop(stmt, depth))
}

/// Whether `stmt`, nested `depth` loops below the loop in question, leaves that loop.
fn stmt_leaves_enclosing_loop(stmt: &Stmt, depth: usize) -> bool {
    match &stmt.kind {
        StmtKind::Break(levels) | StmtKind::Continue(levels) => *levels > depth,
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            block_leaves_enclosing_loop(then_body, depth)
                || elseif_clauses
                    .iter()
                    .any(|(_, body)| block_leaves_enclosing_loop(body, depth))
                || else_body
                    .as_ref()
                    .is_some_and(|body| block_leaves_enclosing_loop(body, depth))
        }
        StmtKind::IfDef {
            then_body, else_body, ..
        } => {
            block_leaves_enclosing_loop(then_body, depth)
                || else_body
                    .as_ref()
                    .is_some_and(|body| block_leaves_enclosing_loop(body, depth))
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            block_leaves_enclosing_loop(try_body, depth)
                || catches
                    .iter()
                    .any(|catch| block_leaves_enclosing_loop(&catch.body, depth))
                || finally_body
                    .as_ref()
                    .is_some_and(|body| block_leaves_enclosing_loop(body, depth))
        }
        StmtKind::Switch { cases, default, .. } => {
            cases
                .iter()
                .any(|(_, body)| block_leaves_enclosing_loop(body, depth))
                || default
                    .as_ref()
                    .is_some_and(|body| block_leaves_enclosing_loop(body, depth))
        }
        StmtKind::Synthetic(body)
        | StmtKind::NamespaceBlock { body, .. }
        | StmtKind::IncludeOnceGuard { body, .. } => block_leaves_enclosing_loop(body, depth),
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::For { body, .. }
        | StmtKind::Foreach { body, .. } => block_leaves_enclosing_loop(body, depth + 1),
        _ => false,
    }
}
