//! Purpose:
//! Parses PHP's alternative control-structure syntax (`:` … `endif;`, `endwhile;`,
//! `endfor;`, `endforeach;`, `endswitch;`) and shares the brace-vs-colon body decision
//! with every control statement parser.
//!
//! Called from:
//! - `crate::parser::control` for `if`/`while`/`for`/`foreach`/`switch` bodies.
//!
//! Key details:
//! - Alternative bodies desugar into exactly the same `StmtKind` bodies as the brace forms,
//!   so no later pass needs to distinguish them.
//! - Statement errors inside a segment are collected (like `parse_block`) so one broken
//!   statement does not hide the rest of the block.

use crate::errors::CompileError;
use crate::lexer::{SpannedToken, Token};
use crate::parser::ast::Stmt;
use crate::parser::stmt::{
    expect_semicolon, expect_token, parse_body, parse_stmt, recover_after_failed_stmt,
};

/// Tokens that end one segment of an alternative-syntax `if` (`then` and `elseif` bodies).
pub(crate) const IF_SEGMENT_STOPS: &[Token] = &[Token::ElseIf, Token::Else, Token::EndIf];

/// Returns true when the body starting at `pos` uses PHP's alternative `:` … `endX;` syntax.
pub(crate) fn starts_alternative_body(tokens: &[SpannedToken], pos: usize) -> bool {
    matches!(tokens.get(pos).map(|(token, _)| token), Some(Token::Colon))
}

/// Parses statements until one of `stop` (or EOF) is reached, leaving the stop token unconsumed.
///
/// Nested statement errors are collected and returned together, mirroring `parse_block`, so a
/// single malformed statement inside an alternative block still reports the following ones.
pub(crate) fn parse_alternative_stmts(
    tokens: &[SpannedToken],
    pos: &mut usize,
    stop: &[Token],
) -> Result<Vec<Stmt>, CompileError> {
    let mut body = Vec::new();
    let mut errors = Vec::new();

    while *pos < tokens.len()
        && tokens[*pos].0 != Token::Eof
        && !stop.contains(&tokens[*pos].0)
    {
        let stmt_start = *pos;
        match parse_stmt(tokens, pos) {
            Ok(stmt) => body.push(stmt),
            Err(error) => {
                errors.extend(error.flatten());
                recover_after_failed_stmt(tokens, pos, stmt_start);
            }
        }
    }

    if errors.is_empty() {
        Ok(body)
    } else {
        Err(CompileError::from_many(errors))
    }
}

/// Parses a control-structure body in either brace/single-statement form or PHP's alternative
/// `:` … `endX;` form, consuming the terminator keyword and its trailing `;` in the latter case.
///
/// `terminator` is the closing keyword token for this statement (e.g. `Token::EndWhile`) and
/// `keyword` its spelling, used only for the diagnostic when the block is left unterminated.
pub(crate) fn parse_control_body(
    tokens: &[SpannedToken],
    pos: &mut usize,
    terminator: &Token,
    keyword: &str,
) -> Result<Vec<Stmt>, CompileError> {
    if !starts_alternative_body(tokens, *pos) {
        return parse_body(tokens, pos);
    }

    *pos += 1;
    let body = parse_alternative_stmts(tokens, pos, std::slice::from_ref(terminator))?;
    close_alternative_block(tokens, pos, terminator, keyword)?;
    Ok(body)
}

/// Rejects an alternative-syntax (`:`) branch body opened inside a brace-form control structure.
///
/// PHP requires one `if` chain to use a single style throughout, so `if (…) { … } else: … endif;`
/// is a syntax error. Reporting it here names the mixing instead of leaving a bare "Unexpected
/// token: Colon" at the branch body.
pub(crate) fn reject_mixed_branch_body(
    tokens: &[SpannedToken],
    pos: usize,
    keyword: &str,
) -> Result<(), CompileError> {
    if !starts_alternative_body(tokens, pos) {
        return Ok(());
    }
    Err(CompileError::new(
        tokens[pos].1.span,
        &format!(
            "Cannot mix brace and alternative syntax in one if statement: '{}' opens a ':' body \
             but the 'if' used braces. Use either braces throughout or ':' … 'endif;' throughout",
            keyword
        ),
    ))
}

/// Returns the diagnostic for an `endif`/`endwhile`/`endfor`/`endforeach`/`endswitch` keyword
/// that appears where no alternative-syntax block is open.
///
/// `keyword` is the terminator's spelling, taken from the token itself so the message repeats
/// exactly what the source wrote.
pub(crate) fn unopened_terminator_error(keyword: &str, span: crate::span::Span) -> CompileError {
    CompileError::new(
        span,
        &format!(
            "Unexpected '{}': there is no open alternative-syntax block for it to close",
            keyword
        ),
    )
}

/// Consumes the `endX` terminator keyword and its mandatory `;`.
pub(crate) fn close_alternative_block(
    tokens: &[SpannedToken],
    pos: &mut usize,
    terminator: &Token,
    keyword: &str,
) -> Result<(), CompileError> {
    expect_token(
        tokens,
        pos,
        terminator,
        &format!("Expected '{}' to close the alternative-syntax block", keyword),
    )?;
    expect_semicolon(tokens, pos)
}
