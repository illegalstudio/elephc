//! Purpose:
//! Produces the explicit "not supported" diagnostics for PHP's `goto` statement and its
//! target labels, so both are named in the error instead of surfacing as generic
//! "Unexpected token" / "Expected ';'" syntax noise.
//!
//! Called from:
//! - `crate::parser::stmt::parse_stmt_dispatch()` for `goto` and for `label:` at statement position.
//!
//! Key details:
//! - `goto` is lexed as a reserved keyword (`Token::Goto`) so the diagnostic can name it and so
//!   the word cannot be taken as a function name, matching PHP's reserved-word list.
//! - elephc lowers structured control flow to EIR through statement-shaped passes (termination
//!   analysis, flow-sensitive type narrowing, loop/branch pruning, constant propagation). An
//!   arbitrary intra-function jump would invalidate those structural assumptions, so the
//!   construct is rejected outright rather than partially supported.
//! - A label is only reachable through `goto`, so both spellings report the same limitation.

use crate::errors::CompileError;
use crate::lexer::{SpannedToken, Token};
use crate::span::Span;

/// Shared tail explaining the supported alternatives for both `goto` diagnostics.
const GOTO_ALTERNATIVES: &str =
    "restructure the jump with `break`, `continue`, a loop flag, or an early `return`";

/// Returns the diagnostic for a `goto` statement, which elephc does not support.
pub(super) fn reject_goto_statement(span: Span) -> CompileError {
    CompileError::new(
        span,
        &format!(
            "`goto` is not supported: elephc compiles structured control flow only, so a jump \
             to an arbitrary label inside a function has no lowering. Please {}",
            GOTO_ALTERNATIVES
        ),
    )
}

/// Returns true when the token at `pos` starts a PHP `goto` label (`name:`) at statement position.
///
/// Only a plain identifier immediately followed by `:` qualifies. `Foo::bar()` lexes `::` as one
/// token, alternative-syntax `else:`/`case`/`default` use their own keyword tokens, and a ternary
/// reaches its `:` only after a `?`, so none of them are mistaken for a label.
pub(super) fn starts_goto_label(tokens: &[SpannedToken], pos: usize) -> bool {
    matches!(tokens.get(pos).map(|(token, _)| token), Some(Token::Identifier(_)))
        && matches!(tokens.get(pos + 1).map(|(token, _)| token), Some(Token::Colon))
}

/// Returns the diagnostic for a `goto` target label, naming the label that was declared.
pub(super) fn reject_goto_label(label: &str, span: Span) -> CompileError {
    CompileError::new(
        span,
        &format!(
            "`goto` labels are not supported: the label `{}:` can only be reached by `goto`, \
             which elephc does not support. Please {}",
            label, GOTO_ALTERNATIVES
        ),
    )
}
