//! Purpose:
//! Parses PHP list and bracket destructuring assignment patterns.
//! Validates positional, keyed, skipped, and append targets before lowering to ordinary statements.
//!
//! Called from:
//! - `crate::parser::stmt::parse_stmt()` through assignment dispatch.
//!
//! Key details:
//! - Pattern validation rejects malformed destructuring before lowerers synthesize temporary access expressions.

use crate::errors::CompileError;
use crate::lexer::Token;
use crate::parser::ast::{Expr, ExprKind, Stmt};
use crate::parser::expr::{parse_assignment_value_expr, parse_expr};
use crate::span::Span;

use super::super::{expect_semicolon, expect_token};

mod lower;

use lower::lower_list_unpack;

/// Parses a `list([]) = $x;` destructuring assignment statement.
/// Consumes the opening `[`, parses the bracket-enclosed pattern, expects `=`, parses the
/// right-hand side expression, and consumes the trailing semicolon. Returns the lowered statement.
pub(in crate::parser::stmt) fn parse_list_unpack(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    let pattern = parse_bracket_list_pattern(tokens, pos, span)?;

    expect_token(
        tokens,
        pos,
        &Token::Assign,
        "Expected '=' after list pattern",
    )?;

    let value = parse_assignment_value_expr(tokens, pos)?;
    expect_semicolon(tokens, pos)?;

    Ok(lower_list_unpack(pattern, value, span))
}

/// Parses a `list() = $x;` destructuring assignment statement using the `list()` construct syntax.
/// Consumes the `list` keyword, parses the parenthesized pattern, expects `=`, parses the
/// right-hand side expression, and consumes the trailing semicolon. Returns the lowered statement.
pub(in crate::parser::stmt) fn parse_list_construct_unpack(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    let pattern = parse_list_construct_pattern(tokens, pos, span)?;

    expect_token(
        tokens,
        pos,
        &Token::Assign,
        "Expected '=' after list pattern",
    )?;

    let value = parse_assignment_value_expr(tokens, pos)?;
    expect_semicolon(tokens, pos)?;

    Ok(lower_list_unpack(pattern, value, span))
}

/// Represents a list destructuring pattern with ordered entries.
#[derive(Debug, Clone)]
struct ListPattern {
    entries: Vec<ListEntry>,
}

/// A single entry in a list pattern: either skipped, or a keyed/unkeyed target.
#[derive(Debug, Clone)]
enum ListEntry {
    Skip,
    Target {
        key: Option<Expr>,
        target: ListTarget,
    },
}

/// The target of a list pattern entry: a plain expression, an append target (`$x[]`), or a nested list.
#[derive(Debug, Clone)]
enum ListTarget {
    Expr(Expr),
    Append(Expr),
    Nested(ListPattern),
}

/// Parses a bracket-enclosed list pattern (`[...]`). Delegates to `parse_delimited_list_pattern`
/// with `[` and `]` tokens.
fn parse_bracket_list_pattern(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<ListPattern, CompileError> {
    parse_delimited_list_pattern(tokens, pos, span, Token::LBracket, Token::RBracket, "]")
}

/// Parses a `list(...)` construct pattern (parenthesized `list` keyword). Consumes `list`,
/// then delegates to `parse_delimited_list_pattern` with `(` and `)` tokens.
fn parse_list_construct_pattern(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<ListPattern, CompileError> {
    *pos += 1; // consume list
    parse_delimited_list_pattern(tokens, pos, span, Token::LParen, Token::RParen, ")")
}

/// Parses the interior of a delimited list pattern (bracket or parenthesized). Expects the opening
/// delimiter at `*pos`, finds the matching close, slices the interior tokens, and calls
/// `parse_list_pattern_content`. Advances `*pos` past the closing delimiter.
fn parse_delimited_list_pattern(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
    open: Token,
    close: Token,
    close_label: &str,
) -> Result<ListPattern, CompileError> {
    if *pos >= tokens.len() || tokens[*pos].0 != open {
        return Err(CompileError::new(
            span,
            &format!("Expected '{}' after list", open_label(&open)),
        ));
    }
    let close_pos = find_matching_delimiter(tokens, *pos, &open, &close).ok_or_else(|| {
        CompileError::new(
            span,
            &format!("Expected '{}' after list pattern", close_label),
        )
    })?;
    let pattern = parse_list_pattern_content(&tokens[*pos + 1..close_pos], span)?;
    *pos = close_pos + 1;
    Ok(pattern)
}

/// Splits the token slice by top-level commas (respecting bracket/paren/brace nesting),
/// converts each segment to a `ListEntry`, and validates the resulting pattern.
/// Returns a `ListPattern` or an error if the pattern is malformed.
fn parse_list_pattern_content(
    tokens: &[(Token, Span)],
    span: Span,
) -> Result<ListPattern, CompileError> {
    let mut entries = Vec::new();
    let mut start = 0usize;
    let mut bracket_depth = 0usize;
    let mut paren_depth = 0usize;
    let mut brace_depth = 0usize;

    for i in 0..tokens.len() {
        let split = matches!(tokens[i].0, Token::Comma)
            && bracket_depth == 0
            && paren_depth == 0
            && brace_depth == 0;

        if split {
            let segment = &tokens[start..i];
            entries.push(parse_list_pattern_segment(segment, span)?);
            start = i + 1;
            continue;
        }

        match tokens[i].0 {
            Token::LBracket => bracket_depth += 1,
            Token::RBracket => bracket_depth = bracket_depth.saturating_sub(1),
            Token::LParen => paren_depth += 1,
            Token::RParen => paren_depth = paren_depth.saturating_sub(1),
            Token::LBrace => brace_depth += 1,
            Token::RBrace => brace_depth = brace_depth.saturating_sub(1),
            _ => {}
        }
    }
    if start < tokens.len() {
        entries.push(parse_list_pattern_segment(&tokens[start..], span)?);
    }

    let pattern = ListPattern { entries };
    validate_list_pattern(&pattern, span)?;
    Ok(pattern)
}

/// Converts a single comma-separated segment of a list pattern into a `ListEntry`.
/// Empty segment → `Skip`. Top-level `=>` present → keyed `Target`. Otherwise → unkeyed `Target`.
fn parse_list_pattern_segment(
    segment: &[(Token, Span)],
    span: Span,
) -> Result<ListEntry, CompileError> {
    if segment.is_empty() {
        return Ok(ListEntry::Skip);
    }

    if let Some(arrow) = find_top_level_double_arrow(segment) {
        if arrow == 0 {
            return Err(CompileError::new(span, "Expected key before '=>'"));
        }
        if arrow + 1 >= segment.len() {
            return Err(CompileError::new(span, "Expected target after '=>'"));
        }
        let key = parse_expr_from_slice(&segment[..arrow], span)?;
        let target = parse_list_target_from_slice(&segment[arrow + 1..], span)?;
        return Ok(ListEntry::Target {
            key: Some(key),
            target,
        });
    }

    Ok(ListEntry::Target {
        key: None,
        target: parse_list_target_from_slice(segment, span)?,
    })
}

/// Parses the target (right-hand side of `=>` or whole segment) into a `ListTarget`.
/// Handles nested `list()` constructs, bracket-wrapped nested patterns, append targets (`$x[]`),
/// and ordinary destructuring targets (variable, property, static property, or array access).
fn parse_list_target_from_slice(
    tokens: &[(Token, Span)],
    span: Span,
) -> Result<ListTarget, CompileError> {
    if tokens.is_empty() {
        return Err(CompileError::new(span, "Expected target in list unpacking"));
    }

    if is_wrapped_by(tokens, 0, Token::LBracket, Token::RBracket) {
        let nested = parse_list_pattern_content(&tokens[1..tokens.len() - 1], span)?;
        return Ok(ListTarget::Nested(nested));
    }

    if is_list_construct_slice(tokens) {
        let nested = parse_list_pattern_content(&tokens[2..tokens.len() - 1], span)?;
        return Ok(ListTarget::Nested(nested));
    }

    if tokens.len() >= 2
        && tokens[tokens.len() - 2].0 == Token::LBracket
        && tokens[tokens.len() - 1].0 == Token::RBracket
    {
        let base = parse_expr_from_slice(&tokens[..tokens.len() - 2], span)?;
        if is_append_target_base(&base) {
            return Ok(ListTarget::Append(base));
        }
        return Err(CompileError::new(span, "Invalid list destructuring target"));
    }

    let expr = parse_expr_from_slice(tokens, span)?;
    if is_list_destructuring_target(&expr) {
        Ok(ListTarget::Expr(expr))
    } else {
        Err(CompileError::new(
            span,
            "Invalid list destructuring target",
        ))
    }
}

/// Convenience wrapper around `parse_expr` that parses a full token slice and verifies all tokens
/// were consumed. Returns the parsed `Expr` or an error if parsing fails or tokens remain.
fn parse_expr_from_slice(tokens: &[(Token, Span)], span: Span) -> Result<Expr, CompileError> {
    let mut pos = 0usize;
    let expr = parse_expr(tokens, &mut pos)?;
    if pos != tokens.len() {
        return Err(CompileError::new(span, "Unexpected token in list pattern"));
    }
    Ok(expr)
}

/// Validates a parsed `ListPattern`: must have at least one target, and keyed entries may not
/// be mixed with unkeyed entries within the same list. Returns `Ok(())` or a compile error.
fn validate_list_pattern(pattern: &ListPattern, span: Span) -> Result<(), CompileError> {
    if list_pattern_target_count(pattern) == 0 {
        return Err(CompileError::new(span, "Cannot use empty list"));
    }

    let has_keyed = pattern
        .entries
        .iter()
        .any(|entry| matches!(entry, ListEntry::Target { key: Some(_), .. }));
    let has_unkeyed = pattern.entries.iter().any(|entry| {
        matches!(
            entry,
            ListEntry::Skip | ListEntry::Target { key: None, .. }
        )
    });
    if has_keyed && has_unkeyed {
        return Err(CompileError::new(
            span,
            "Cannot mix keyed and unkeyed list entries",
        ));
    }

    Ok(())
}

/// Counts the number of leaf destructuring targets in a nested `ListPattern`, recursing into
/// nested patterns. Skipped entries contribute 0; each `Target` contributes 1.
fn list_pattern_target_count(pattern: &ListPattern) -> usize {
    pattern
        .entries
        .iter()
        .map(|entry| match entry {
            ListEntry::Skip => 0,
            ListEntry::Target {
                target: ListTarget::Nested(pattern),
                ..
            } => list_pattern_target_count(pattern),
            ListEntry::Target { .. } => 1,
        })
        .sum()
}

/// Scans tokens for the top-level `=>` (double arrow) that is not inside brackets, parentheses,
/// or braces. Returns the token index of the arrow, or `None` if no top-level arrow exists.
fn find_top_level_double_arrow(tokens: &[(Token, Span)]) -> Option<usize> {
    let mut bracket_depth = 0usize;
    let mut paren_depth = 0usize;
    let mut brace_depth = 0usize;
    for (i, (token, _)) in tokens.iter().enumerate() {
        match token {
            Token::DoubleArrow if bracket_depth == 0 && paren_depth == 0 && brace_depth == 0 => {
                return Some(i);
            }
            Token::LBracket => bracket_depth += 1,
            Token::RBracket => bracket_depth = bracket_depth.saturating_sub(1),
            Token::LParen => paren_depth += 1,
            Token::RParen => paren_depth = paren_depth.saturating_sub(1),
            Token::LBrace => brace_depth += 1,
            Token::RBrace => brace_depth = brace_depth.saturating_sub(1),
            _ => {}
        }
    }
    None
}

/// Scans tokens starting at `open_pos` for the matching close delimiter, tracking nesting depth.
/// Returns the index of the matching close token, or `None` if no match is found.
fn find_matching_delimiter(
    tokens: &[(Token, Span)],
    open_pos: usize,
    open: &Token,
    close: &Token,
) -> Option<usize> {
    let mut depth = 0usize;
    for (i, (token, _)) in tokens.iter().enumerate().skip(open_pos) {
        if token == open {
            depth += 1;
        } else if token == close {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

/// Returns `true` if the token slice starting at `open_pos` opens with the given `open` token
/// and closes with the matching `close` token, with no inner tokens consuming the wrapper.
/// Uses `find_matching_delimiter` to verify the slice is a single balanced pair.
fn is_wrapped_by(tokens: &[(Token, Span)], open_pos: usize, open: Token, close: Token) -> bool {
    if tokens.get(open_pos).map(|(token, _)| token) != Some(&open) {
        return false;
    }
    find_matching_delimiter(tokens, open_pos, &open, &close) == Some(tokens.len() - 1)
}

/// Returns `true` if the token slice begins with a case-insensitive `list` identifier followed
/// by a parenthesized expression (i.e., `list(...)`). Checks the first token is `Identifier("list")`
/// and the second is `LParen`; the wrapper check `is_wrapped_by` validates balanced parens.
fn is_list_construct_slice(tokens: &[(Token, Span)]) -> bool {
    matches!(
        tokens,
        [(Token::Identifier(name), _), (Token::LParen, _), ..]
            if name.eq_ignore_ascii_case("list")
    ) && is_wrapped_by(tokens, 1, Token::LParen, Token::RParen)
}

/// Returns `true` if the expression is a valid list destructuring target: a variable,
/// property access, static property access, or an array access whose array is one of those forms.
fn is_list_destructuring_target(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Variable(_)
        | ExprKind::PropertyAccess { .. }
        | ExprKind::StaticPropertyAccess { .. } => true,
        ExprKind::ArrayAccess { array, .. } => matches!(
            &array.kind,
            ExprKind::Variable(_)
                | ExprKind::PropertyAccess { .. }
                | ExprKind::StaticPropertyAccess { .. }
        ),
        _ => false,
    }
}

/// Returns `true` if the expression is a valid base for an append target (`$x[] = ...`).
/// Must be a variable, property access, or static property access.
fn is_append_target_base(expr: &Expr) -> bool {
    matches!(
        &expr.kind,
        ExprKind::Variable(_)
            | ExprKind::PropertyAccess { .. }
            | ExprKind::StaticPropertyAccess { .. }
    )
}

/// Maps bracket and parenthesis open tokens to their textual label for error messages.
fn open_label(token: &Token) -> &'static str {
    match token {
        Token::LBracket => "[",
        Token::LParen => "(",
        _ => "",
    }
}
