//! Purpose:
//! Parses complex prefix expressions that need multi-token bodies or nested parser coordination.
//! Handles match expressions, closures, arrow functions, named expressions, and object construction.
//!
//! Called from:
//! - `crate::parser::expr::prefix::parse_prefix()`.
//!
//! Key details:
//! - Closure bodies and parameter defaults must preserve their own spans and PHP evaluation context.

use crate::errors::CompileError;
use crate::lexer::Token;
use crate::parser::ast::{
    CallableTarget, Expr, ExprKind, StaticReceiver, Stmt, StmtKind,
};
use crate::parser::stmt::{looks_like_typed_param, parse_block, parse_name, parse_type_expr};
use crate::span::Span;

use super::calls::parse_first_class_callable_parens;
use super::{parse_args, parse_expr};

pub(super) fn parse_match_expr(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Expr, CompileError> {
    *pos += 1;
    if *pos >= tokens.len() || tokens[*pos].0 != Token::LParen {
        return Err(CompileError::new(span, "Expected '(' after 'match'"));
    }
    *pos += 1;
    let subject = parse_expr(tokens, pos)?;
    if *pos >= tokens.len() || tokens[*pos].0 != Token::RParen {
        return Err(CompileError::new(span, "Expected ')' after match subject"));
    }
    *pos += 1;
    if *pos >= tokens.len() || tokens[*pos].0 != Token::LBrace {
        return Err(CompileError::new(span, "Expected '{' after match subject"));
    }
    *pos += 1;
    let mut arms = Vec::new();
    let mut default = None;
    while *pos < tokens.len() && tokens[*pos].0 != Token::RBrace {
        if tokens[*pos].0 == Token::Default {
            *pos += 1;
            if *pos >= tokens.len() || tokens[*pos].0 != Token::DoubleArrow {
                return Err(CompileError::new(span, "Expected '=>' after 'default'"));
            }
            *pos += 1;
            let result = parse_expr(tokens, pos)?;
            default = Some(Box::new(result));
            if *pos < tokens.len() && tokens[*pos].0 == Token::Comma {
                *pos += 1;
            }
        } else {
            let mut patterns = Vec::new();
            loop {
                patterns.push(parse_expr(tokens, pos)?);
                if *pos < tokens.len() && tokens[*pos].0 == Token::Comma {
                    let saved = *pos;
                    *pos += 1;
                    if *pos < tokens.len() && tokens[*pos].0 == Token::DoubleArrow {
                        *pos = saved;
                        break;
                    }
                } else {
                    break;
                }
            }
            if *pos >= tokens.len() || tokens[*pos].0 != Token::DoubleArrow {
                return Err(CompileError::new(span, "Expected '=>' in match arm"));
            }
            *pos += 1;
            let result = parse_expr(tokens, pos)?;
            arms.push((patterns, result));
            if *pos < tokens.len() && tokens[*pos].0 == Token::Comma {
                *pos += 1;
            }
        }
    }
    if *pos >= tokens.len() || tokens[*pos].0 != Token::RBrace {
        return Err(CompileError::new(span, "Expected '}' to close match"));
    }
    *pos += 1;
    Ok(Expr::new(
        ExprKind::Match {
            subject: Box::new(subject),
            arms,
            default,
        },
        span,
    ))
}

pub(super) fn parse_attributed_closure(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Expr, CompileError> {
    // PHP 8.0 allows `#[Foo] function() {…}`, `#[Foo] fn() => …`, and the
    // static variants. Attributes parse for shape only and are discarded.
    crate::parser::consume_attribute_lists(tokens, pos)?;
    let span = tokens.get(*pos).map(|(_, s)| *s).unwrap_or(span);
    match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Function) => parse_closure(tokens, pos, span, false),
        Some(Token::Fn) => parse_arrow_closure(tokens, pos, span, false),
        Some(Token::Static) => match tokens.get(*pos + 1).map(|(t, _)| t) {
            Some(Token::Function) => {
                *pos += 1;
                parse_closure(tokens, pos, span, true)
            }
            Some(Token::Fn) => {
                *pos += 1;
                parse_arrow_closure(tokens, pos, span, true)
            }
            _ => Err(CompileError::new(
                span,
                "Expected closure or arrow function after attribute group",
            )),
        },
        _ => Err(CompileError::new(
            span,
            "Expected closure or arrow function after attribute group",
        )),
    }
}

pub(super) fn parse_closure(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
    is_static: bool,
) -> Result<Expr, CompileError> {
    if *pos + 1 >= tokens.len() || tokens[*pos + 1].0 != Token::LParen {
        return Err(CompileError::new(span, "Unexpected token: Function"));
    }
    *pos += 2;
    let (params, variadic) = parse_closure_params(tokens, pos, span)?;
    let mut captures = Vec::new();
    let mut capture_refs = Vec::new();
    if *pos < tokens.len() && tokens[*pos].0 == Token::Use {
        *pos += 1;
        if *pos >= tokens.len() || tokens[*pos].0 != Token::LParen {
            return Err(CompileError::new(span, "Expected '(' after 'use'"));
        }
        *pos += 1;
        while *pos < tokens.len() && tokens[*pos].0 != Token::RParen {
            if !captures.is_empty() {
                if tokens[*pos].0 != Token::Comma {
                    return Err(CompileError::new(
                        tokens[*pos].1,
                        "Expected ',' between captured variables",
                    ));
                }
                *pos += 1;
            }
            let is_ref = if tokens.get(*pos).map(|(token, _)| token) == Some(&Token::Ampersand) {
                *pos += 1;
                true
            } else {
                false
            };
            match tokens.get(*pos).map(|(token, _)| token) {
                Some(Token::Variable(name)) => {
                    if is_ref {
                        capture_refs.push(name.clone());
                    }
                    captures.push(name.clone());
                    *pos += 1;
                }
                _ => {
                    return Err(CompileError::new(
                        span,
                        "Expected variable in use() capture list",
                    ))
                }
            }
        }
        if *pos >= tokens.len() || tokens[*pos].0 != Token::RParen {
            return Err(CompileError::new(
                span,
                "Expected ')' after use() capture list",
            ));
        }
        *pos += 1;
    }
    let return_type = parse_optional_closure_return_type(tokens, pos, span)?;
    let body = parse_block(tokens, pos)?;
    Ok(Expr::new(
        ExprKind::Closure {
            params,
            variadic,
            return_type,
            body,
            is_arrow: false,
            is_static,
            captures,
            capture_refs,
        },
        span,
    ))
}

pub(super) fn parse_arrow_closure(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
    is_static: bool,
) -> Result<Expr, CompileError> {
    *pos += 1;
    if *pos >= tokens.len() || tokens[*pos].0 != Token::LParen {
        return Err(CompileError::new(span, "Expected '(' after 'fn'"));
    }
    *pos += 1;
    let (params, variadic) = parse_closure_params(tokens, pos, span)?;
    let return_type = parse_optional_closure_return_type(tokens, pos, span)?;
    if *pos >= tokens.len() || tokens[*pos].0 != Token::DoubleArrow {
        return Err(CompileError::new(
            span,
            "Expected '=>' after arrow function parameters",
        ));
    }
    *pos += 1;
    let body_expr = parse_expr(tokens, pos)?;
    let body = vec![Stmt::new(StmtKind::Return(Some(body_expr)), span)];
    Ok(Expr::new(
        ExprKind::Closure {
            params,
            variadic,
            return_type,
            body,
            is_arrow: true,
            is_static,
            captures: vec![],
            capture_refs: vec![],
        },
        span,
    ))
}

fn parse_optional_closure_return_type(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Option<crate::parser::ast::TypeExpr>, CompileError> {
    if *pos < tokens.len() && tokens[*pos].0 == Token::Colon {
        *pos += 1;
        Ok(Some(parse_type_expr(tokens, pos, span)?))
    } else {
        Ok(None)
    }
}

fn parse_closure_params(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<
    (
        Vec<(String, Option<crate::parser::ast::TypeExpr>, Option<Expr>, bool)>,
        Option<String>,
    ),
    CompileError,
> {
    let mut params = Vec::new();
    let mut variadic = None;
    while *pos < tokens.len() && tokens[*pos].0 != Token::RParen {
        if !params.is_empty() || variadic.is_some() {
            if tokens[*pos].0 != Token::Comma {
                return Err(CompileError::new(
                    tokens[*pos].1,
                    "Expected ',' between parameters",
                ));
            }
            *pos += 1;
        }
        // PHP 8.0 closure-parameter attributes (`fn(#[X] $a) => …`).
        crate::parser::consume_attribute_lists(tokens, pos)?;
        if variadic.is_some() {
            return Err(CompileError::new(
                span,
                "Variadic parameter must be the last parameter",
            ));
        }
        let type_ann = if looks_like_typed_param(tokens, *pos) {
            Some(parse_type_expr(tokens, pos, span)?)
        } else {
            None
        };
        let is_ref = if *pos < tokens.len() && tokens[*pos].0 == Token::Ampersand {
            *pos += 1;
            true
        } else {
            false
        };
        if *pos < tokens.len() && tokens[*pos].0 == Token::Ellipsis {
            if type_ann.is_some() {
                return Err(CompileError::new(
                    span,
                    "Typed variadic parameters are not supported yet",
                ));
            }
            *pos += 1;
            match tokens.get(*pos).map(|(token, _)| token) {
                Some(Token::Variable(name)) => {
                    variadic = Some(name.clone());
                    *pos += 1;
                }
                _ => return Err(CompileError::new(span, "Expected variable after '...'")),
            }
            continue;
        }
        match tokens.get(*pos).map(|(token, _)| token) {
            Some(Token::Variable(name)) => {
                let name = name.clone();
                *pos += 1;
                let default = if *pos < tokens.len() && tokens[*pos].0 == Token::Assign {
                    *pos += 1;
                    Some(parse_expr(tokens, pos)?)
                } else {
                    None
                };
                params.push((name, type_ann, default, is_ref));
            }
            _ => return Err(CompileError::new(span, "Expected parameter variable")),
        }
    }
    if *pos >= tokens.len() || tokens[*pos].0 != Token::RParen {
        return Err(CompileError::new(span, "Expected ')' after parameters"));
    }
    *pos += 1;
    Ok((params, variadic))
}

pub(super) fn parse_named_expr(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Expr, CompileError> {
    let name = parse_name(tokens, pos, span, "Expected name")?;
    if name.parts.len() == 1
        && name.parts[0] == "buffer_new"
        && *pos < tokens.len()
        && tokens[*pos].0 == Token::Less
    {
        *pos += 1;
        let element_type = parse_type_expr(tokens, pos, span)?;
        if *pos >= tokens.len() || tokens[*pos].0 != Token::Greater {
            return Err(CompileError::new(span, "Expected '>' after buffer_new<T"));
        }
        *pos += 1;
        if *pos >= tokens.len() || tokens[*pos].0 != Token::LParen {
            return Err(CompileError::new(span, "Expected '(' after buffer_new<T>"));
        }
        *pos += 1;
        let len = parse_expr(tokens, pos)?;
        if *pos >= tokens.len() || tokens[*pos].0 != Token::RParen {
            return Err(CompileError::new(
                span,
                "Expected ')' after buffer_new length",
            ));
        }
        *pos += 1;
        return Ok(Expr::new(
            ExprKind::BufferNew {
                element_type,
                len: Box::new(len),
            },
            span,
        ));
    }
    if name.parts.len() == 1
        && name.parts[0] == "ptr_cast"
        && *pos < tokens.len()
        && tokens[*pos].0 == Token::Less
    {
        *pos += 1;
        let target_type = parse_name(tokens, pos, span, "Expected type name after 'ptr_cast<'")?
            .as_canonical();
        if *pos >= tokens.len() || tokens[*pos].0 != Token::Greater {
            return Err(CompileError::new(span, "Expected '>' after ptr_cast<T"));
        }
        *pos += 1;
        if *pos >= tokens.len() || tokens[*pos].0 != Token::LParen {
            return Err(CompileError::new(span, "Expected '(' after ptr_cast<T>"));
        }
        *pos += 1;
        let expr = parse_expr(tokens, pos)?;
        if *pos >= tokens.len() || tokens[*pos].0 != Token::RParen {
            return Err(CompileError::new(
                span,
                "Expected ')' after ptr_cast argument",
            ));
        }
        *pos += 1;
        return Ok(Expr::new(
            ExprKind::PtrCast {
                target_type,
                expr: Box::new(expr),
            },
            span,
        ));
    }
    if *pos < tokens.len() && tokens[*pos].0 == Token::LParen {
        *pos += 1;
        if parse_first_class_callable_parens(tokens, pos)? {
            Ok(Expr::new(
                ExprKind::FirstClassCallable(CallableTarget::Function(name)),
                span,
            ))
        } else {
            let args = parse_args(tokens, pos, span)?;
            Ok(Expr::new(ExprKind::FunctionCall { name, args }, span))
        }
    } else if *pos < tokens.len() && tokens[*pos].0 == Token::DoubleColon {
        *pos += 1;
        let member = match tokens.get(*pos).map(|(token, _)| token) {
            Some(Token::Variable(property)) => {
                let property = property.clone();
                *pos += 1;
                return Ok(Expr::new(
                    ExprKind::StaticPropertyAccess {
                        receiver: StaticReceiver::Named(name),
                        property,
                    },
                    span,
                ));
            }
            Some(Token::Class) => {
                *pos += 1;
                return Ok(Expr::new(
                    ExprKind::ClassConstant {
                        receiver: StaticReceiver::Named(name),
                    },
                    span,
                ));
            }
            Some(Token::Identifier(member)) => {
                let member = member.clone();
                *pos += 1;
                member
            }
            _ => return Err(CompileError::new(span, "Expected member name after '::'")),
        };
        if *pos < tokens.len() && tokens[*pos].0 == Token::LParen {
            *pos += 1;
            if parse_first_class_callable_parens(tokens, pos)? {
                Ok(Expr::new(
                    ExprKind::FirstClassCallable(CallableTarget::StaticMethod {
                        receiver: StaticReceiver::Named(name),
                        method: member,
                    }),
                    span,
                ))
            } else {
                let args = parse_args(tokens, pos, span)?;
                Ok(Expr::new(
                    ExprKind::StaticMethodCall {
                        receiver: StaticReceiver::Named(name),
                        method: member,
                        args,
                    },
                    span,
                ))
            }
        } else {
            // `Foo::BAR` (no parens) is either an enum case access or a
            // user-declared class constant. Disambiguation is done by the
            // type checker, which falls back from enum lookup to class const.
            Ok(Expr::new(
                ExprKind::ScopedConstantAccess {
                    receiver: StaticReceiver::Named(name),
                    name: member,
                },
                span,
            ))
        }
    } else {
        Ok(Expr::new(ExprKind::ConstRef(name), span))
    }
}

pub(super) fn parse_new_object(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Expr, CompileError> {
    *pos += 1;

    // `new self()`, `new static()`, `new parent()` — late-static-binding
    // factory pattern. Parsed as a NewScopedObject so codegen can apply LSB
    // for `static`.
    let scoped_receiver = match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Self_) => Some(StaticReceiver::Self_),
        Some(Token::Static) => Some(StaticReceiver::Static),
        Some(Token::Parent) => Some(StaticReceiver::Parent),
        _ => None,
    };
    if let Some(receiver) = scoped_receiver {
        *pos += 1;
        if *pos >= tokens.len() || tokens[*pos].0 != Token::LParen {
            return Err(CompileError::new(
                span,
                "Expected '(' after self/static/parent",
            ));
        }
        *pos += 1;
        let args = parse_args(tokens, pos, span)?;
        return Ok(Expr::new(
            ExprKind::NewScopedObject { receiver, args },
            span,
        ));
    }

    let class_name = parse_name(tokens, pos, span, "Expected class name after 'new'")?;
    if *pos >= tokens.len() || tokens[*pos].0 != Token::LParen {
        return Err(CompileError::new(span, "Expected '(' after class name"));
    }
    *pos += 1;
    let args = parse_args(tokens, pos, span)?;
    Ok(Expr::new(ExprKind::NewObject { class_name, args }, span))
}
