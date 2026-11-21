//! Purpose:
//! Parses PHP's `goto name;` statement and its `name:` target labels into
//! `StmtKind::Goto` and `StmtKind::GotoLabel`.
//!
//! Called from:
//! - `crate::syntax::parser::stmt::parse_stmt_dispatch()` for `goto` and for `label:` at statement position.
//!
//! Key details:
//! - `goto` is lexed as a reserved keyword (`Token::Goto`), matching PHP's reserved-word list,
//!   so the word cannot be taken as a function name.
//! - The parser only records the jump and its target. Label resolution, PHP's jump rules and
//!   the rewrite into structured control flow all live in the compiler's `goto_lowering` module, which runs on
//!   each parsed file before any pass that reads control flow.
//! - Label names are case-sensitive in PHP, so the spelling is kept exactly as written.

use crate::syntax::errors::CompileError;
use crate::syntax::lexer::{SpannedToken, Token};
use crate::syntax::parser::ast::{Stmt, StmtKind};
use crate::syntax::span::Span;

use super::blocks::expect_semicolon;

/// Parses `goto name;` with the cursor on the `goto` keyword.
///
/// PHP's grammar is `T_GOTO T_STRING ';'`: the target must be a plain identifier, never an
/// expression, so a computed or keyword target is a syntax error here as it is there.
pub(super) fn parse_goto(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1;
    let label = match tokens.get(*pos).and_then(label_text) {
        Some(label) => label.to_owned(),
        _ => {
            let at = tokens.get(*pos).map_or(span, |(_, meta)| meta.span);
            return Err(CompileError::new(
                at,
                "Expected a label name after 'goto'",
            ));
        }
    };
    *pos += 1;
    expect_semicolon(tokens, pos)?;
    Ok(Stmt::new(StmtKind::Goto(label), span))
}

/// Returns true when the token at `pos` starts a PHP `goto` label (`name:`) at statement position.
///
/// Only a plain identifier immediately followed by `:` qualifies. `Foo::bar()` lexes `::` as one
/// token, alternative-syntax `else:`/`case`/`default` use their own keyword tokens, and a ternary
/// reaches its `:` only after a `?`, so none of them are mistaken for a label.
pub(super) fn goto_label(tokens: &[SpannedToken], pos: usize) -> Option<&str> {
    if !matches!(tokens.get(pos + 1).map(|(token, _)| token), Some(Token::Colon)) {
        return None;
    }
    tokens.get(pos).and_then(label_text)
}

/// Keeps contextual eval label spellings as type or literal tokens elsewhere in the grammar.
fn label_text((token, metadata): &SpannedToken) -> Option<&str> {
    match token {
        Token::Identifier(label) => Some(label),
        Token::Enum | Token::True | Token::False | Token::Null
            if crate::syntax::parser::runtime_fragment() => token.word_spelling(metadata),
        _ => None,
    }
}

/// Parses a `name:` label with the cursor on the identifier, consuming the name and the colon.
pub(super) fn parse_goto_label(label: &str, pos: &mut usize, span: Span) -> Stmt {
    *pos += 2;
    Stmt::new(StmtKind::GotoLabel(label.to_string()), span)
}
