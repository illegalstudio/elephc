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
