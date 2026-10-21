//! Purpose:
//! Recovers the statement parser after a syntax error without consuming the next statement.
//!
//! Called from:
//! - `crate::parser::stmt::blocks` while accumulating block diagnostics.
//!
//! Key details:
//! - Parenthesis and bracket nesting suppress boundary detection until depth returns to zero.
//! - `recover_after_failed_stmt` handles a compound statement that already reported its
//!   nested block's errors and consumed that block's `}`: it must not skip from there, or it
//!   eats the enclosing block's `}` / the next statement and cascades into bogus errors.

use crate::lexer::{SpannedToken, Token};

/// Advances `pos` to the next PHP statement boundary following a parse error.
pub(crate) fn recover_to_statement_boundary(tokens: &[SpannedToken], pos: &mut usize) {
    recover_with_open_delimiters(tokens, pos, 0, 0);
}

/// Advances `pos` to the next statement boundary, starting inside `paren_depth` open `(` and
/// `bracket_depth` open `[` that the failed statement had already entered.
fn recover_with_open_delimiters(
    tokens: &[SpannedToken],
    pos: &mut usize,
    mut paren_depth: usize,
    mut bracket_depth: usize,
) {
    let start = *pos;
    // Delimiters the failed statement opened and never closed (`return [1, ;`) must not keep
    // recovery from stopping at this statement's `;` or the enclosing `}`: they may never be
    // closed, and skipping to EOF lost every later diagnostic. Only delimiters opened during
    // recovery itself (and braces) still suppress boundaries.
    let (inherited_paren, inherited_bracket) = (paren_depth, bracket_depth);
    let mut brace_depth = 0usize;
    let at_statement_level = |paren: usize, bracket: usize, brace: usize| {
        brace == 0 && paren <= inherited_paren && bracket <= inherited_bracket
    };

    while *pos < tokens.len() {
        match tokens[*pos].0 {
            Token::LParen => {
                paren_depth += 1;
                *pos += 1;
            }
            Token::RParen => {
                paren_depth = paren_depth.saturating_sub(1);
                *pos += 1;
            }
            Token::LBracket => {
                bracket_depth += 1;
                *pos += 1;
            }
            Token::RBracket => {
                bracket_depth = bracket_depth.saturating_sub(1);
                *pos += 1;
            }
            Token::Semicolon if at_statement_level(paren_depth, bracket_depth, brace_depth) => {
                *pos += 1;
                break;
            }
            Token::LBrace if inherited_paren + inherited_bracket > 0 => {
                brace_depth += 1;
                *pos += 1;
            }
            Token::RBrace if brace_depth > 0 => {
                brace_depth -= 1;
                *pos += 1;
            }
            Token::RBrace if at_statement_level(paren_depth, bracket_depth, brace_depth) => break,
            Token::RBrace
            | Token::EndDeclare
            | Token::EndIf
            | Token::EndWhile
            | Token::EndFor
            | Token::EndForeach
            | Token::EndSwitch
            | Token::Eof
                if paren_depth == 0 && bracket_depth == 0 =>
            {
                break;
            }
            Token::Echo
            | Token::Print
            | Token::Variable(_)
            | Token::This
            | Token::PlusPlus
            | Token::MinusMinus
            | Token::Class
            | Token::Enum
            | Token::ReadOnly
            | Token::Packed
            | Token::Interface
            | Token::Trait
            | Token::Abstract
            | Token::Final
            | Token::Function
            | Token::Namespace
            | Token::Use
            | Token::Declare
            | Token::Return
            | Token::Throw
            | Token::Include
            | Token::IncludeOnce
            | Token::Require
            | Token::RequireOnce
            | Token::Const
            | Token::Global
            | Token::Static
            | Token::Goto
            | Token::Identifier(_)
            | Token::Self_
            | Token::Parent
            | Token::Backslash
            | Token::Question
            | Token::Switch
            | Token::If
            | Token::IfDef
            | Token::Try
            | Token::While
            | Token::Do
            | Token::For
            | Token::Foreach
            | Token::Break
            | Token::Continue
                if *pos > start && paren_depth == 0 && bracket_depth == 0 =>
            {
                break;
            }
            _ => {
                *pos += 1;
            }
        }
    }

    if *pos == start && *pos < tokens.len() && !matches!(tokens[*pos].0, Token::Eof) {
        *pos += 1;
    }
}

/// Recovers after `parse_stmt` failed on a statement that began at `stmt_start`.
///
/// A compound statement (`if`, `try`, `foreach`, a closure-bearing expression, ...) whose
/// braced body contained an error fails only AFTER `parse_block` has recovered inside the body,
/// collected those errors and consumed the body's closing `}`. `pos` then already sits on a
/// clean boundary. Blind boundary recovery from there skipped the next token unconditionally
/// (consuming the enclosing block's `}`) or the next statement's keyword (descending into its
/// body), so one real error became a cascade of `Unexpected token ... RBrace/Catch` errors
/// that pointed at correct code. This keeps the boundary, skipping only the clauses of the
/// failed statement itself (`catch`/`finally`/`else`/`elseif`) and a trailing `;`.
pub(crate) fn recover_after_failed_stmt(
    tokens: &[SpannedToken],
    pos: &mut usize,
    stmt_start: usize,
) {
    // A declaration whose member failed (an error in a class's last method, say) returns with
    // its own body still open: the method's `}` was consumed, the class's was not. Close those
    // braces first, or the class's `}` is reported as an unexpected statement.
    close_braces_opened_since(tokens, pos, stmt_start);
    let previous = (*pos > stmt_start && *pos > 0)
        .then(|| tokens.get(*pos - 1).map(|(token, _)| token))
        .flatten();
    // A statement whose parser consumed its own `;` before rejecting it (an invalid
    // assignment target, for instance) also already stands on the next statement.
    if matches!(previous, Some(Token::Semicolon))
        && tokens.get(*pos).is_none_or(|(token, _)| is_clean_resume_token(token))
    {
        return;
    }
    if matches!(previous, Some(Token::RBrace)) {
        skip_trailing_compound_clauses(tokens, pos);
        match tokens.get(*pos).map(|(token, _)| token) {
            Some(Token::Semicolon) => {
                *pos += 1;
                return;
            }
            Some(token) if is_clean_resume_token(token) => return,
            None => return,
            _ => {}
        }
    }
    // The error was raised mid-expression: resume with the `(`/`[` nesting the statement had
    // already opened, so `[..., $a::X, 'k' => $v]` is skipped to its `;` instead of resuming
    // at `$v` (which reported a bogus "Expected '=' after variable name").
    let (paren_depth, bracket_depth) = open_delimiters_between(tokens, stmt_start, *pos);
    recover_with_open_delimiters(tokens, pos, paren_depth, bracket_depth);
}

/// Counts the `(` and `[` opened and not yet closed in `tokens[start..end]`, at the statement's
/// own brace level: delimiters left open inside a nested `{...}` that already closed (an
/// erroneous statement in a method body) belong to that block and are not inherited.
fn open_delimiters_between(tokens: &[SpannedToken], start: usize, end: usize) -> (usize, usize) {
    let mut levels = vec![(0usize, 0usize)];
    for (token, _) in tokens.iter().take(end).skip(start) {
        let level = levels.last_mut().expect("statement level");
        match token {
            Token::LParen => level.0 += 1,
            Token::RParen => level.0 = level.0.saturating_sub(1),
            Token::LBracket => level.1 += 1,
            Token::RBracket => level.1 = level.1.saturating_sub(1),
            Token::LBrace => levels.push((0, 0)),
            Token::RBrace if levels.len() > 1 => {
                levels.pop();
            }
            _ => {}
        }
    }
    levels.last().copied().unwrap_or((0, 0))
}

/// Skips `catch (...) {...}`, `finally {...}`, `else {...}` and `elseif (...) {...}` clauses
/// that belong to a compound statement whose earlier body failed to parse.
fn skip_trailing_compound_clauses(tokens: &[SpannedToken], pos: &mut usize) {
    while matches!(
        tokens.get(*pos).map(|(token, _)| token),
        Some(Token::Catch | Token::Finally | Token::Else | Token::ElseIf)
    ) {
        *pos += 1;
        let mut paren_depth = 0usize;
        // Advance to the clause's `{` (or, for a braceless `else stmt;`, its `;`).
        while let Some((token, _)) = tokens.get(*pos) {
            match token {
                Token::LParen => paren_depth += 1,
                Token::RParen => paren_depth = paren_depth.saturating_sub(1),
                Token::LBrace if paren_depth == 0 => break,
                Token::Semicolon if paren_depth == 0 => {
                    *pos += 1;
                    return;
                }
                Token::Eof => return,
                _ => {}
            }
            *pos += 1;
        }
        let mut brace_depth = 0usize;
        while let Some((token, _)) = tokens.get(*pos) {
            match token {
                Token::LBrace => brace_depth += 1,
                Token::RBrace => {
                    brace_depth = brace_depth.saturating_sub(1);
                    if brace_depth == 0 {
                        *pos += 1;
                        break;
                    }
                }
                Token::Eof => return,
                _ => {}
            }
            *pos += 1;
        }
    }
}

/// Returns true for a token where statement parsing can resume without skipping anything:
/// the end of the enclosing block or file, or the first token of a statement.
fn is_clean_resume_token(token: &Token) -> bool {
    matches!(
        token,
        Token::RBrace
            | Token::EndDeclare
            | Token::EndIf
            | Token::EndWhile
            | Token::EndFor
            | Token::EndForeach
            | Token::EndSwitch
            | Token::Eof
            | Token::Echo
            | Token::Print
            | Token::Variable(_)
            | Token::This
            | Token::PlusPlus
            | Token::MinusMinus
            | Token::Class
            | Token::Enum
            | Token::ReadOnly
            | Token::Interface
            | Token::Trait
            | Token::Abstract
            | Token::Final
            | Token::Function
            | Token::Namespace
            | Token::Use
            | Token::Declare
            | Token::Return
            | Token::Throw
            | Token::Include
            | Token::IncludeOnce
            | Token::Require
            | Token::RequireOnce
            | Token::Const
            | Token::Global
            | Token::Static
            | Token::Goto
            | Token::Identifier(_)
            | Token::Self_
            | Token::Parent
            | Token::Backslash
            | Token::Switch
            | Token::If
            | Token::Try
            | Token::While
            | Token::Do
            | Token::For
            | Token::Foreach
            | Token::Break
            | Token::Continue
            | Token::Case
            | Token::Default
            // Expression statements may also start with these (`[$a] = ...`, `(fn)()`, `@f()`,
            // `!$x && f()`, `-1 . f()`, `new Foo;`, a literal, ...): stopping short of them made
            // recovery swallow the following, correct statement.
            | Token::LParen
            | Token::LBracket
            | Token::LBrace
            | Token::At
            | Token::Bang
            | Token::Minus
            | Token::Plus
            | Token::Tilde
            | Token::Ampersand
            | Token::Semicolon
            | Token::StringLiteral(_)
            | Token::IntLiteral(_)
            | Token::FloatLiteral(_)
            | Token::True
            | Token::False
            | Token::Null
            | Token::New
            | Token::Clone
            | Token::Match
            | Token::Fn
            | Token::Yield
            | Token::AttrOpen
            | Token::Packed
            | Token::Extern
            | Token::IfDef
            | Token::Question
            | Token::DunderDir
            | Token::DunderFile
            | Token::DunderLine
            | Token::DunderFunction
            | Token::DunderClass
            | Token::DunderMethod
            | Token::DunderNamespace
            | Token::DunderTrait
    )
}

/// Advances `pos` past the `}` that close every `{` opened in `tokens[start..pos]`.
fn close_braces_opened_since(tokens: &[SpannedToken], pos: &mut usize, start: usize) {
    let mut open = 0usize;
    for (token, _) in tokens.iter().take(*pos).skip(start) {
        match token {
            Token::LBrace => open += 1,
            Token::RBrace => open = open.saturating_sub(1),
            _ => {}
        }
    }
    while open > 0 {
        match tokens.get(*pos).map(|(token, _)| token) {
            None | Some(Token::Eof) => return,
            Some(Token::LBrace) => open += 1,
            Some(Token::RBrace) => open -= 1,
            _ => {}
        }
        *pos += 1;
    }
}
