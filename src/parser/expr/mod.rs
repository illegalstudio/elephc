//! Purpose:
//! Provides expression parser entry points and shared argument parsing.
//! Coordinates Pratt parsing, assignment-value parsing, and call argument list parsing.
//!
//! Called from:
//! - `crate::parser::stmt`, `crate::parser::control`, and nested expression parsers.
//!
//! Key details:
//! - Assignment-value parsing intentionally permits assignment expressions where PHP syntax allows them.

mod assignment_targets;
mod calls;
mod prefix;
mod prefix_complex;
mod pratt;

pub(crate) use prefix::token_starts_prefix_expression;

use crate::errors::CompileError;
use crate::lexer::Token;
use crate::parser::ast::{Expr, ExprKind};
use crate::span::Span;

/// Parses a PHP expression using a Pratt parser, starting at binding power 0.
/// Returns the parsed expression or a compile error if syntax is invalid.
pub fn parse_expr(tokens: &[(Token, Span)], pos: &mut usize) -> Result<Expr, CompileError> {
    pratt::parse_expr_bp(tokens, pos, 0)
}

/// Parses an assignment-value expression (binding power 7), used in argument
/// positions, return statements, and other contexts where full expressions are
/// permitted. Assignment expressions are allowed here per PHP grammar rules.
pub(crate) fn parse_assignment_value_expr(
    tokens: &[(Token, Span)],
    pos: &mut usize,
) -> Result<Expr, CompileError> {
    pratt::parse_expr_bp(tokens, pos, 7)
}

/// Parses a `require`/`include` (optionally `_once`) used in expression position, returning an
/// `IncludeValue` marker expression without consuming a trailing semicolon. Returns `Ok(None)`
/// when the next token is not an include/require keyword so callers can use it speculatively.
///
/// PHP allows `include`/`require` in expression position (e.g. `if (true === (require_once X) || ...)`,
/// `f(require X)`, `$x = require X;`, `return require X;`). The expression evaluates to the
/// included file's `return` value, or `1` when the file has no explicit `return` (and `false`
/// for a missing non-required include). The included file runs in the calling scope. elephc
/// represents this with the transient `ExprKind::IncludeValue` marker, which the resolver
/// expands by inlining the included file's statements into the caller's scope and capturing its
/// top-level `return` into a hidden temporary. Statement-position includes (`require X;` as a
/// whole statement) are routed to `parse_include` before the expression parser runs, so this
/// helper only fires in true expression context.
pub(in crate::parser) fn parse_include_value_expr(
    tokens: &[(Token, Span)],
    pos: &mut usize,
) -> Result<Option<Expr>, CompileError> {
    let (once, required) = match tokens.get(*pos).map(|(token, _)| token) {
        Some(Token::Include) => (false, false),
        Some(Token::IncludeOnce) => (true, false),
        Some(Token::Require) => (false, true),
        Some(Token::RequireOnce) => (true, true),
        _ => return Ok(None),
    };
    let span = tokens[*pos].1;
    *pos += 1; // consume the include/require keyword

    let has_parens = *pos < tokens.len() && tokens[*pos].0 == Token::LParen;
    if has_parens {
        *pos += 1;
    }
    let path = parse_expr(tokens, pos)?;
    if has_parens {
        crate::parser::stmt::expect_token(tokens, pos, &Token::RParen, "Expected ')' after include path")?;
    }

    Ok(Some(Expr::new(
        ExprKind::IncludeValue {
            path: Box::new(path),
            once,
            required,
        },
        span,
    )))
}

/// Returns the name to use for a named-argument label, accepting identifiers and PHP
/// semi-reserved keywords (e.g. `f(array: 1)`); delegates to the shared bareword mapper.
fn argument_name_from_token(token: &Token) -> Option<String> {
    crate::parser::keyword_name::bareword_name_from_token(token)
}

/// Parse a comma-separated argument list. The opening `(` must already be consumed.
/// Consumes through the closing `)`.
pub(crate) fn parse_args(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    err_span: Span,
) -> Result<Vec<Expr>, CompileError> {
    let mut args = Vec::new();
    while *pos < tokens.len() && tokens[*pos].0 != Token::RParen {
        if !args.is_empty() {
            if tokens[*pos].0 != Token::Comma {
                return Err(CompileError::new(
                    tokens[*pos].1,
                    "Expected ',' between arguments",
                ));
            }
            *pos += 1;
            // Allow a trailing comma before the closing paren (PHP 7.3+).
            if *pos < tokens.len() && tokens[*pos].0 == Token::RParen {
                break;
            }
        }
        if *pos < tokens.len() && tokens[*pos].0 == Token::Ellipsis {
            let spread_span = tokens[*pos].1;
            *pos += 1;
            let inner = parse_expr(tokens, pos)?;
            args.push(Expr::new(ExprKind::Spread(Box::new(inner)), spread_span));
        } else if matches!(tokens.get(*pos + 1), Some((Token::Colon, _)))
            && argument_name_from_token(&tokens[*pos].0).is_some()
        {
            let arg_span = tokens[*pos].1;
            let name = argument_name_from_token(&tokens[*pos].0).unwrap();
            *pos += 2;
            let value = parse_expr(tokens, pos)?;
            args.push(Expr::new(
                ExprKind::NamedArg {
                    name,
                    value: Box::new(value),
                },
                arg_span,
            ));
        } else {
            args.push(parse_expr(tokens, pos)?);
        }
    }
    if *pos >= tokens.len() || tokens[*pos].0 != Token::RParen {
        return Err(CompileError::new(err_span, "Expected ')' after arguments"));
    }
    *pos += 1;
    Ok(args)
}
