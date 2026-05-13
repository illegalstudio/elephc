//! Purpose:
//! Parses expression suffixes that involve calls, casts, static receivers, and first-class callables.
//! Recognizes scoped static call forms and PHP cast syntax around grouped expressions.
//!
//! Called from:
//! - `crate::parser::expr::pratt` and `crate::parser::expr::prefix`.
//!
//! Key details:
//! - Callable and cast disambiguation depends on PHP token spelling and case-insensitive type names.

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
        Some(Token::Variable(property)) => {
            let property = property.clone();
            *pos += 1;
            return Ok(Expr::new(
                ExprKind::StaticPropertyAccess { receiver, property },
                span,
            ));
        }
        Some(Token::Class) => {
            *pos += 1;
            return Ok(Expr::new(ExprKind::ClassConstant { receiver }, span));
        }
        Some(Token::Identifier(method)) => {
            let method = method.clone();
            *pos += 1;
            method
        }
        _ => {
            return Err(CompileError::new(
                span,
                &format!("Expected method or property name after '{}::'", receiver_name),
            ))
        }
    };
    // If a `(` follows, this is a static method call; otherwise it's a
    // user-declared class-constant access (`MyClass::FOO`).
    if *pos >= tokens.len() || tokens[*pos].0 != Token::LParen {
        return Ok(Expr::new(
            ExprKind::ScopedConstantAccess {
                receiver,
                name: method,
            },
            span,
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
        Token::Identifier(name) if matches_case_insensitive(name, &["int", "integer"]) => {
            Some(CastType::Int)
        }
        Token::Identifier(name) if matches_case_insensitive(name, &["float", "double", "real"]) => {
            Some(CastType::Float)
        }
        Token::Identifier(name) if name.eq_ignore_ascii_case("string") => Some(CastType::String),
        Token::Identifier(name) if matches_case_insensitive(name, &["bool", "boolean"]) => {
            Some(CastType::Bool)
        }
        Token::Identifier(name) if name.eq_ignore_ascii_case("array") => Some(CastType::Array),
        _ => None,
    }
}

fn matches_case_insensitive(name: &str, keywords: &[&str]) -> bool {
    keywords
        .iter()
        .any(|keyword| name.eq_ignore_ascii_case(keyword))
}
