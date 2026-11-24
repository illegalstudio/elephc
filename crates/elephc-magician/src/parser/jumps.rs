//! Purpose:
//! Validates the `goto` statements of one label scope when it is parsed: a function, method or
//! closure body, or the fragment's top-level code.
//!
//! Called from:
//! - The runtime frontend for the fragment's own statements.
//! - AST-to-EvalIR lowering for each function-like body.
//!
//! Key details:
//! - PHP rejects an invalid jump when it compiles the code, before any of it runs. Validating
//!   here keeps that order: a fragment with a bad jump is refused whole, as a parse failure.
//! - A jump is valid when one of the statement lists enclosing it holds the label, by
//!   `EvalStmt::holds_jump_label`, the same rule the interpreter resolves jumps with. So
//!   execution never meets a jump it cannot complete.
//! - The search stops at a `finally` body: PHP forbids jumping out of one.
//! - Nested functions and classes are other scopes and are not searched. A closure is an
//!   expression, so the statement walk never reaches its body.

use std::collections::HashSet;

use crate::errors::EvalParseError;
use crate::eval_ir::{sequence_holds_jump_label, EvalStmt};

/// One statement list enclosing the statement being checked.
struct Enclosing<'a> {
    statements: &'a [EvalStmt],
    /// Whether this list is a `finally` body, which a jump may not leave.
    is_finally: bool,
}

/// Validates every `goto` and label of one label scope.
pub(crate) fn validate_jump_scope(body: &[EvalStmt]) -> Result<(), EvalParseError> {
    let mut labels = HashSet::new();
    collect_labels(body, &mut labels)?;
    let mut enclosing = Vec::new();
    check_statements(body, false, &mut enclosing)
}

/// Lists the statement lists a statement owns in this label scope, with whether each is a
/// `finally` body.
fn child_sequences(stmt: &EvalStmt) -> Vec<(&[EvalStmt], bool)> {
    match stmt {
        EvalStmt::If {
            then_branch,
            else_branch,
            ..
        } => vec![(then_branch.as_slice(), false), (else_branch.as_slice(), false)],
        EvalStmt::While { body, .. }
        | EvalStmt::DoWhile { body, .. }
        | EvalStmt::For { body, .. }
        | EvalStmt::Foreach { body, .. } => vec![(body.as_slice(), false)],
        EvalStmt::Switch { cases, .. } => {
            cases.iter().map(|case| (case.body.as_slice(), false)).collect()
        }
        EvalStmt::Try {
            body,
            catches,
            finally_body,
        } => {
            let mut sequences = vec![(body.as_slice(), false)];
            sequences.extend(catches.iter().map(|clause| (clause.body.as_slice(), false)));
            sequences.push((finally_body.as_slice(), true));
            sequences
        }
        _ => Vec::new(),
    }
}

/// Records every label of the scope, refusing a name declared twice.
fn collect_labels<'a>(
    statements: &'a [EvalStmt],
    labels: &mut HashSet<&'a str>,
) -> Result<(), EvalParseError> {
    for stmt in statements {
        if let EvalStmt::Label(name) = stmt {
            if !labels.insert(name.as_str()) {
                return Err(EvalParseError::InvalidJump);
            }
        }
        for (sequence, _) in child_sequences(stmt) {
            collect_labels(sequence, labels)?;
        }
    }
    Ok(())
}

/// Checks the jumps of one statement list and of the lists below it.
fn check_statements<'a>(
    statements: &'a [EvalStmt],
    is_finally: bool,
    enclosing: &mut Vec<Enclosing<'a>>,
) -> Result<(), EvalParseError> {
    enclosing.push(Enclosing {
        statements,
        is_finally,
    });
    for stmt in statements {
        if let EvalStmt::Goto(label) = stmt {
            check_jump(label, enclosing)?;
        }
        for (sequence, is_finally) in child_sequences(stmt) {
            check_statements(sequence, is_finally, enclosing)?;
        }
    }
    enclosing.pop();
    Ok(())
}

/// Checks that an enclosing statement list holds `label`, searching from the innermost one out
/// and never past a `finally` body.
fn check_jump(label: &str, enclosing: &[Enclosing<'_>]) -> Result<(), EvalParseError> {
    for list in enclosing.iter().rev() {
        if sequence_holds_jump_label(list.statements, label) {
            return Ok(());
        }
        if list.is_finally {
            break;
        }
    }
    Err(EvalParseError::InvalidJump)
}
