mod calls;
mod prefix;
mod prefix_complex;
mod pratt;

use crate::errors::CompileError;
use crate::lexer::Token;
use crate::parser::ast::{Expr, ExprKind};
use crate::span::Span;

pub fn parse_expr(tokens: &[(Token, Span)], pos: &mut usize) -> Result<Expr, CompileError> {
    pratt::parse_expr_bp(tokens, pos, 0)
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
        }
        if *pos < tokens.len() && tokens[*pos].0 == Token::Ellipsis {
            let spread_span = tokens[*pos].1;
            *pos += 1;
            let inner = parse_expr(tokens, pos)?;
            args.push(Expr::new(ExprKind::Spread(Box::new(inner)), spread_span));
        } else if matches!(tokens.get(*pos), Some((Token::Identifier(_), _)))
            && matches!(tokens.get(*pos + 1), Some((Token::Colon, _)))
        {
            let arg_span = tokens[*pos].1;
            let name = match &tokens[*pos].0 {
                Token::Identifier(name) => name.clone(),
                _ => unreachable!(),
            };
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
