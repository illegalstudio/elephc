use crate::errors::CompileError;
use crate::lexer::Token;
use crate::parser::ast::{CallableTarget, CastType, Expr, ExprKind, StaticReceiver};
use crate::span::Span;

use super::parse_args;

pub(super) fn parse_scoped_static_call(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
    receiver: StaticReceiver,
    receiver_name: &str,
) -> Result<Expr, CompileError> {
    if *pos >= tokens.len() || tokens[*pos].0 != Token::DoubleColon {
        return Err(CompileError::new(
            span,
            &format!("Expected '::' after '{}'", receiver_name),
        ));
    }
    *pos += 1;
    let method = match tokens.get(*pos).map(|(token, _)| token) {
        Some(Token::Identifier(method)) => {
            let method = method.clone();
            *pos += 1;
            method
        }
        _ => {
            return Err(CompileError::new(
                span,
                &format!("Expected method name after '{}::'", receiver_name),
            ))
        }
    };
    if *pos >= tokens.len() || tokens[*pos].0 != Token::LParen {
        return Err(CompileError::new(
            span,
            &format!("Expected '(' after {} method name", receiver_name),
        ));
    }
    *pos += 1;
    if parse_first_class_callable_parens(tokens, pos)? {
        Ok(Expr::new(
            ExprKind::FirstClassCallable(CallableTarget::StaticMethod { receiver, method }),
            span,
        ))
    } else {
        let args = parse_args(tokens, pos, span)?;
        Ok(Expr::new(
            ExprKind::StaticMethodCall {
                receiver,
                method,
                args,
            },
            span,
        ))
    }
}

pub(super) fn parse_first_class_callable_parens(
    tokens: &[(Token, Span)],
    pos: &mut usize,
) -> Result<bool, CompileError> {
    if *pos + 1 < tokens.len()
        && tokens[*pos].0 == Token::Ellipsis
        && tokens[*pos + 1].0 == Token::RParen
    {
        *pos += 2;
        return Ok(true);
    }
    Ok(false)
}

pub(super) fn peek_cast(tokens: &[(Token, Span)], pos: usize) -> Option<CastType> {
    if pos + 2 >= tokens.len() {
        return None;
    }
    if tokens[pos].0 != Token::LParen || tokens[pos + 2].0 != Token::RParen {
        return None;
    }
    match &tokens[pos + 1].0 {
        Token::Identifier(name) => match name.as_str() {
            "int" | "integer" => Some(CastType::Int),
            "float" | "double" | "real" => Some(CastType::Float),
            "string" => Some(CastType::String),
            "bool" | "boolean" => Some(CastType::Bool),
            "array" => Some(CastType::Array),
            _ => None,
        },
        _ => None,
    }
}
